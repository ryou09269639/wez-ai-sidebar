use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;
use crate::state::AgentKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub refresh_interval_ms: u64,
    pub stale_after_secs: u64,
    pub sidebar: SidebarConfig,
    pub notifications: NotificationConfig,
    pub agents: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarConfig {
    pub width: u16,
    pub show_cwd: bool,
    pub show_message: bool,
    pub unicode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub permission: bool,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub claude: bool,
    pub codex: bool,
    pub opencode: bool,
    pub copilot: bool,
    pub antigravity: bool,
    pub kimi: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval_ms: 300,
            stale_after_secs: 86_400,
            sidebar: SidebarConfig::default(),
            notifications: NotificationConfig::default(),
            agents: AgentConfig::default(),
        }
    }
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            width: 20,
            show_cwd: true,
            show_message: true,
            unicode: true,
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            permission: true,
            done: false,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            claude: true,
            codex: true,
            opencode: true,
            copilot: true,
            antigravity: true,
            kimi: true,
        }
    }
}

impl AgentConfig {
    pub fn enabled(&self, kind: AgentKind) -> bool {
        match kind {
            AgentKind::Claude => self.claude,
            AgentKind::Codex => self.codex,
            AgentKind::OpenCode => self.opencode,
            AgentKind::Copilot => self.copilot,
            AgentKind::Antigravity => self.antigravity,
            AgentKind::Kimi => self.kimi,
            _ => true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = paths::config_file()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid config at {}", path.display()))
    }

    pub fn write_default_if_missing(path: &Path) -> Result<bool> {
        if path.exists() {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(&Self::default())?;
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(true)
    }
}
