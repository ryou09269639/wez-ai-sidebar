use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
    sync::Mutex,
    time,
};
use tracing::{debug, warn};

use super::process::{self, ProcessSnapshot};
use crate::{
    adapters::parse_terminal_output,
    config::Config,
    ipc::{ClientRequest, ServerResponse},
    paths,
    state::{project_name, AgentKind, AgentState, AgentStatus, DetectionSource, StateStore},
    wezterm::{WeztermClient, WeztermPane},
};

type SharedStore = Arc<Mutex<StateStore>>;

pub async fn run(config: Config) -> Result<()> {
    let socket = paths::socket_path()?;
    let cache = paths::cache_dir()?;
    fs::create_dir_all(&cache).await?;
    fs::create_dir_all(paths::state_dir()?).await?;
    fs::create_dir_all(paths::inbox_dir()?).await?;
    if socket.exists() {
        match crate::ipc::request(&socket, &ClientRequest::Ping).await {
            Ok(ServerResponse::Pong) => anyhow::bail!("daemon is already running"),
            _ => fs::remove_file(&socket)
                .await
                .with_context(|| format!("failed to remove stale socket {}", socket.display()))?,
        }
    }
    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("failed to bind {}", socket.display()))?;
    set_socket_permissions(&socket)?;
    let store = Arc::new(Mutex::new(StateStore::load(paths::state_dir()?)?));
    let scanner_store = Arc::clone(&store);
    let scanner_config = config.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(
            scanner_config.refresh_interval_ms.max(100),
        ));
        loop {
            interval.tick().await;
            if let Err(error) = rescan(&scanner_store, &scanner_config).await {
                debug!(%error, "background rescan failed");
            }
        }
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let store = Arc::clone(&store);
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, store, config).await {
                        warn!(%error, "IPC request failed");
                    }
                });
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                break;
            }
        }
    }
    if socket.exists() {
        fs::remove_file(socket).await?;
    }
    Ok(())
}

async fn handle_connection(stream: UnixStream, store: SharedStore, config: Config) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await?;
    let request: ClientRequest = serde_json::from_str(&line)?;
    let response = match request {
        ClientRequest::Snapshot => ServerResponse::Snapshot {
            agents: store
                .lock()
                .await
                .snapshot()
                .into_iter()
                .filter(|state| config.agents.enabled(state.agent))
                .collect(),
        },
        ClientRequest::Upsert { state } => {
            if config.agents.enabled(state.agent) {
                upsert_with_notification(&store, state, &config).await?;
            }
            ServerResponse::Ok
        }
        ClientRequest::Remove { key } => {
            store.lock().await.remove(&key)?;
            ServerResponse::Ok
        }
        ClientRequest::Rescan => {
            rescan(&store, &config).await?;
            ServerResponse::Ok
        }
        ClientRequest::Ping => ServerResponse::Pong,
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    Ok(())
}

async fn rescan(store: &SharedStore, config: &Config) -> Result<()> {
    ingest_inbox(store, config).await?;
    let client = WeztermClient::default();
    let panes = match client.list_panes().await {
        Ok(panes) => panes,
        Err(error) => {
            debug!(%error, "WezTerm pane discovery failed; preserving current states");
            store
                .lock()
                .await
                .prune(Duration::from_secs(config.stale_after_secs.max(60)))?;
            return Ok(());
        }
    };
    let processes = process::detect(&panes).await;
    reconcile_closed_agents(store, &panes, &processes).await?;
    discover_process_placeholders(store, &panes, &processes, config).await?;
    enrich_locations(store, &panes).await?;
    scrape_fallbacks(store, &client).await?;
    store
        .lock()
        .await
        .prune(Duration::from_secs(config.stale_after_secs.max(60)))?;
    Ok(())
}

async fn ingest_inbox(store: &SharedStore, config: &Config) -> Result<()> {
    let inbox = paths::inbox_dir()?;
    let mut entries = fs::read_dir(&inbox).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        match fs::read(&path).await {
            Ok(bytes) => match serde_json::from_slice::<AgentState>(&bytes) {
                Ok(state) if config.agents.enabled(state.agent) => {
                    upsert_with_notification(store, state, config).await?
                }
                Ok(_) => {}
                Err(error) => warn!(%error, path = %path.display(), "invalid inbox event"),
            },
            Err(error) => warn!(%error, path = %path.display(), "cannot read inbox event"),
        }
        fs::remove_file(path).await?;
    }
    Ok(())
}

