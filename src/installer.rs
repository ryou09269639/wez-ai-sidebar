use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::process::Command;

use crate::{config::Config, paths};

const OWNED_MARKER: &str = "wez-ai-sidebar";

pub async fn install(enable_service: bool) -> Result<()> {
    let config_file = paths::config_file()?;
    let created = Config::write_default_if_missing(&config_file)?;
    println!(
        "config                 {} ({})",
        config_file.display(),
        if created { "created" } else { "kept" }
    );
    install_lua()?;
    install_claude()?;
    install_codex()?;
    install_opencode()?;
    install_copilot()?;
    install_antigravity()?;
    let service = install_service()?;
    if enable_service {
        let daemon_reload = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()
            .await;
        let enabled = Command::new("systemctl")
            .args(["--user", "enable", "--now", "wez-ai-sidebar.service"])
            .status()
            .await;
        if !daemon_reload
            .map(|status| status.success())
            .unwrap_or(false)
            || !enabled.map(|status| status.success()).unwrap_or(false)
        {
            println!(
                "systemd               installed at {}; enable it manually",
                service.display()
            );
        }
    }
    println!("\nAdd the generated WezTerm snippet shown below to your config:");
    println!(
        "  {}",
        paths::config_dir()?.join("wezterm.lua.snippet").display()
    );
    println!("Existing WezTerm Lua was not edited because arbitrary `return config` layouts cannot be merged safely.");
    Ok(())
}

pub async fn uninstall() -> Result<()> {
    let home = home_dir()?;
    let config_home = dirs::config_dir().context("cannot determine config directory")?;
    for path in [
        home.join(".claude/settings.json"),
        home.join(".codex/hooks.json"),
        home.join(".copilot/hooks/wez-ai-sidebar.json"),
        home.join(".gemini/config/hooks.json"),
    ] {
        cleanup_owned_json(&path)?;
    }
    for path in [
        config_home.join("opencode/plugins/wez-ai-sidebar.js"),
        config_home.join("wezterm/wez-ai-sidebar.lua"),
        paths::config_dir()?.join("wezterm.lua.snippet"),
        config_home.join("systemd/user/wez-ai-sidebar.service"),
    ] {
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
            println!("removed                {}", path.display());
        }
    }
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "wez-ai-sidebar.service"])
        .status()
        .await;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .await;
    println!("User config and cached status were preserved.");
    Ok(())
}

fn install_lua() -> Result<()> {
    let wezterm_dir = dirs::config_dir()
        .context("cannot determine config directory")?
        .join("wezterm");
    fs::create_dir_all(&wezterm_dir)?;
    let module = wezterm_dir.join("wez-ai-sidebar.lua");
    write_owned(&module, include_str!("../wezterm/wez-ai-sidebar.lua"))?;
    let snippet = paths::config_dir()?.join("wezterm.lua.snippet");
    let text = format!(
        "package.path = package.path .. ';{}/?.lua'\nlocal wez_ai = require('wez-ai-sidebar')\nwez_ai.setup(config, {{ width = 26, position = 'left', auto_create = true }})\n",
        wezterm_dir.display()
    );
    write_owned(&snippet, &text)?;
    println!("WezTerm module          {}", module.display());
    Ok(())
}

fn install_claude() -> Result<()> {
    let path = home_dir()?.join(".claude/settings.json");
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
        "Notification",
        "Stop",
        "StopFailure",
        "SessionEnd",
    ];
    let mut hooks = serde_json::Map::new();
    for event in events {
        hooks.insert(
            event.to_owned(),
            json!([{"matcher":"","hooks":[{"type":"command","command":format!("wez-ai-sidebar hook claude {event}"),"timeout":5}]}]),
        );
    }
    merge_json_file(&path, json!({"hooks": hooks}))?;
    println!("Claude integration     {}", path.display());
    Ok(())
}

fn install_codex() -> Result<()> {
    let path = home_dir()?.join(".codex/hooks.json");
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "SessionEnd",
    ];
    let mut hooks = serde_json::Map::new();
    for event in events {
        hooks.insert(
            event.to_owned(),
            json!([{"matcher":"","hooks":[{"type":"command","command":format!("wez-ai-sidebar hook codex {event}"),"timeout":5}]}]),
        );
    }
    merge_json_file(&path, json!({"hooks": hooks}))?;
    println!("Codex integration      {}", path.display());
    Ok(())
}

fn install_opencode() -> Result<()> {
    let path = dirs::config_dir()
        .context("cannot determine config directory")?
        .join("opencode/plugins/wez-ai-sidebar.js");
    write_owned(&path, include_str!("../integrations/opencode.js"))?;
    println!("OpenCode integration   {}", path.display());
    Ok(())
}

