use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::state::AgentState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    Snapshot,
    Upsert { state: AgentState },
    Remove { key: String },
    Rescan,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerResponse {
    Snapshot { agents: Vec<AgentState> },
    Ok,
    Pong,
    Error { message: String },
}

pub async fn request(socket: &Path, request: &ClientRequest) -> Result<ServerResponse> {
    let mut stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("cannot connect to {}", socket.display()))?;
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await?;
    if line.is_empty() {
        anyhow::bail!("daemon closed the socket without a response");
    }
    serde_json::from_str(&line).context("daemon returned invalid JSON")
}

pub async fn snapshot(socket: &Path) -> Result<Vec<AgentState>> {
    match request(socket, &ClientRequest::Snapshot).await? {
        ServerResponse::Snapshot { agents } => Ok(agents),
        ServerResponse::Error { message } => anyhow::bail!(message),
        response => anyhow::bail!("unexpected daemon response: {response:?}"),
    }
}
