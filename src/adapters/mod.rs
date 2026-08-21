mod antigravity;
mod claude;
mod codex;
mod common;
mod copilot;
mod kimi;
mod opencode;

use std::process::Command;

use anyhow::Result;

use crate::state::{AgentKind, AgentState};

pub use antigravity::AntigravityAdapter;
pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;
pub use copilot::CopilotAdapter;
pub use kimi::KimiAdapter;
pub use opencode::OpenCodeAdapter;

pub trait AgentAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn kind(&self) -> AgentKind;
    fn executable_names(&self) -> &'static [&'static str];
    fn parse_event(&self, event: &str) -> Option<AgentState>;

    fn detect(&self) -> bool {
        self.executable_names().iter().any(|executable| {
            Command::new("sh")
                .args(["-c", &format!("command -v {} >/dev/null 2>&1", executable)])
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        })
    }

    fn install_integration(&self) -> Result<()> {
        Ok(())
    }
}

pub fn adapter(kind: AgentKind) -> Box<dyn AgentAdapter> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeAdapter),
        AgentKind::OpenCode => Box::new(OpenCodeAdapter),
        AgentKind::Codex => Box::new(CodexAdapter),
        AgentKind::Copilot => Box::new(CopilotAdapter),
        AgentKind::Antigravity => Box::new(AntigravityAdapter),
        AgentKind::Kimi => Box::new(KimiAdapter),
        _ => Box::new(common::GenericAdapter::new(kind)),
    }
}

pub fn primary_adapters() -> Vec<Box<dyn AgentAdapter>> {
    vec![
        Box::new(ClaudeAdapter),
        Box::new(OpenCodeAdapter),
        Box::new(CodexAdapter),
        Box::new(CopilotAdapter),
        Box::new(AntigravityAdapter),
        Box::new(KimiAdapter),
    ]
}

pub fn parse_terminal_output(text: &str) -> Option<common::ParsedStatus> {
    common::parse_terminal_output(text)
}
