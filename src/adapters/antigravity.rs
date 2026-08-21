use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource};

use super::{
    common::{base_state, bool_at, event_name, message_from, permission_from_tool, string_at},
    AgentAdapter,
};

pub struct AntigravityAdapter;

impl AgentAdapter for AntigravityAdapter {
    fn name(&self) -> &'static str {
        "Google Antigravity CLI"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Antigravity
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["agy"]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        let mut state = base_state(self.kind(), &value);
        state.source = DetectionSource::Hook;
        let name = event_name(&value);
        match name.as_str() {
            "PreInvocation" | "PostInvocation" | "PreToolUse" | "PostToolUse" => {
                state.status = AgentStatus::Working;
            }
            "Stop" => {
                let reason = string_at(&value, &["terminationReason"]).unwrap_or_default();
                state.status = if !string_at(&value, &["error"]).unwrap_or_default().is_empty()
                    || reason == "error"
                {
                    AgentStatus::Error
                } else if bool_at(&value, &["fullyIdle"]) == Some(true) {
                    AgentStatus::Done
                } else {
                    AgentStatus::Idle
                };
            }
            "PermissionRequest" => {
                state.status = AgentStatus::PermissionRequired;
                let tool = string_at(&value, &["toolCall", "name"]).unwrap_or_default();
                state.permission = Some(permission_from_tool(&tool));
            }
            _ => state.status = AgentStatus::Unknown,
        }
        if state.status == AgentStatus::Error {
            state.message = message_from(&value);
        }
        Some(state)
    }
}
