use std::path::PathBuf;

use anyhow::{Context, Result};

pub const APP_NAME: &str = "wez-ai-sidebar";

pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .context("cannot determine the user config directory")
        .map(|path| path.join(APP_NAME))
}

pub fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .context("cannot determine the user cache directory")
        .map(|path| path.join(APP_NAME))
}

pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("daemon.sock"))
}

pub fn state_dir() -> Result<PathBuf> {
    Ok(cache_dir()?.join("agents"))
}

pub fn inbox_dir() -> Result<PathBuf> {
    Ok(cache_dir()?.join("inbox"))
}
