use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn kimi_hook_observes_without_writing_to_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_wez-ai-sidebar"))
        .args(["hook", "kimi", "Notification"])
        .env("XDG_CACHE_HOME", temp.path().join("cache"))
        .env("XDG_CONFIG_HOME", temp.path().join("config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            br#"{"session_id":"s1","cwd":"/tmp/project","notification_type":"permission_prompt","title":"Approval required"}"#,
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());

    let inbox = temp.path().join("cache/wez-ai-sidebar/inbox");
    assert_eq!(std::fs::read_dir(inbox).unwrap().count(), 1);
}