fn install_copilot() -> Result<()> {
    let path = home_dir()?.join(".copilot/hooks/wez-ai-sidebar.json");
    let config = json!({
        "version": 1,
        "hooks": {
            "sessionStart": [copilot_hook("sessionStart")],
            "userPromptSubmitted": [copilot_hook("userPromptSubmitted")],
            "preToolUse": [copilot_hook("preToolUse")],
            "postToolUse": [copilot_hook("postToolUse")],
            "agentStop": [copilot_hook("agentStop")],
            "errorOccurred": [copilot_hook("errorOccurred")],
            "notification": [copilot_hook("Notification")]
        }
    });
    write_owned(&path, &serde_json::to_string_pretty(&config)?)?;
    println!("Copilot integration    {}", path.display());
    Ok(())
}

fn install_antigravity() -> Result<()> {
    let path = home_dir()?.join(".gemini/config/hooks.json");
    // Antigravity currently has no post-routing PermissionRequest hook. Installing
    // PreToolUse with decision=allow would auto-approve and decision=ask would
    // change user policy, so only passive lifecycle hooks are installed.
    let config = json!({
        "wez-ai-sidebar": {
            "PostInvocation": [{"type":"command","command":"wez-ai-sidebar hook antigravity PostInvocation","timeout":5}],
            "Stop": [{"type":"command","command":"wez-ai-sidebar hook antigravity Stop","timeout":5}]
        }
    });
    merge_json_file(&path, config)?;
    println!(
        "Antigravity integration {} (permission uses pane fallback)",
        path.display()
    );
    Ok(())
}

fn install_service() -> Result<PathBuf> {
    let path = dirs::config_dir()
        .context("cannot determine config directory")?
        .join("systemd/user/wez-ai-sidebar.service");
    let executable = std::env::current_exe().context("cannot resolve installed executable")?;
    let service = include_str!("../systemd/wez-ai-sidebar.service")
        .replace("@BINARY@", &executable.to_string_lossy());
    write_owned(&path, &service)?;
    println!("systemd user service   {}", path.display());
    Ok(path)
}

fn copilot_hook(event: &str) -> Value {
    json!({
        "type": "command",
        "bash": format!("wez-ai-sidebar hook copilot {event}"),
        "timeoutSec": 5
    })
}

fn merge_json_file(path: &Path, addition: Value) -> Result<()> {
    let mut current = if path.exists() {
        serde_json::from_str::<Value>(&fs::read_to_string(path)?)
            .with_context(|| format!("refusing to modify invalid JSON at {}", path.display()))?
    } else {
        json!({})
    };
    let before = current.clone();
    merge_value(&mut current, addition);
    if current == before {
        return Ok(());
    }
    write_owned(path, &serde_json::to_string_pretty(&current)?)
}

fn merge_value(current: &mut Value, addition: Value) {
    match (current, addition) {
        (Value::Object(current), Value::Object(addition)) => {
            for (key, value) in addition {
                if let Some(existing) = current.get_mut(&key) {
                    merge_value(existing, value);
                } else {
                    current.insert(key, value);
                }
            }
        }
        (Value::Array(current), Value::Array(addition)) => {
            for value in addition {
                if !current.contains(&value) {
                    current.push(value);
                }
            }
        }
        (current, addition) if current.is_null() => *current = addition,
        _ => {}
    }
}

fn cleanup_owned_json(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(path)?;
    if !text.contains(OWNED_MARKER) {
        return Ok(());
    }
    let mut value: Value = serde_json::from_str(&text)
        .with_context(|| format!("refusing to modify invalid JSON at {}", path.display()))?;
    cleanup_value(&mut value);
    backup(path)?;
    write_raw(path, &serde_json::to_string_pretty(&value)?)?;
    println!("cleaned                {}", path.display());
    Ok(())
}

fn cleanup_value(value: &mut Value) {
    match value {
        Value::Array(values) => {
            values.retain(|item| !item.to_string().contains(OWNED_MARKER));
            for item in values {
                cleanup_value(item);
            }
        }
        Value::Object(values) => {
            values.retain(|key, item| {
                key != OWNED_MARKER
                    && (!item.is_string() || !item.to_string().contains(OWNED_MARKER))
            });
            for item in values.values_mut() {
                cleanup_value(item);
            }
        }
        _ => {}
    }
}

fn write_owned(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing == text {
            return Ok(());
        }
        backup(path)?;
    }
    write_raw(path, text)
}

fn write_raw(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn backup(path: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let backup = path.with_extension(format!("bak.{timestamp}"));
    fs::copy(path, &backup)?;
    println!("backup                 {}", backup.display());
    Ok(backup)
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("cannot determine home directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_existing_hooks() {
        let mut current = json!({"hooks":{"Stop":[{"command":"mine"}]}});
        merge_value(
            &mut current,
            json!({"hooks":{"Stop":[{"command":"wez-ai-sidebar hook claude Stop"}]}}),
        );
        assert_eq!(current["hooks"]["Stop"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn cleanup_keeps_unrelated_entries() {
        let mut value = json!({"hooks":{"Stop":[{"command":"mine"},{"command":"wez-ai-sidebar hook codex Stop"}]}});
        cleanup_value(&mut value);
        let entries = value["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["command"], "mine");
    }
}
