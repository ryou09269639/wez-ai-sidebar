use std::path::PathBuf;

use anyhow::Result;
use tokio::process::Command;

use crate::{adapters::primary_adapters, ipc, paths};

pub async fn run() -> Result<()> {
    let wezterm = command_ok("wezterm", &["--version"]).await;
    let wezterm_cli = command_ok("wezterm", &["cli", "list", "--format", "json"]).await;
    print_check("WezTerm", wezterm, None);
    print_check("WezTerm CLI", wezterm_cli, None);
    for adapter in primary_adapters() {
        print_check(adapter.name(), adapter.detect(), None);
        let integration = integration_path(adapter.kind())?;
        print_check(
            &format!("{} integration", adapter.kind().display_name()),
            integration.exists(),
            Some(&integration),
        );
    }
    let socket = paths::socket_path()?;
    let daemon = matches!(
        ipc::request(&socket, &ipc::ClientRequest::Ping).await,
        Ok(ipc::ServerResponse::Pong)
    );
    print_check("daemon", daemon, None);
    print_check("socket", socket.exists() && daemon, Some(&socket));
    Ok(())
}

fn integration_path(kind: crate::state::AgentKind) -> Result<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    Ok(match kind {
        crate::state::AgentKind::Claude => home.join(".claude/settings.json"),
        crate::state::AgentKind::Codex => home.join(".codex/hooks.json"),
        crate::state::AgentKind::OpenCode => dirs::config_dir()
            .unwrap_or_default()
            .join("opencode/plugins/wez-ai-sidebar.js"),
        crate::state::AgentKind::Copilot => home.join(".copilot/hooks/wez-ai-sidebar.json"),
        crate::state::AgentKind::Antigravity => home.join(".gemini/config/hooks.json"),
        _ => paths::config_dir()?,
    })
}

async fn command_ok(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn print_check(label: &str, ok: bool, path: Option<&PathBuf>) {
    let result = if ok { "OK" } else { "Not Found" };
    if let Some(path) = path {
        println!("{label:<28} {result:<10} {}", path.display());
    } else {
        println!("{label:<28} {result}");
    }
}
