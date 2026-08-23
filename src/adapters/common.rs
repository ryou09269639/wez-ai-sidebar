use std::env;

use chrono::Utc;
use regex::Regex;
use serde_json::Value;

use crate::state::{
    AgentKind, AgentState, AgentStatus, DetectionSource, PermissionType, WeztermLocation,
};

use super::AgentAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStatus {
    pub status: AgentStatus,
    pub permission: Option<PermissionType>,
    pub message: Option<String>,
}

pub struct GenericAdapter {
    kind: AgentKind,
}

impl GenericAdapter {
    pub fn new(kind: AgentKind) -> Self {
        Self { kind }
    }
}

impl AgentAdapter for GenericAdapter {
    fn name(&self) -> &'static str {
        self.kind.display_name()
    }

    fn kind(&self) -> AgentKind {
        self.kind
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &[]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        Some(base_state(self.kind, &value))
    }
}

pub fn base_state(kind: AgentKind, value: &Value) -> AgentState {
    let cwd = string_at(value, &["cwd"])
        .or_else(|| string_at(value, &["directory"]))
        .or_else(|| string_at(value, &["worktree"]))
        .or_else(|| string_at(value, &["workspacePaths", "0"]))
        .or_else(|| env::var("PWD").ok())
        .unwrap_or_else(|| "unknown".to_owned());
    let id = session_id(value).unwrap_or_else(|| {
        let pane = env::var("WEZTERM_PANE").unwrap_or_else(|_| "outside-wezterm".to_owned());
        format!("{}-{}", pane, process_id())
    });
    let mut state = AgentState::new(id, kind, cwd);
    state.wezterm = WeztermLocation {
        pane_id: env::var("WEZTERM_PANE").ok().and_then(|id| id.parse().ok()),
        ..WeztermLocation::default()
    };
    state.pid = Some(parent_pid());
    state.source = DetectionSource::Hook;
    state.updated_at = Utc::now();
    state
}

pub fn event_name(value: &Value) -> String {
    string_at(value, &["_wez_event"])
        .or_else(|| string_at(value, &["hook_event_name"]))
        .or_else(|| string_at(value, &["event", "type"]))
        .or_else(|| string_at(value, &["type"]))
        .unwrap_or_default()
}

pub fn session_id(value: &Value) -> Option<String> {
    [
        &["session_id"][..],
        &["sessionId"][..],
        &["conversationId"][..],
        &["sessionID"][..],
        &["properties", "sessionID"][..],
        &["properties", "info", "id"][..],
        &["data", "sessionID"][..],
    ]
    .iter()
    .find_map(|path| string_at(value, path))
}

pub fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        if let Ok(index) = key.parse::<usize>() {
            current = current.as_array()?.get(index)?;
        } else {
            current = current.get(*key)?;
        }
    }
    match current {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

pub fn permission_from_tool(tool: &str) -> PermissionType {
    let normalized = tool.to_ascii_lowercase();
    if normalized.contains("bash")
        || normalized.contains("shell")
        || normalized.contains("command")
        || normalized == "run_command"
    {
        PermissionType::ShellCommand
    } else if normalized.contains("write") || normalized == "write_to_file" {
        PermissionType::FileWrite
    } else if normalized.contains("edit")
        || normalized.contains("patch")
        || normalized.contains("replace_file")
    {
        PermissionType::FileEdit
    } else if normalized.contains("network")
        || normalized.contains("web")
        || normalized.contains("url")
    {
        PermissionType::Network
    } else if normalized.contains("mcp") {
        PermissionType::Mcp
    } else {
        PermissionType::Other
    }
}

pub fn message_from(value: &Value) -> Option<String> {
    [
        &["message"][..],
        &["title"][..],
        &["body"][..],
        &["reason"][..],
        &["tool_input", "description"][..],
        &["properties", "message"][..],
        // OpenCode wraps every session.error cause in `{name, data}` (see
        // NamedError.toObject in its source), so the message lives one level
        // deeper than a plain `error.message` shape.
        &["properties", "error", "data", "message"][..],
        &["properties", "error", "message"][..],
        &["error"][..],
    ]
    .iter()
    .find_map(|path| string_at(value, path))
    .and_then(AgentState::short_message)
}

pub fn parse_terminal_output(text: &str) -> Option<ParsedStatus> {
    let tail = text.lines().rev().take(40).collect::<Vec<_>>().join("\n");
    let patterns = [
        (
            r"(?i)(allow (this |the )?(bash |shell )?command|approve command|command.*permission|required permission.*command)",
            AgentStatus::PermissionRequired,
            Some(PermissionType::ShellCommand),
        ),
        (
            r"(?i)(allow (editing|edit|writing|write)|approve (edit|patch)|file (edit|write).*permission)",
            AgentStatus::PermissionRequired,
            Some(PermissionType::FileEdit),
        ),
        (
            r"(?i)(network access.*(allow|approve|permission)|allow network)",
            AgentStatus::PermissionRequired,
            Some(PermissionType::Network),
        ),
        (
            r"(?i)(permission required|do you want to proceed|approve\?|allow\?)",
            AgentStatus::PermissionRequired,
            Some(PermissionType::Other),
        ),
        (
            r"(?i)(waiting for (your )?input|enter your choice|select an option|answer the question)",
            AgentStatus::WaitingInput,
            None,
        ),
        (
            r"(?i)(fatal error|authentication failed|rate limit exceeded|execution failed)",
            AgentStatus::Error,
            None,
        ),
        (
            r"(?i)(task completed|agent finished|completed successfully)",
            AgentStatus::Done,
            None,
        ),
    ];
    for (pattern, status, permission) in patterns {
        let regex = Regex::new(pattern).ok()?;
        if let Some(found) = regex.find(&tail) {
            return Some(ParsedStatus {
                status,
                permission,
                // Never retain the rest of a terminal line: it may include a
                // command, URL, token, or other secret. The matched phrase is
                // enough to explain why the state changed.
                message: AgentState::short_message(found.as_str()),
            });
        }
    }
    None
}

fn parent_pid() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: getppid has no preconditions and does not dereference memory.
        unsafe { libc::getppid() as u32 }
    }
    #[cfg(not(unix))]
    {
        process_id()
    }
}

fn process_id() -> u32 {
    std::process::id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_parser_prioritizes_shell_permission() {
        let parsed = parse_terminal_output("Working...\nAllow this Bash command?\n").unwrap();
        assert_eq!(parsed.status, AgentStatus::PermissionRequired);
        assert_eq!(parsed.permission, Some(PermissionType::ShellCommand));
    }

    #[test]
    fn terminal_parser_detects_waiting_input() {
        let parsed = parse_terminal_output("Please select an option to continue").unwrap();
        assert_eq!(parsed.status, AgentStatus::WaitingInput);
    }
}
