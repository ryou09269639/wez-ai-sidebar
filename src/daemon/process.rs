use std::collections::HashMap;

use crate::{state::AgentKind, wezterm::WeztermPane};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedAgent {
    pub kind: AgentKind,
    pub pid: u32,
}

#[derive(Debug, Default)]
pub struct ProcessSnapshot {
    pub available: bool,
    pub by_pane: HashMap<u64, DetectedAgent>,
}

pub async fn detect(panes: &[WeztermPane]) -> ProcessSnapshot {
    let panes = panes.to_vec();
    tokio::task::spawn_blocking(move || detect_blocking(&panes))
        .await
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn detect_blocking(panes: &[WeztermPane]) -> ProcessSnapshot {
    use std::{fs, path::PathBuf};

    let tty_to_pane = panes
        .iter()
        .filter_map(|pane| {
            pane.tty_name
                .as_ref()
                .map(|tty| (PathBuf::from(tty), pane.pane_id))
        })
        .collect::<HashMap<_, _>>();
    let Ok(entries) = fs::read_dir("/proc") else {
        return ProcessSnapshot::default();
    };
    let mut snapshot = ProcessSnapshot {
        available: true,
        by_pane: HashMap::new(),
    };
    if tty_to_pane.is_empty() {
        return snapshot;
    }

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(tty) = process_tty(pid) else {
            continue;
        };
        let Some(&pane_id) = tty_to_pane.get(&tty) else {
            continue;
        };
        let Some(kind) = read_argv(pid).as_deref().and_then(kind_from_argv) else {
            continue;
        };
        record(&mut snapshot, pane_id, kind, pid);
    }
    snapshot
}

#[cfg(target_os = "macos")]
fn detect_blocking(panes: &[WeztermPane]) -> ProcessSnapshot {
    let tty_to_pane = panes
        .iter()
        .filter_map(|pane| {
            pane.tty_name
                .as_ref()
                .map(|tty| (macos_tty_key(tty), pane.pane_id))
        })
        .collect::<HashMap<_, _>>();
    let Some(listing) = run_ps() else {
        return ProcessSnapshot::default();
    };
    let mut snapshot = ProcessSnapshot {
        available: true,
        by_pane: HashMap::new(),
    };
    if tty_to_pane.is_empty() {
        return snapshot;
    }

    for line in listing.lines() {
        let Some((pid, tty, args)) = parse_ps_line(line) else {
            continue;
        };
        let Some(tty) = tty else { continue };
        let Some(&pane_id) = tty_to_pane.get(&tty) else {
            continue;
        };
        let argv = args
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let Some(kind) = kind_from_argv(&argv) else {
            continue;
        };
        record(&mut snapshot, pane_id, kind, pid);
    }
    snapshot
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn detect_blocking(_panes: &[WeztermPane]) -> ProcessSnapshot {
    ProcessSnapshot::default()
}

fn record(snapshot: &mut ProcessSnapshot, pane_id: u64, kind: AgentKind, pid: u32) {
    snapshot
        .by_pane
        .entry(pane_id)
        .and_modify(|detected| {
            if pid < detected.pid {
                *detected = DetectedAgent { kind, pid };
            }
        })
        .or_insert(DetectedAgent { kind, pid });
}

#[cfg(target_os = "linux")]
fn process_tty(pid: u32) -> Option<std::path::PathBuf> {
    (0..=2)
        .filter_map(|fd| std::fs::read_link(format!("/proc/{pid}/fd/{fd}")).ok())
        .find(|target| {
            target.starts_with("/dev/pts")
                || target.starts_with("/dev/tty")
                || target.starts_with("/dev/console")
        })
}

#[cfg(target_os = "linux")]
fn read_argv(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect::<Vec<_>>();
    (!argv.is_empty()).then_some(argv)
}

// macOS has no /proc. `ps` is the documented, stable way to read another
// process's tty and argv without linking libproc/sysctl bindings.
#[cfg(target_os = "macos")]
fn run_ps() -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,tty=,args="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn read_argv_macos(pid: u32) -> Option<Vec<String>> {
    let output = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.split_whitespace().map(str::to_owned).collect())
}

