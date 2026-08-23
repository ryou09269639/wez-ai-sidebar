use wez_ai_sidebar::{
    adapters::{
        AgentAdapter, ClaudeAdapter, CodexAdapter, CopilotAdapter, KimiAdapter, OpenCodeAdapter,
    },
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
fn opencode_session_deleted_is_terminal() {
    let event =
        r#"{"type":"session.deleted","properties":{"info":{"id":"s1"}},"cwd":"/tmp/project"}"#;
    let state = OpenCodeAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::Done);
    assert_eq!(state.id, "s1");
}

#[test]
fn opencode_session_error_reads_the_nested_named_error_message() {
    let event = r#"{"type":"session.error","properties":{"sessionID":"s1","error":{"name":"UnknownError","data":{"message":"boom"}}},"cwd":"/tmp/project"}"#;
    let state = OpenCodeAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::Error);
    assert_eq!(state.message.as_deref(), Some("boom"));
}

#[test]
fn opencode_session_error_without_a_session_id_is_not_tracked() {
    // e.g. a skill frontmatter parse failure, which OpenCode publishes as a
    // session.error with no sessionID at all. Tracking it would create a
    // placeholder no later event can ever update or clear.
    let event = r#"{"type":"session.error","properties":{"error":{"name":"UnknownError","data":{"message":"boom"}}},"cwd":"/tmp/project"}"#;
    assert!(OpenCodeAdapter.parse_event(event).is_none());
}

#[test]
fn copilot_notification_distinguishes_human_input() {
    let event = r#"{"sessionId":"s1","cwd":"/tmp/project","hook_event_name":"Notification","notification_type":"elicitation_dialog","message":"Choose a target"}"#;
    let state = CopilotAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::WaitingInput);
}

#[test]
fn kimi_permission_notification_requires_human_attention() {
    let event = r#"{"session_id":"s1","cwd":"/tmp/project","hook_event_name":"Notification","sink":"llm","notification_type":"permission_prompt","title":"Approval required","body":"Allow Shell?","severity":"info"}"#;
    let state = KimiAdapter.parse_event(event).unwrap();
    assert_eq!(state.status, AgentStatus::PermissionRequired);
    assert_eq!(state.permission, Some(PermissionType::Other));
    assert_eq!(state.message.as_deref(), Some("Approval required"));
}

#[test]
fn kimi_lifecycle_events_transition_to_working_and_done() {
    let working = KimiAdapter
        .parse_event(
            r#"{"session_id":"s1","cwd":"/tmp/project","hook_event_name":"PreToolUse","tool_name":"Shell"}"#,
        )
        .unwrap();
    let done = KimiAdapter
        .parse_event(
            r#"{"session_id":"s1","cwd":"/tmp/project","hook_event_name":"Stop","stop_hook_active":false}"#,
        )
        .unwrap();
    assert_eq!(working.status, AgentStatus::Working);
    assert_eq!(done.status, AgentStatus::Done);
}

#[test]
fn kimi_ignores_unrelated_notifications() {
    let event = r#"{"session_id":"s1","cwd":"/tmp/project","hook_event_name":"Notification","notification_type":"informational"}"#;
    assert!(KimiAdapter.parse_event(event).is_none());
}
