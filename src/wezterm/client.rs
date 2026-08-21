use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;

use super::WeztermPane;

#[derive(Debug, Clone)]
pub struct WeztermClient {
    executable: String,
}

impl Default for WeztermClient {
    fn default() -> Self {
        Self {
            executable: "wezterm".to_owned(),
        }
    }
}

impl WeztermClient {
    #[cfg(test)]
    pub fn with_executable(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub async fn list_panes(&self) -> Result<Vec<WeztermPane>> {
        let output = Command::new(&self.executable)
            .args(["cli", "list", "--format", "json"])
            .stdin(Stdio::null())
            .output()
            .await
            .context("failed to execute `wezterm cli list`")?;
        if !output.status.success() {
            anyhow::bail!(
                "wezterm cli list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        serde_json::from_slice(&output.stdout).context("invalid JSON from `wezterm cli list`")
    }

    pub async fn get_text(&self, pane_id: u64, lines: u16) -> Result<String> {
        let start_line = format!("-{}", lines.max(1));
        let output = Command::new(&self.executable)
            .args([
                "cli",
                "get-text",
                "--pane-id",
                &pane_id.to_string(),
                "--start-line",
                &start_line,
            ])
            .stdin(Stdio::null())
            .output()
            .await
            .context("failed to execute `wezterm cli get-text`")?;
        if !output.status.success() {
            anyhow::bail!(
                "wezterm cli get-text failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub async fn activate_pane(&self, pane_id: u64) -> Result<()> {
        let status = Command::new(&self.executable)
            .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
            .stdin(Stdio::null())
            .status()
            .await
            .context("failed to execute `wezterm cli activate-pane`")?;
        if !status.success() {
            anyhow::bail!("wezterm could not activate pane {pane_id}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_list_schema() {
        let panes: Vec<WeztermPane> = serde_json::from_str(
            r#"[{"window_id":0,"tab_id":4,"pane_id":12,"workspace":"default","size":{"rows":24,"cols":80},"title":"claude","cwd":"file://host/home/user/project","tty_name":"/dev/pts/7"}]"#,
        )
        .unwrap();
        assert_eq!(panes[0].pane_id, 12);
        assert_eq!(panes[0].tab_id, 4);
        assert_eq!(panes[0].cwd_path(), "/home/user/project");
        assert_eq!(panes[0].tty_name.as_deref(), Some("/dev/pts/7"));
    }
}
