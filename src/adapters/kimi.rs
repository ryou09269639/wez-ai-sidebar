use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource, PermissionType};

use super::{
    common::{base_state, event_name, message_from, string_at},
    AgentAdapter,
};

pub struct KimiAdapter;

impl AgentAdapter for KimiAdapter {
    fn name(&self) -> &'static str {
        "Kimi Code CLI"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Kimi
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["kimi", "kimi-cli"]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        let mut state = base_state(self.kind(), &value);
        state.source = DetectionSource::StructuredEvent;
        let name = event_name(&value);
        match name.as_str() {
            "Notification" => {
                let notification = string_at(&value, &["notification_type"])
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                match notification.as_str() {
                    "permission_prompt" => {
                        state.status = AgentStatus::PermissionRequired;
                        state.permission = Some(PermissionType::Other);
                    }
                    "elicitation_dialog" | "input_required" | "question" => {
                        state.status = AgentStatus::WaitingInput;
                    }
                    "task_completed" => state.status = AgentStatus::Done,
                    "agent_idle" => state.status = AgentStatus::Idle,
                    _ => return None,
                }
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "SubagentStart"
            | "SubagentStop" | "PreCompact" | "PostCompact" => {
                state.status = AgentStatus::Working;
            }
            "PostToolUseFailure" | "StopFailure" => state.status = AgentStatus::Error,
            "Stop" | "SessionEnd" => state.status = AgentStatus::Done,
            "SessionStart" => state.status = AgentStatus::Idle,
            _ => state.status = AgentStatus::Unknown,
        }
        if matches!(
            name.as_str(),
            "Notification" | "PostToolUseFailure" | "StopFailure"
        ) {
            state.message = message_from(&value);
        }
        Some(state)
    }
}