async fn discover_process_placeholders(
    store: &SharedStore,
    panes: &[WeztermPane],
    processes: &ProcessSnapshot,
    config: &Config,
) -> Result<()> {
    let existing = store.lock().await.snapshot();
    for pane in panes {
        if existing
            .iter()
            .any(|state| state.wezterm.pane_id == Some(pane.pane_id))
            || pane.title.to_ascii_lowercase().contains("wez-ai-sidebar")
        {
            continue;
        }
        let detected = processes.by_pane.get(&pane.pane_id).copied();
        let kind = detected.map(|item| item.kind).or_else(|| {
            if processes.available && pane.tty_name.is_some() {
                None
            } else {
                kind_from_title(&pane.title)
            }
        });
        let Some(kind) = kind else {
            continue;
        };
        if !config.agents.enabled(kind) {
            continue;
        }
        let cwd = pane.cwd_path();
        let mut state = AgentState::new(format!("pane-{}", pane.pane_id), kind, cwd);
        state.wezterm.pane_id = Some(pane.pane_id);
        state.wezterm.tab_id = Some(pane.tab_id);
        state.wezterm.window_id = Some(pane.window_id);
        state.pid = detected.map(|item| item.pid);
        state.source = DetectionSource::Process;
        store.lock().await.upsert(state)?;
    }
    Ok(())
}

async fn reconcile_closed_agents(
    store: &SharedStore,
    panes: &[WeztermPane],
    processes: &ProcessSnapshot,
) -> Result<()> {
    let pane_map = panes
        .iter()
        .map(|pane| (pane.pane_id, pane))
        .collect::<HashMap<_, _>>();
    let snapshot = store.lock().await.snapshot();
    let mut remove = Vec::new();
    for state in snapshot {
        let Some(pane_id) = state.wezterm.pane_id else {
            continue;
        };
        let Some(pane) = pane_map.get(&pane_id) else {
            remove.push(state.key());
            continue;
        };
        if !is_pane_placeholder(&state) {
            continue;
        }
        let detected_now = processes
            .by_pane
            .get(&pane_id)
            .is_some_and(|detected| detected.kind == state.agent);
        let recorded_pid_alive = state
            .pid
            .is_some_and(|pid| process::pid_matches_agent(pid, state.agent));
        let title_fallback_alive = (!processes.available || pane.tty_name.is_none())
            && kind_from_title(&pane.title) == Some(state.agent);
        if !detected_now && !recorded_pid_alive && !title_fallback_alive {
            remove.push(state.key());
        }
    }
    let mut store = store.lock().await;
    for key in remove {
        store.remove(&key)?;
    }
    Ok(())
}

fn is_pane_placeholder(state: &AgentState) -> bool {
    state
        .wezterm
        .pane_id
        .is_some_and(|pane_id| state.id == format!("pane-{pane_id}"))
}

async fn enrich_locations(store: &SharedStore, panes: &[WeztermPane]) -> Result<()> {
    let pane_map = panes
        .iter()
        .map(|pane| (pane.pane_id, pane))
        .collect::<HashMap<_, _>>();
    let snapshot = store.lock().await.snapshot();
    for mut state in snapshot {
        let Some(pane_id) = state.wezterm.pane_id else {
            continue;
        };
        let Some(pane) = pane_map.get(&pane_id) else {
            continue;
        };
        let mut changed = false;
        if state.wezterm.tab_id != Some(pane.tab_id) {
            state.wezterm.tab_id = Some(pane.tab_id);
            changed = true;
        }
        if state.wezterm.window_id != Some(pane.window_id) {
            state.wezterm.window_id = Some(pane.window_id);
            changed = true;
        }
        if state.cwd == "unknown" {
            state.cwd = pane.cwd_path();
            state.project = project_name(&state.cwd);
            changed = true;
        }
        if changed {
            store.lock().await.upsert(state)?;
        }
    }
    Ok(())
}

