use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    OpenCode,
    Codex,
    Copilot,
    Antigravity,
    Gemini,
    Aider,
    Cursor,
    Custom,
}

impl AgentKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenCode => "OpenCode",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot",
            Self::Antigravity => "Antigravity",
            Self::Gemini => "Gemini",
            Self::Aider => "Aider",
            Self::Cursor => "Cursor",
            Self::Custom => "Agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    WaitingInput,
    PermissionRequired,
    Done,
    Error,
    Unknown,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Working => "WORKING",
            Self::WaitingInput => "INPUT",
            Self::PermissionRequired => "PERMISSION",
            Self::Done => "DONE",
            Self::Error => "ERROR",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionType {
    FileEdit,
    FileWrite,
    ShellCommand,
    Network,
    Mcp,
    Other,
}

impl PermissionType {
    pub fn label(self) -> &'static str {
        match self {
            Self::FileEdit => "File edit",
            Self::FileWrite => "File write",
            Self::ShellCommand => "Shell",
            Self::Network => "Network",
            Self::Mcp => "MCP",
            Self::Other => "Approval",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    StructuredEvent,
    Hook,
    SessionFile,
    TerminalOutput,
    Process,
    Mock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WeztermLocation {
    pub pane_id: Option<u64>,
    pub tab_id: Option<u64>,
    pub window_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentState {
    pub id: String,
    pub agent: AgentKind,
    pub status: AgentStatus,
    pub permission: Option<PermissionType>,
    pub cwd: String,
    pub project: String,
    pub wezterm: WeztermLocation,
    pub pid: Option<u32>,
    pub message: Option<String>,
    pub source: DetectionSource,
    pub updated_at: DateTime<Utc>,
}

impl AgentState {
    pub fn new(id: impl Into<String>, agent: AgentKind, cwd: impl Into<String>) -> Self {
        let cwd = cwd.into();
        Self {
            id: id.into(),
            agent,
            status: AgentStatus::Unknown,
            permission: None,
            project: project_name(&cwd),
            cwd,
            wezterm: WeztermLocation::default(),
            pid: None,
            message: None,
            source: DetectionSource::Process,
            updated_at: Utc::now(),
        }
    }

    pub fn key(&self) -> String {
        format!("{:?}:{}", self.agent, self.id).to_lowercase()
    }

    pub fn short_message(message: impl AsRef<str>) -> Option<String> {
        let cleaned = message
            .as_ref()
            .chars()
            .filter(|ch| !ch.is_control() || *ch == ' ')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned.chars().take(120).collect())
        }
    }
}

pub fn project_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("unknown")
        .to_owned()
}
