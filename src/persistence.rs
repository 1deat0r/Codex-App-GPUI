//! Small, explicit persistence boundary. Only UI state and user-authored
//! transcript metadata are stored; auth cookies, access tokens, and process
//! handles never enter this snapshot.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{Settings, Task, Workspace};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub settings: Settings,
    #[serde(default)]
    pub skill_roots: Vec<String>,
    #[serde(default)]
    pub selected_project: String,
    #[serde(default)]
    pub selected_task: String,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(default)]
    pub show_archived: bool,
    #[serde(default = "default_content_layout")]
    pub content_layout: String,
    #[serde(default)]
    pub bottom_panel_open: bool,
    #[serde(default)]
    pub side_panel_open: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

fn default_content_layout() -> String {
    "Chat".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShareArtifact {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    pub created_at: String,
    pub task: Task,
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
            skill_roots: Vec::new(),
            selected_project: project,
            selected_task: task,
            sidebar_collapsed: false,
            show_archived: false,
            content_layout: default_content_layout(),
            bottom_panel_open: false,
            side_panel_open: false,
            fullscreen: false,
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

pub fn share_path(id: &str) -> PathBuf {
    let parent = state_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    parent
        .join("shares")
        .join(format!("{}.json", safe_share_id(id)))
}

pub fn save_share(artifact: &ShareArtifact) -> Result<PathBuf> {
    let path = share_path(&artifact.id);
    save_share_to(&path, artifact)?;
    Ok(path)
}

pub fn save_share_to(path: &Path, artifact: &ShareArtifact) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create share directory {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(artifact).context("encode Codex share artifact")?;
    fs::write(&temporary, format!("{contents}\n"))
        .with_context(|| format!("write share artifact at {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("commit share artifact at {}", path.display()))?;
    Ok(())
}

pub fn load_share(id: &str) -> Result<Option<ShareArtifact>> {
    load_share_from(&share_path(id))
}

pub fn load_share_from(path: &Path) -> Result<Option<ShareArtifact>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(
            serde_json::from_str(&contents).context("decode Codex share artifact")?,
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read share at {}", path.display())),
    }
}

pub fn new_share_id(thread_id: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{}-{timestamp}", safe_share_id(thread_id))
}

fn safe_share_id(id: &str) -> String {
    let mut result = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if result.is_empty() {
        result.push_str("share");
    }
    result.truncate(120);
    result
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
    fn snapshot_round_trip_preserves_view_and_reference_settings() {
        let directory = std::env::temp_dir().join(format!(
            "codex-app-gpui-settings-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let path = directory.join("state.json");
        let mut original = Snapshot::demo();
        original.content_layout = "Review".into();
        original.bottom_panel_open = true;
        original.side_panel_open = true;
        original.fullscreen = true;
        original.settings.enter_behavior = "newline".into();
        original.settings.language = "English".into();
        original.settings.worktree_auto_cleanup = true;
        original.settings.worktree_keep_count = 20;
        original.settings.custom_instructions = "Prefer concise evidence".into();
        original.settings.commit_instructions = "Run the full suite".into();
        original.settings.pull_request_instructions = "Run the checks".into();
        original.settings.pull_request_watch_instructions = "Fix failing checks".into();
        original.settings.projectless_task_folder = "/tmp/projectless".into();
        original.skill_roots = vec!["/tmp/skills".into()];
        save_to(&path, &original).unwrap();
        let restored = load_from(&path).unwrap().unwrap();
        assert_eq!(restored, original);
        let encoded = fs::read_to_string(&path).unwrap();
        assert!(!contains_credentials(&encoded));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn snapshot_migration_defaults_view_and_settings_fields() {
        let demo = Snapshot::demo();
        let legacy = serde_json::json!({
            "workspace": demo.workspace,
            "settings": {},
            "selected_project": "codex-app-gpui",
            "selected_task": "codex-app-gpui-parity"
        });
        let restored: Snapshot = serde_json::from_value(legacy).unwrap();
        assert!(restored.skill_roots.is_empty());
        assert_eq!(restored.content_layout, "Chat");
        assert!(!restored.bottom_panel_open);
        assert!(!restored.side_panel_open);
        assert!(!restored.fullscreen);
        assert_eq!(restored.settings.enter_behavior, "send");
        assert_eq!(restored.settings.language, "system");
    }

    #[test]
    fn credential_detector_rejects_secret_like_values() {
        assert!(contains_credentials("authorization: Bearer token"));
        assert!(contains_credentials("sk-example"));
        assert!(!contains_credentials("model = 5.6 Luna Max"));
    }

    #[test]
    fn share_artifacts_are_atomic_and_reject_path_traversal_ids() {
        let directory =
            std::env::temp_dir().join(format!("codex-app-gpui-share-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let path = directory.join("nested/share.json");
        let mut artifact = ShareArtifact {
            id: "../thread/one".into(),
            thread_id: "thread-1".into(),
            title: "Shared".into(),
            created_at: "now".into(),
            task: Snapshot::demo().workspace.projects[0].tasks[0].clone(),
        };
        save_share_to(&path, &artifact).unwrap();
        artifact.task.title = "Changed locally".into();
        let restored = load_share_from(&path).unwrap().unwrap();
        assert_eq!(restored.title, "Shared");
        assert_eq!(new_share_id("../thread/one").contains('/'), false);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(directory);
    }
}
