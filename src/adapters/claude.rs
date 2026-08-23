use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource};

use super::{
    common::{base_state, event_name, message_from, permission_from_tool, string_at, tool_summary},
    AgentAdapter,
};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["claude"]
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
            "Notification" => {
                let notification = string_at(&value, &["notification_type"]).unwrap_or_default();
                if notification == "permission_prompt" {
                    state.status = AgentStatus::PermissionRequired;
                } else if notification == "idle_prompt" || notification == "elicitation_dialog" {
                    state.status = AgentStatus::WaitingInput;
                } else {
                    state.status = AgentStatus::Idle;
                }
            }
            "UserPromptSubmit" => {
                state.status = AgentStatus::Working;
            }
            "PreToolUse" | "PostToolUse" => {
                state.status = AgentStatus::Working;
                state.message = tool_summary(&value);
            }
            "PostToolUseFailure" | "StopFailure" => state.status = AgentStatus::Error,
            "Stop" => state.status = AgentStatus::Done,
            "SessionStart" => state.status = AgentStatus::Idle,
            "SessionEnd" => state.status = AgentStatus::Done,
            _ => state.status = AgentStatus::Unknown,
        }
        if matches!(
            name.as_str(),
            "PermissionRequest" | "Notification" | "PostToolUseFailure" | "StopFailure"
        ) {
            state.message = message_from(&value);
        }
        Some(state)
    }
}
