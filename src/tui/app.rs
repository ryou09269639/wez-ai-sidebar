use std::{io, time::Duration};

use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    config::Config,
    ipc::{self, ClientRequest},
    paths,
    state::{load_snapshot, AgentState},
    wezterm::WeztermClient,
};

use super::ui;

pub async fn run(config: Config) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut agents = fetch_states(&config).await;
    let mut selected = 0usize;
    let mut help = false;
    let refresh = Duration::from_millis(config.refresh_interval_ms.max(100));
    loop {
        selected = selected.min(agents.len().saturating_sub(1));
        terminal.draw(|frame| ui::render(frame, &agents, selected, help, &config.sidebar))?;
        if event::poll(refresh)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('?') => help = !help,
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !agents.is_empty() {
                            selected = (selected + 1).min(agents.len() - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Enter => activate_selected(&agents, selected).await,
                    KeyCode::Char('r') => {
                        if let Ok(socket) = paths::socket_path() {
                            let _ = ipc::request(&socket, &ClientRequest::Rescan).await;
                        }
                    }
                    KeyCode::Char(character) if ('1'..='9').contains(&character) => {
                        let index = character.to_digit(10).unwrap_or(1) as usize - 1;
                        if index < agents.len() {
                            selected = index;
                            activate_selected(&agents, selected).await;
                        }
                    }
                    _ => {}
                }
            }
        }
        agents = fetch_states(&config).await;
    }
    Ok(())
}

async fn fetch_states(config: &Config) -> Vec<AgentState> {
    let socket = match paths::socket_path() {
        Ok(socket) => socket,
        Err(_) => return Vec::new(),
    };
    if let Ok(states) = ipc::snapshot(&socket).await {
        return states
            .into_iter()
            .filter(|state| config.agents.enabled(state.agent))
            .collect();
    }
    paths::state_dir()
        .ok()
        .and_then(|directory| load_snapshot(&directory).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|state| config.agents.enabled(state.agent))
        .collect()
}

async fn activate_selected(agents: &[AgentState], selected: usize) {
    let Some(pane_id) = agents.get(selected).and_then(|state| state.wezterm.pane_id) else {
        return;
    };
    let _ = WeztermClient::default().activate_pane(pane_id).await;
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            SetTitle("wez-ai-sidebar")
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show,
            SetTitle("wez-ai-sidebar closed")
        );
    }
}