// `ps -axo pid=,tty=,args=` pads columns with spaces, so a naive split on
// whitespace would fragment `args`. Each field is taken from the front of
// the line instead, leaving `args` with its internal spacing intact.
#[cfg(target_os = "macos")]
fn parse_ps_line(line: &str) -> Option<(u32, Option<String>, String)> {
    let mut remainder = line;
    let pid = take_token(&mut remainder)?.parse::<u32>().ok()?;
    let tty_token = take_token(&mut remainder)?;
    let tty = (tty_token != "??").then(|| macos_tty_key(tty_token));
    Some((pid, tty, remainder.trim_start().to_owned()))
}

#[cfg(target_os = "macos")]
fn take_token<'a>(input: &mut &'a str) -> Option<&'a str> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let (token, rest) = trimmed.split_at(end);
    *input = rest;
    Some(token)
}

// WezTerm reports the full device path (e.g. `/dev/ttys004`) while `ps -o
// tty` reports the bare name (`ttys004`); normalize both to the bare name.
#[cfg(target_os = "macos")]
fn macos_tty_key(tty: &str) -> String {
    tty.trim_start_matches("/dev/").to_owned()
}

pub fn pid_matches_agent(pid: u32, expected: AgentKind) -> bool {
    #[cfg(target_os = "linux")]
    {
        read_argv(pid).as_deref().and_then(kind_from_argv) == Some(expected)
    }
    #[cfg(target_os = "macos")]
    {
        read_argv_macos(pid).as_deref().and_then(kind_from_argv) == Some(expected)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (pid, expected);
        false
    }
}

fn kind_from_argv(argv: &[String]) -> Option<AgentKind> {
    let executable = argv.first()?.to_ascii_lowercase();
    let basename = std::path::Path::new(&executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&executable);
    match basename.trim_end_matches(".exe") {
        "claude" => Some(AgentKind::Claude),
        "opencode" => Some(AgentKind::OpenCode),
        "codex" => Some(AgentKind::Codex),
        "copilot" | "github-copilot-cli" => Some(AgentKind::Copilot),
        "antigravity" | "agy" => Some(AgentKind::Antigravity),
        "kimi" | "kimi-cli" => Some(AgentKind::Kimi),
        "node" | "nodejs" => kind_from_node_script(argv.get(1)?),
        _ => None,
    }
}

fn kind_from_node_script(script: &str) -> Option<AgentKind> {
    let normalized = script.to_ascii_lowercase().replace('\\', "/");
    if normalized.contains("/@openai/codex/") || normalized.ends_with("/codex.js") {
        Some(AgentKind::Codex)
    } else if normalized.contains("/@anthropic-ai/claude-code/") {
        Some(AgentKind::Claude)
    } else if normalized.contains("/opencode/") || normalized.ends_with("/opencode.js") {
        Some(AgentKind::OpenCode)
    } else if normalized.contains("github-copilot-cli") {
        Some(AgentKind::Copilot)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn detects_native_and_node_codex_processes() {
        assert_eq!(
            kind_from_argv(&argv(&["/usr/local/bin/codex", "--no-alt-screen"])),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            kind_from_argv(&argv(&[
                "node",
                "/usr/lib/node_modules/@openai/codex/bin/codex.js"
            ])),
            Some(AgentKind::Codex)
        );
    }

    #[test]
    fn does_not_treat_a_prompt_or_project_path_as_codex() {
        assert_eq!(
            kind_from_argv(&argv(&["zsh", "-c", "please run codex here"])),
            None
        );
        assert_eq!(
            kind_from_argv(&argv(&["cargo", "test", "wez-ai-sidebar/codex"])),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_a_padded_ps_line_and_keeps_arg_spacing() {
        let (pid, tty, args) =
            parse_ps_line("  4242 ttys004   /usr/local/bin/codex --no-alt-screen").unwrap();
        assert_eq!(pid, 4242);
        assert_eq!(tty.as_deref(), Some("ttys004"));
        assert_eq!(args, "/usr/local/bin/codex --no-alt-screen");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn treats_a_double_question_mark_tty_as_absent() {
        let (_, tty, _) = parse_ps_line("100 ??       /sbin/launchd").unwrap();
        assert_eq!(tty, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn normalizes_wezterm_and_ps_tty_names_to_the_same_key() {
        assert_eq!(macos_tty_key("/dev/ttys004"), macos_tty_key("ttys004"));
    }
}
