use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource, PermissionType};

use super::{
    common::{base_state, event_name, message_from, permission_from_tool, string_at},
    AgentAdapter,
};

pub struct OpenCodeAdapter;

impl AgentAdapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["opencode"]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        let mut state = base_state(self.kind(), &value);
        state.source = DetectionSource::StructuredEvent;
        let name = event_name(&value);
        match name.as_str() {
            "permission.asked" => {
                state.status = AgentStatus::PermissionRequired;
                let permission = string_at(&value, &["properties", "permission"])
                    .or_else(|| string_at(&value, &["permission"]))
                    .unwrap_or_default();
                state.permission = Some(permission_from_opencode(&permission));
            }
            "permission.replied" => state.status = AgentStatus::Working,
            "session.idle" => state.status = AgentStatus::Done,
            "session.error" => state.status = AgentStatus::Error,
            "session.created" => state.status = AgentStatus::Idle,
            "session.status" => {
                let status = string_at(&value, &["properties", "status", "type"])
                    .or_else(|| string_at(&value, &["status", "type"]))
                    .or_else(|| string_at(&value, &["status"]))
                    .unwrap_or_default();
                state.status = match status.as_str() {
                    "busy" => AgentStatus::Working,
                    "idle" => AgentStatus::Idle,
                    "retry" => AgentStatus::Working,
                    _ => AgentStatus::Unknown,
                };
            }
            "message.updated"
            | "message.part.updated"
            | "tool.execute.before"
            | "tool.execute.after" => state.status = AgentStatus::Working,
            _ => state.status = AgentStatus::Unknown,
        }
        if matches!(name.as_str(), "permission.asked" | "session.error") {
            state.message = message_from(&value);
        }
        Some(state)
    }
}

fn permission_from_opencode(permission: &str) -> PermissionType {
    match permission.to_ascii_lowercase().as_str() {
        "edit" | "patch" => PermissionType::FileEdit,
        "write" => PermissionType::FileWrite,
        "bash" | "shell" => PermissionType::ShellCommand,
        "webfetch" | "network" | "external_directory" => PermissionType::Network,
        other => permission_from_tool(other),
    }
}
