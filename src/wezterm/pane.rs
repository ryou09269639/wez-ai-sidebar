use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSize {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeztermPane {
    pub window_id: u64,
    pub tab_id: u64,
    pub pane_id: u64,
    #[serde(default)]
    pub workspace: String,
    pub size: Option<PaneSize>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cwd: String,
}

impl WeztermPane {
    pub fn cwd_path(&self) -> String {
        if let Ok(url) = url::Url::parse(&self.cwd) {
            if url.scheme() == "file" {
                return url
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.to_str().map(ToOwned::to_owned))
                    .unwrap_or_else(|| url.path().to_owned());
            }
        }
        self.cwd.clone()
    }
}
