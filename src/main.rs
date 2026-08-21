use std::{io::Read, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
use wez_ai_sidebar::{
    adapters,
    config::Config,
    daemon, doctor, installer, ipc, paths,
    state::{AgentKind, AgentState, AgentStatus, DetectionSource},
    tui,
};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the background state daemon.
    Daemon,
    /// Print the current agent snapshot.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Diagnose WezTerm, integrations, daemon, and agent CLIs.
    Doctor,
    /// Install config, integrations, Lua module, and user service.
    Install {
        #[arg(long)]
        no_enable: bool,
    },
    /// Remove files created by the installer without touching unrelated config.
    Uninstall,
    /// Receive one structured lifecycle event on stdin (used by integrations).
    #[command(hide = true)]
    Hook {
        agent: AgentArg,
        event: Option<String>,
    },
    /// Publish a mock state for UI testing.
    #[command(hide = true)]
    Mock {
        agent: AgentArg,
        #[arg(value_enum, default_value = "working")]
        status: StatusArg,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AgentArg {
    Claude,
    Opencode,
    Codex,
    Copilot,
    Antigravity,
}

impl From<AgentArg> for AgentKind {
    fn from(value: AgentArg) -> Self {
        match value {
            AgentArg::Claude => Self::Claude,
            AgentArg::Opencode => Self::OpenCode,
            AgentArg::Codex => Self::Codex,
            AgentArg::Copilot => Self::Copilot,
            AgentArg::Antigravity => Self::Antigravity,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StatusArg {
    Idle,
    Working,
    WaitingInput,
    PermissionRequired,
    Done,
    Error,
    Unknown,
}

impl From<StatusArg> for AgentStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Idle => Self::Idle,
            StatusArg::Working => Self::Working,
            StatusArg::WaitingInput => Self::WaitingInput,
            StatusArg::PermissionRequired => Self::PermissionRequired,
            StatusArg::Done => Self::Done,
            StatusArg::Error => Self::Error,
            StatusArg::Unknown => Self::Unknown,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let config = Config::load()?;
    match cli.command {
        None => tui::run(config).await,
        Some(Command::Daemon) => daemon::run(config).await,
        Some(Command::Status { json }) => print_status(json, &config).await,
        Some(Command::Doctor) => doctor::run().await,
        Some(Command::Install { no_enable }) => installer::install(!no_enable).await,
        Some(Command::Uninstall) => installer::uninstall().await,
        Some(Command::Hook { agent, event }) => receive_hook(agent.into(), event).await,
        Some(Command::Mock {
            agent,
            status,
            id,
            cwd,
        }) => publish_mock(agent.into(), status.into(), id, cwd).await,
    }
}

async fn print_status(json: bool, config: &Config) -> Result<()> {
    let mut agents = match ipc::snapshot(&paths::socket_path()?).await {
        Ok(agents) => agents,
        Err(_) => wez_ai_sidebar::state::load_snapshot(&paths::state_dir()?)?,
    };
    agents.retain(|state| config.agents.enabled(state.agent));
    if json {
        println!("{}", serde_json::to_string_pretty(&agents)?);
    } else if agents.is_empty() {
        println!("No agents detected");
    } else {
        for state in agents {
            println!(
                "{:<12} {:<12} {:<20} pane={}",
                state.agent.display_name(),
                state.status.label(),
                state.project,
                state
                    .wezterm
                    .pane_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_owned())
            );
        }
    }
    Ok(())
}

async fn receive_hook(kind: AgentKind, event_name: Option<String>) -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input = attach_event_name(&input, event_name.as_deref());
    if let Some(state) = adapters::adapter(kind).parse_event(&input) {
        if publish_state(state).await.is_err() {
            // Hooks must never break or approve/deny an agent operation. A failed
            // daemon delivery is spooled for later ingestion.
        }
    }
    if kind == AgentKind::Antigravity && event_name.as_deref() == Some("Stop") {
        println!(r#"{{"decision":"stop"}}"#);
    } else {
        println!("{{}}");
    }
    Ok(())
}

fn attach_event_name(input: &str, event_name: Option<&str>) -> String {
    let Some(event_name) = event_name else {
        return input.to_owned();
    };
    let mut value: Value =
        serde_json::from_str(input).unwrap_or_else(|_| Value::Object(Default::default()));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "_wez_event".to_owned(),
            Value::String(event_name.to_owned()),
        );
    }
    value.to_string()
}

async fn publish_mock(
    kind: AgentKind,
    status: AgentStatus,
    id: Option<String>,
    cwd: Option<PathBuf>,
) -> Result<()> {
    let cwd = cwd
        .unwrap_or(std::env::current_dir()?)
        .to_string_lossy()
        .into_owned();
    let mut state = AgentState::new(id.unwrap_or_else(|| Uuid::new_v4().to_string()), kind, cwd);
    state.status = status;
    state.source = DetectionSource::Mock;
    state.wezterm.pane_id = std::env::var("WEZTERM_PANE")
        .ok()
        .and_then(|value| value.parse().ok());
    publish_state(state).await
}

async fn publish_state(state: AgentState) -> Result<()> {
    let socket = paths::socket_path()?;
    if let Ok(response) = ipc::request(
        &socket,
        &ipc::ClientRequest::Upsert {
            state: state.clone(),
        },
    )
    .await
    {
        if matches!(response, ipc::ServerResponse::Ok) {
            return Ok(());
        }
    }
    let inbox = paths::inbox_dir()?;
    tokio::fs::create_dir_all(&inbox).await?;
    let path = inbox.join(format!("{}.json", Uuid::new_v4()));
    tokio::fs::write(&path, serde_json::to_vec(&state)?)
        .await
        .with_context(|| format!("failed to spool {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_hook_name_without_persisting_raw_text() {
        let value: Value = serde_json::from_str(&attach_event_name("{}", Some("Stop"))).unwrap();
        assert_eq!(value["_wez_event"], "Stop");
    }
}
