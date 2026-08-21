use wez_ai_sidebar::{
    adapters::{AgentAdapter, ClaudeAdapter, CodexAdapter, CopilotAdapter, OpenCodeAdapter},
    state::{AgentStatus, PermissionType},
};

#[test]
fn claude_permission_request_is_structured() {
    let event = r#"{"session_id":"abc","cwd":"/tmp/project","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"cargo test"}}"#;
    let state = ClaudeAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::PermissionRequired);
    assert_eq!(state.permission, Some(PermissionType::ShellCommand));
}

#[test]
fn codex_apply_patch_permission_is_file_edit() {
    let event = r#"{"session_id":"abc","cwd":"/tmp/project","hook_event_name":"PermissionRequest","tool_name":"apply_patch","tool_input":{"command":"*** Begin Patch"}}"#;
    let state = CodexAdapter.parse_event(event).unwrap();
    assert_eq!(state.permission, Some(PermissionType::FileEdit));
}

#[test]
fn opencode_permission_asked_is_detected() {
    let event = r#"{"type":"permission.asked","properties":{"sessionID":"s1","permission":"edit"},"cwd":"/tmp/project"}"#;
    let state = OpenCodeAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::PermissionRequired);
    assert_eq!(state.permission, Some(PermissionType::FileEdit));
}

#[test]
fn copilot_notification_distinguishes_human_input() {
    let event = r#"{"sessionId":"s1","cwd":"/tmp/project","hook_event_name":"Notification","notification_type":"elicitation_dialog","message":"Choose a target"}"#;
    let state = CopilotAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::WaitingInput);
}
