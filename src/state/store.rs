use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::Utc;

use super::{AgentState, AgentStatus};

#[derive(Debug)]
pub struct StateStore {
    states: HashMap<String, AgentState>,
    directory: PathBuf,
}

impl StateStore {
    pub fn load(directory: impl Into<PathBuf>) -> Result<Self> {
        let directory = directory.into();
        fs::create_dir_all(&directory)?;
        let mut store = Self {
            states: HashMap::new(),
            directory,
        };
        for entry in fs::read_dir(&store.directory)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => continue,
            };
            if let Ok(state) = serde_json::from_str::<AgentState>(&text) {
                store.states.insert(state.key(), state);
            }
        }
        Ok(store)
    }

    pub fn upsert(&mut self, state: AgentState) -> Result<Option<AgentStatus>> {
        let key = state.key();
        let previous = self.states.get(&key).map(|item| item.status);
        if state.wezterm.pane_id.is_some() {
            let superseded = self
                .states
                .iter()
                .filter(|(other_key, item)| {
                    *other_key != &key
                        && item.wezterm.pane_id == state.wezterm.pane_id
                        && (item.agent == state.agent
                            || item.source == super::DetectionSource::Process)
                })
                .map(|(other_key, _)| other_key.clone())
                .collect::<Vec<_>>();
            for key in superseded {
                if let Some(old) = self.states.remove(&key) {
                    let path = self.state_path(&old);
                    if path.exists() {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        self.persist(&state)?;
        self.states.insert(key, state);
        Ok(previous)
    }

    pub fn snapshot(&self) -> Vec<AgentState> {
        let mut values = self.states.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| {
            priority(left.status)
                .cmp(&priority(right.status))
                .then_with(|| left.agent.display_name().cmp(right.agent.display_name()))
                .then_with(|| left.project.cmp(&right.project))
        });
        values
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut AgentState> {
        self.states.get_mut(key)
    }

    pub fn remove(&mut self, key: &str) -> Result<bool> {
        let Some(state) = self.states.remove(key) else {
            return Ok(false);
        };
        let path = self.state_path(&state);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(true)
    }

    pub fn prune(&mut self, max_age: Duration) -> Result<usize> {
        let now = Utc::now();
        let old_keys = self
            .states
            .iter()
            .filter(|(_, state)| {
                if state
                    .wezterm
                    .pane_id
                    .is_some_and(|pane_id| state.id == format!("pane-{pane_id}"))
                {
                    return false;
                }
                now.signed_duration_since(state.updated_at)
                    .to_std()
                    .map(|age| age > max_age)
                    .unwrap_or(false)
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in &old_keys {
            self.remove(key)?;
        }
        Ok(old_keys.len())
    }

    fn persist(&self, state: &AgentState) -> Result<()> {
        let path = self.state_path(state);
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(state)?;
        fs::write(&temporary, bytes)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("failed to replace {}", path.display()))
    }

    fn state_path(&self, state: &AgentState) -> PathBuf {
        self.directory
            .join(format!("{}.json", safe_name(&state.key())))
    }
}

pub fn load_snapshot(directory: &Path) -> Result<Vec<AgentState>> {
    Ok(StateStore::load(directory.to_path_buf())?.snapshot())
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn priority(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::PermissionRequired | AgentStatus::WaitingInput => 0,
        AgentStatus::Working => 1,
        AgentStatus::Idle => 2,
        AgentStatus::Unknown => 3,
        AgentStatus::Error => 4,
        AgentStatus::Done => 5,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration as ChronoDuration, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::state::AgentKind;

    #[test]
    fn supports_multiple_instances_of_one_agent() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        store
            .upsert(AgentState::new("one", AgentKind::Claude, "/tmp/one"))
            .unwrap();
        store
            .upsert(AgentState::new("two", AgentKind::Claude, "/tmp/two"))
            .unwrap();
        assert_eq!(store.snapshot().len(), 2);
    }

    #[test]
    fn removes_stale_agents() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        let mut state = AgentState::new("old", AgentKind::Codex, "/tmp/old");
        state.updated_at = Utc::now() - ChronoDuration::hours(2);
        store.upsert(state).unwrap();
        assert_eq!(store.prune(Duration::from_secs(60)).unwrap(), 1);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn state_transitions_replace_the_same_session() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        let mut state = AgentState::new("same", AgentKind::Claude, "/tmp/project");
        state.status = AgentStatus::Working;
        store.upsert(state.clone()).unwrap();
        state.status = AgentStatus::PermissionRequired;
        assert_eq!(store.upsert(state).unwrap(), Some(AgentStatus::Working));
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].status, AgentStatus::PermissionRequired);
    }

    #[test]
    fn a_new_session_in_the_same_pane_supersedes_the_old_one() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        let mut old = AgentState::new("old", AgentKind::Claude, "/tmp/project");
        old.wezterm.pane_id = Some(12);
        store.upsert(old).unwrap();
        let mut new = AgentState::new("new", AgentKind::Claude, "/tmp/project");
        new.wezterm.pane_id = Some(12);
        store.upsert(new).unwrap();
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, "new");
    }

    #[test]
    fn active_agents_sort_before_terminal_states() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        for (id, status) in [
            ("done", AgentStatus::Done),
            ("idle", AgentStatus::Idle),
            ("working", AgentStatus::Working),
            ("permission", AgentStatus::PermissionRequired),
            ("error", AgentStatus::Error),
        ] {
            let mut state = AgentState::new(id, AgentKind::Codex, format!("/tmp/{id}"));
            state.status = status;
            store.upsert(state).unwrap();
        }
        let statuses = store
            .snapshot()
            .into_iter()
            .map(|state| state.status)
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec![
                AgentStatus::PermissionRequired,
                AgentStatus::Working,
                AgentStatus::Idle,
                AgentStatus::Error,
                AgentStatus::Done,
            ]
        );
    }

    #[test]
    fn remove_deletes_memory_and_persisted_state() {
        let dir = tempdir().unwrap();
        let mut store = StateStore::load(dir.path()).unwrap();
        let state = AgentState::new("closed", AgentKind::Codex, "/tmp/project");
        let key = state.key();
        store.upsert(state).unwrap();
        assert!(store.remove(&key).unwrap());
        assert!(store.snapshot().is_empty());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