async fn scrape_fallbacks(store: &SharedStore, client: &WeztermClient) -> Result<()> {
    let snapshot = store.lock().await.snapshot();
    for mut state in snapshot {
        if state.agent != AgentKind::Antigravity
            && state.source != DetectionSource::Process
            && state.source != DetectionSource::TerminalOutput
        {
            continue;
        }
        let Some(pane_id) = state.wezterm.pane_id else {
            continue;
        };
        let Ok(text) = client.get_text(pane_id, 30).await else {
            continue;
        };
        let Some(parsed) = parse_terminal_output(&text) else {
            continue;
        };
        if state.status != parsed.status || state.permission != parsed.permission {
            state.status = parsed.status;
            state.permission = parsed.permission;
            state.message = parsed.message;
            state.source = DetectionSource::TerminalOutput;
            state.updated_at = Utc::now();
            store.lock().await.upsert(state)?;
        }
    }
    Ok(())
}

async fn upsert_with_notification(
    store: &SharedStore,
    state: AgentState,
    config: &Config,
) -> Result<()> {
    let previous = store.lock().await.upsert(state.clone())?;
    let notify_permission = config.notifications.permission
        && state.status == AgentStatus::PermissionRequired
        && previous != Some(AgentStatus::PermissionRequired);
    let notify_done = config.notifications.done
        && state.status == AgentStatus::Done
        && previous != Some(AgentStatus::Done);
    if notify_permission || notify_done {
        let title = format!("{} - {}", state.agent.display_name(), state.project);
        let body = state
            .message
            .clone()
            .unwrap_or_else(|| state.status.label().to_owned());
        tokio::spawn(async move {
            let _ = Command::new("notify-send")
                .arg(title)
                .arg(body)
                .status()
                .await;
        });
    }
    Ok(())
}

fn kind_from_title(title: &str) -> Option<AgentKind> {
    let title = title.to_ascii_lowercase();
    if title.contains("claude") {
        Some(AgentKind::Claude)
    } else if title.contains("opencode") || title == "oc" || title.starts_with("oc |") {
        Some(AgentKind::OpenCode)
    } else if title.contains("codex") {
        Some(AgentKind::Codex)
    } else if title.contains("copilot") {
        Some(AgentKind::Copilot)
    } else if title.contains("antigravity") || title == "agy" || title.starts_with("agy ") {
        Some(AgentKind::Antigravity)
    } else if title == "kimi"
        || title.starts_with("kimi ")
        || title.contains("kimi code")
        || title.contains("kimi-cli")
    {
        Some(AgentKind::Kimi)
    } else {
        None
    }
}

#[cfg(unix)]
fn set_socket_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_socket_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(pane_id: u64, title: &str) -> WeztermPane {
        WeztermPane {
            window_id: 1,
            tab_id: 2,
            pane_id,
            workspace: "default".to_owned(),
            size: None,
            title: title.to_owned(),
            cwd: "file://localhost/tmp/project".to_owned(),
            tty_name: Some(format!("/dev/pts/{pane_id}")),
        }
    }

    #[test]
    fn process_detection_does_not_confuse_sidebar() {
        assert_eq!(kind_from_title("wez-ai-sidebar"), None);
        assert_eq!(kind_from_title("Claude Code"), Some(AgentKind::Claude));
        assert_eq!(kind_from_title("agy"), Some(AgentKind::Antigravity));
        assert_eq!(kind_from_title("Kimi Code CLI"), Some(AgentKind::Kimi));
    }

    #[tokio::test]
    async fn tty_process_detects_codex_with_a_shell_title() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Mutex::new(StateStore::load(directory.path()).unwrap()));
        let panes = vec![pane(12, "zsh")];
        let processes = ProcessSnapshot {
            available: true,
            by_pane: HashMap::from([(
                12,
                process::DetectedAgent {
                    kind: AgentKind::Codex,
                    pid: 4242,
                },
            )]),
        };
        discover_process_placeholders(&store, &panes, &processes, &Config::default())
            .await
            .unwrap();
        let snapshot = store.lock().await.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].agent, AgentKind::Codex);
        assert_eq!(snapshot[0].pid, Some(4242));
    }

    #[tokio::test]
    async fn removes_an_agent_as_soon_as_its_pane_closes() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Mutex::new(StateStore::load(directory.path()).unwrap()));
        let mut state = AgentState::new("session", AgentKind::Codex, "/tmp/project");
        state.wezterm.pane_id = Some(12);
        store.lock().await.upsert(state).unwrap();
        reconcile_closed_agents(&store, &[], &ProcessSnapshot::default())
            .await
            .unwrap();
        assert!(store.lock().await.snapshot().is_empty());
    }
}
