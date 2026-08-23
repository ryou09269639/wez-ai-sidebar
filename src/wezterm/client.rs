use std::{env, fs, path::PathBuf, process::Stdio, time::SystemTime};

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

    // A process started outside any WezTerm pane - the systemd-managed daemon,
    // notably - never inherits `WEZTERM_UNIX_SOCKET`. Without it, `wezterm cli`
    // does not error: it silently falls back to the default `unix` mux domain
    // and spawns an unrelated single-pane session, which makes every real pane
    // look closed on the very next rescan. Pin the socket to the actual running
    // GUI instance ourselves so pane discovery stays correct regardless of how
    // the daemon was launched.
    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        if let Some(socket) = gui_socket_path() {
            command.env("WEZTERM_UNIX_SOCKET", socket);
        }
        command
    }

    pub async fn list_panes(&self) -> Result<Vec<WeztermPane>> {
        let output = self
            .command()
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
        let output = self
            .command()
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
        let status = self
            .command()
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

/// Locates the unix socket of the currently running WezTerm GUI instance, so
/// `wezterm cli` can be pointed at it explicitly instead of relying on
/// ambient environment variables a background process won't have.
fn gui_socket_path() -> Option<PathBuf> {
    // If the caller's own environment already targets an instance (e.g. the
    // daemon was started interactively from inside a WezTerm pane), trust it.
    if env::var_os("WEZTERM_UNIX_SOCKET").is_some() {
        return None;
    }
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for directory in candidate_directories() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|name| name.strip_prefix("gui-sock-"))
            else {
                continue;
            };
            let Ok(pid) = pid.parse::<u32>() else {
                continue;
            };
            if !process_is_alive(pid) {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
                continue;
            };
            let replace = match &newest {
                Some((time, _)) => modified > *time,
                None => true,
            };
            if replace {
                newest = Some((modified, entry.path()));
            }
        }
    }
    newest.map(|(_, path)| path)
}

fn candidate_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(runtime) = dirs::runtime_dir() {
        directories.push(runtime.join("wezterm"));
    }
    if let Ok(tmpdir) = env::var("TMPDIR") {
        directories.push(PathBuf::from(tmpdir).join("wezterm"));
    }
    directories.push(env::temp_dir().join("wezterm"));
    directories
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // Signal 0 sends nothing; it only checks whether the target could be
    // signaled. EPERM (owned by another user, e.g. a root-owned pid) still
    // means the process exists - only ESRCH means it does not.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

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

    #[test]
    fn finds_the_socket_of_a_live_gui_process_and_ignores_dead_ones() {
        let directory = tempfile::tempdir().unwrap();
        let live_pid = std::process::id();
        // A pid this large is exceedingly unlikely to be in use (max is
        // typically 2^22 on Linux), so `kill(pid, 0)` should report ESRCH.
        File::create(directory.path().join("gui-sock-999999999")).unwrap();
        File::create(directory.path().join(format!("gui-sock-{live_pid}"))).unwrap();
        File::create(directory.path().join("not-a-gui-sock")).unwrap();
        let mut newest: Option<(SystemTime, PathBuf)> = None;
        for entry in fs::read_dir(directory.path()).unwrap().flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|name| name.strip_prefix("gui-sock-"))
            else {
                continue;
            };
            let Ok(pid) = pid.parse::<u32>() else {
                continue;
            };
            if !process_is_alive(pid) {
                continue;
            }
            newest = Some((entry.metadata().unwrap().modified().unwrap(), entry.path()));
        }
        assert_eq!(
            newest.unwrap().1,
            directory.path().join(format!("gui-sock-{live_pid}"))
        );
    }

    #[test]
    fn an_already_set_env_var_is_left_untouched() {
        // Guards the "trust the caller's own environment" branch: it must not
        // be overridden by discovery, even if discovery would find something.
        env::set_var("WEZTERM_UNIX_SOCKET", "/tmp/already-targeted.sock");
        let result = gui_socket_path();
        env::remove_var("WEZTERM_UNIX_SOCKET");
        assert!(result.is_none());
    }
}
