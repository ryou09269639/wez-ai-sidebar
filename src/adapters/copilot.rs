use serde_json::Value;

use crate::state::{AgentKind, AgentState, AgentStatus, DetectionSource};

use super::{
    common::{base_state, event_name, message_from, permission_from_tool, string_at},
    AgentAdapter,
};

pub struct CopilotAdapter;

impl AgentAdapter for CopilotAdapter {
    fn name(&self) -> &'static str {
        "GitHub Copilot CLI"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Copilot
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["copilot", "github-copilot-cli"]
    }

    fn parse_event(&self, event: &str) -> Option<AgentState> {
        let value: Value = serde_json::from_str(event).ok()?;
        let mut state = base_state(self.kind(), &value);
        state.source = DetectionSource::StructuredEvent;
        let name = event_name(&value);
        let normalized = name.to_ascii_lowercase();
        match normalized.as_str() {
            "notification" => {
                let notification = string_at(&value, &["notification_type"])
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                match notification.as_str() {
                    "permission_prompt" => state.status = AgentStatus::PermissionRequired,
                    "elicitation_dialog" => state.status = AgentStatus::WaitingInput,
                    "agent_idle" | "agent_completed" => state.status = AgentStatus::Done,
                    _ => state.status = AgentStatus::Idle,
                }
            }
            "permissionrequest" => {
                state.status = AgentStatus::PermissionRequired;
                let tool = string_at(&value, &["toolName"])
                    .or_else(|| string_at(&value, &["tool_name"]))
                    .unwrap_or_default();
                state.permission = Some(permission_from_tool(&tool));
            }
            "userpromptsubmitted" | "pretooluse" | "posttooluse" => {
                state.status = AgentStatus::Working;
            }
            "agentstop" | "sessionend" => state.status = AgentStatus::Done,
            "sessionstart" => state.status = AgentStatus::Idle,
            "erroroccurred" => state.status = AgentStatus::Error,
            _ => state.status = AgentStatus::Unknown,
        }
        if matches!(
            normalized.as_str(),
            "notification" | "permissionrequest" | "erroroccurred"
        ) {
            state.message = message_from(&value);
        }
        Some(state)
    }
}
