//! Small, explicit persistence boundary. Only UI state and user-authored
//! transcript metadata are stored; auth cookies, access tokens, and process
//! handles never enter this snapshot.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{Settings, Workspace};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub settings: Settings,
    #[serde(default)]
    pub selected_project: String,
    #[serde(default)]
    pub selected_task: String,
    #[serde(default)]
    pub sidebar_collapsed: bool,
}

impl Snapshot {
    pub fn demo() -> Self {
        let workspace = Workspace::demo();
        let project = workspace
            .projects
            .first()
            .map(|project| project.id.clone())
            .unwrap_or_default();
        let task = workspace
            .projects
            .first()
            .and_then(|project| project.tasks.first())
            .map(|task| task.id.clone())
            .unwrap_or_default();
        Self {
            workspace,
            settings: Settings::default(),
            selected_project: project,
            selected_task: task,
            sidebar_collapsed: false,
        }
    }
}

pub fn state_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CODEX_APP_GPUI_HOME") {
        return PathBuf::from(path).join("state.json");
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("codex-app-gpui/state.json");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/state/codex-app-gpui/state.json")
}

pub fn load() -> Result<Option<Snapshot>> {
    load_from(&state_path())
}

pub fn load_from(path: &Path) -> Result<Option<Snapshot>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(
            serde_json::from_str(&contents).context("decode Codex App GPUI state")?,
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read state at {}", path.display())),
    }
}

pub fn save(snapshot: &Snapshot) -> Result<()> {
    save_to(&state_path(), snapshot)
}

pub fn save_to(path: &Path, snapshot: &Snapshot) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create state directory {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(snapshot).context("encode Codex App GPUI state")?;
    fs::write(&temporary, format!("{contents}\n"))
        .with_context(|| format!("write state at {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("commit state at {}", path.display()))?;
    Ok(())
}

pub fn contains_credentials(contents: &str) -> bool {
    [
        "sk-",
        "ghp_",
        "github_pat_",
        "Bearer ",
        "refresh_token",
        "access_token",
    ]
    .iter()
    .any(|needle| contents.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_uses_atomic_file() {
        let directory =
            std::env::temp_dir().join(format!("codex-app-gpui-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let path = directory.join("state.json");
        let original = Snapshot::demo();
        save_to(&path, &original).unwrap();
        let restored = load_from(&path).unwrap().unwrap();
        assert_eq!(restored, original);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn credential_detector_rejects_secret_like_values() {
        assert!(contains_credentials("authorization: Bearer token"));
        assert!(contains_credentials("sk-example"));
        assert!(!contains_credentials("model = 5.6 Luna Max"));
    }
}
