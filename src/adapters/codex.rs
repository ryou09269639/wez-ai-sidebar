use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource};

use super::{
    common::{base_state, event_name, message_from, permission_from_tool, string_at},
    AgentAdapter,
};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "OpenAI Codex CLI"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["codex"]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        let mut state = base_state(self.kind(), &value);
        state.source = DetectionSource::StructuredEvent;
        let name = event_name(&value);
        match name.as_str() {
            "PermissionRequest" => {
                state.status = AgentStatus::PermissionRequired;
                let tool = string_at(&value, &["tool_name"]).unwrap_or_default();
                state.permission = Some(permission_from_tool(&tool));
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PreCompact" | "PostCompact" => {
                state.status = AgentStatus::Working
            }
            "Stop" => state.status = AgentStatus::Done,
            "SessionStart" => state.status = AgentStatus::Idle,
            "SessionEnd" => state.status = AgentStatus::Done,
            _ => state.status = AgentStatus::Unknown,
        }
        if name == "PermissionRequest" {
            state.message = message_from(&value);
        }
        Some(state)
    }
}
