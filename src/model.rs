//! Product data types shared by the GPUI surface, persistence, and protocol
//! adapters. These types intentionally describe the user-visible contract and
//! do not contain credentials or process handles.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub projects: Vec<Project>,
    #[serde(default)]
    pub automations: Vec<Automation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Automation {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub schedule: String,
    #[serde(default = "default_automation_status")]
    pub status: String,
    #[serde(default)]
    pub next_run: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub task_id: Option<String>,
}

fn default_automation_status() -> String {
    "active".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Goal {
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub token_budget: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub project_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub plan: Vec<PlanStep>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub goal: Option<Goal>,
    #[serde(default)]
    pub children: Vec<ChildTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub label: String,
    #[serde(default = "default_step_status")]
    pub status: String,
}

fn default_step_status() -> String {
    "pending".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    User {
        id: String,
        text: String,
        #[serde(default)]
        time: String,
    },
    Assistant {
        id: String,
        text: String,
        #[serde(default)]
        time: String,
    },
    Reasoning {
        id: String,
        text: String,
        #[serde(default)]
        collapsed: bool,
    },
    Tool {
        id: String,
        name: String,
        status: String,
        #[serde(default)]
        detail: String,
        #[serde(default)]
        output: String,
    },
    Code {
        id: String,
        language: String,
        code: String,
        #[serde(default)]
        output: String,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    Diff {
        id: String,
        path: String,
        #[serde(default)]
        additions: u32,
        #[serde(default)]
        deletions: u32,
        #[serde(default)]
        summary: String,
    },
    Approval {
        id: String,
        title: String,
        command: String,
        cwd: String,
        #[serde(default)]
        reason: String,
        #[serde(default)]
        requested: bool,
        #[serde(default)]
        approval_kind: String,
        #[serde(default)]
        choices: Vec<String>,
        #[serde(default)]
        request_details: String,
    },
    Attachment {
        id: String,
        name: String,
        attachment_kind: String,
    },
    System {
        id: String,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cached: u64,
    #[serde(default)]
    pub context: u64,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cached += other.cached;
        self.context = other.context.max(self.context);
    }

    pub fn cache_rate(&self) -> Option<u64> {
        let total = self.cached + self.input;
        (total > 0).then(|| self.cached.saturating_mul(100) / total)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default = "default_reasoning")]
    pub default_reasoning: String,
    #[serde(default = "default_approval_mode")]
    pub approval_mode: String,
    #[serde(default = "default_sandbox_mode")]
    pub sandbox_mode: String,
    #[serde(default = "default_font_size")]
    pub font_size: u8,
    #[serde(default = "default_true")]
    pub notifications: bool,
    #[serde(default = "default_true")]
    pub sound_effects: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub worktree_root: String,
}

fn default_theme() -> String {
    "system".into()
}
fn default_model() -> String {
    "5.6 Luna Max".into()
}
fn default_reasoning() -> String {
    "max".into()
}
fn default_approval_mode() -> String {
    "on-request".into()
}
fn default_sandbox_mode() -> String {
    "workspace-write".into()
}
fn default_font_size() -> u8 {
    14
}
fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            default_model: default_model(),
            default_reasoning: default_reasoning(),
            approval_mode: default_approval_mode(),
            sandbox_mode: default_sandbox_mode(),
            font_size: default_font_size(),
            notifications: true,
            sound_effects: true,
            reduced_motion: false,
            worktree_root: String::new(),
        }
    }
}

impl Workspace {
    pub fn demo() -> Self {
        let active_project = Project {
            id: "codex-app-gpui".into(),
            name: "Codex-App-GPUI".into(),
            path: "/run/media/mustbearnold/Projects/AI Agents/Codex-App-GPUI".into(),
            collapsed: false,
            tasks: vec![Task::parity_demo()],
        };
        let project = |id: &str, name: &str, title: &str| Project {
            id: id.into(),
            name: name.into(),
            path: format!("/projects/{name}"),
            collapsed: false,
            tasks: vec![Task {
                id: format!("{id}-task"),
                title: title.into(),
                project_id: id.into(),
                status: "idle".into(),
                path: format!("/projects/{name}"),
                branch: None,
                model: default_model(),
                reasoning: default_reasoning(),
                updated_at: "Yesterday".into(),
                archived: false,
                pinned: false,
                entries: Vec::new(),
                plan: Vec::new(),
                usage: Usage::default(),
                goal: None,
                children: Vec::new(),
            }],
        };
        Self {
            projects: vec![
                active_project,
                project(
                    "webmcp",
                    "WebMCP Challenge Hackathon",
                    "Micro-factory Order Desk",
                ),
                project("openputty", "OpenPutty", "Plan OpenPutty architecture"),
                project(
                    "cabinet-linux",
                    "Cabinet-Linux",
                    "Ship the Linux portable build",
                ),
                project("praxis", "Praxis", "Continue Praxis development"),
                project("asi-agent", "ASI-Agent", "Review the development runtime"),
                project("type-agent", "Type Agent", "Finish TypeScript parity"),
                project(
                    "fluid-harness",
                    "Fluid-Harness",
                    "Design the next harness slice",
                ),
                project("pi-rust", "pi-rust", "Finish pi-rust conversion"),
            ],
            automations: vec![Automation {
                id: "parity-review-automation".into(),
                name: "Parity review checkpoint".into(),
                prompt: "Review the current parity ledger and run the exhaustive gates.".into(),
                schedule: "Every weekday at 09:00".into(),
                status: "active".into(),
                next_run: "Next weekday".into(),
                project_id: "codex-app-gpui".into(),
                task_id: Some("codex-app-gpui-parity".into()),
            }],
        }
    }

    pub fn task(&self, project_id: &str, task_id: &str) -> Option<&Task> {
        self.projects
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.tasks.iter().find(|task| task.id == task_id))
    }

    pub fn task_mut(&mut self, project_id: &str, task_id: &str) -> Option<&mut Task> {
        self.projects
            .iter_mut()
            .find(|project| project.id == project_id)
            .and_then(|project| project.tasks.iter_mut().find(|task| task.id == task_id))
    }

    pub fn all_tasks(&self) -> impl Iterator<Item = (&Project, &Task)> {
        self.projects
            .iter()
            .flat_map(|project| project.tasks.iter().map(move |task| (project, task)))
    }

    pub fn task_by_id(&self, task_id: &str) -> Option<(&Project, &Task)> {
        self.all_tasks().find(|(_, task)| task.id == task_id)
    }
}

impl Task {
    pub fn parity_demo() -> Self {
        Self {
            id: "codex-app-gpui-parity".into(),
            title: "Achieve Codex App GPUI parity".into(),
            project_id: "codex-app-gpui".into(),
            status: "running".into(),
            path: "/run/media/mustbearnold/Projects/AI Agents/Codex-App-GPUI".into(),
            branch: Some("main".into()),
            model: "5.6 Luna Max".into(),
            reasoning: "max".into(),
            updated_at: "Now".into(),
            archived: false,
            pinned: false,
            entries: vec![
                Entry::User {
                    id: "demo-user-1".into(),
                    text: "Make 1:1 Codex App > Codex App GPUI\n(100% parity in every single avenue)".into(),
                    time: "Now".into(),
                },
                Entry::Assistant {
                    id: "demo-assistant-1".into(),
                    text: "I’m treating this as a parity implementation and verification task. I’m inventorying the reference shell, building the native GPUI surface, and keeping each claim tied to a runnable or manually recorded gate.".into(),
                    time: "Now".into(),
                },
                Entry::Tool {
                    id: "demo-tool-1".into(),
                    name: "workspace inspection".into(),
                    status: "complete".into(),
                    detail: "Loaded a tool, read files, and ran commands".into(),
                    output: "Reference window measured; local app-server contract identified.".into(),
                },
            ],
            plan: vec![
                PlanStep {
                    label: "Inventory reference surface".into(),
                    status: "complete".into(),
                },
                PlanStep {
                    label: "Implement GPUI shell and interaction model".into(),
                    status: "running".into(),
                },
                PlanStep {
                    label: "Connect app-server protocol".into(),
                    status: "pending".into(),
                },
                PlanStep {
                    label: "Exercise primary flows".into(),
                    status: "pending".into(),
                },
                PlanStep {
                    label: "Re-verify parity evidence".into(),
                    status: "pending".into(),
                },
            ],
            usage: Usage {
                input: 1_900,
                output: 624,
                cached: 1_420,
                context: 32_000,
            },
            goal: Some(Goal {
                objective: "Reach evidence-backed 100% parity across the reference surface".into(),
                status: "active".into(),
                token_budget: None,
            }),
            children: vec![
                ChildTask {
                    id: "reference-audit".into(),
                    title: "Reference surface audit".into(),
                    status: "complete".into(),
                },
                ChildTask {
                    id: "protocol-slice".into(),
                    title: "App-server protocol slice".into(),
                    status: "running".into(),
                },
            ],
        }
    }

    pub fn title_from_prompt(prompt: &str) -> String {
        let normalized = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut title = normalized.chars().take(48).collect::<String>();
        if normalized.chars().count() > 48 {
            title.push('…');
        }
        if title.is_empty() {
            "New task".into()
        } else {
            title
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Task,
    PullRequests,
    Sites,
    Scheduled,
    Plugins,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Account,
    Appearance,
    Notifications,
    Apps,
    Mcp,
    Skills,
    Plugins,
    Keybindings,
    Worktrees,
    Git,
}

impl SettingsPage {
    pub const ALL: &[Self] = &[
        Self::General,
        Self::Account,
        Self::Appearance,
        Self::Notifications,
        Self::Apps,
        Self::Mcp,
        Self::Skills,
        Self::Plugins,
        Self::Keybindings,
        Self::Worktrees,
        Self::Git,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Account => "Account",
            Self::Appearance => "Appearance",
            Self::Notifications => "Notifications",
            Self::Apps => "Apps & Connectors",
            Self::Mcp => "MCP",
            Self::Skills => "Skills",
            Self::Plugins => "Plugins",
            Self::Keybindings => "Keybindings",
            Self::Worktrees => "Worktrees",
            Self::Git => "Git",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "⚙",
            Self::Account => "◎",
            Self::Appearance => "◐",
            Self::Notifications => "◌",
            Self::Apps => "⊞",
            Self::Mcp => "⌘",
            Self::Skills => "✦",
            Self::Plugins => "▦",
            Self::Keybindings => "⌨",
            Self::Worktrees => "⑂",
            Self::Git => "●",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_workspace_contains_reference_shell_projects() {
        let workspace = Workspace::demo();
        assert_eq!(
            workspace.projects.first().map(|p| p.name.as_str()),
            Some("Codex-App-GPUI")
        );
        assert!(workspace
            .all_tasks()
            .any(|(_, task)| task.title == "Achieve Codex App GPUI parity"));
    }

    #[test]
    fn prompt_titles_are_compact_and_nonempty() {
        assert_eq!(Task::title_from_prompt("  hello   world "), "hello world");
        assert_eq!(Task::title_from_prompt(""), "New task");
        assert!(Task::title_from_prompt(&"x".repeat(80)).chars().count() <= 49);
    }

    #[test]
    fn usage_tracks_cache_rate() {
        let usage = Usage {
            input: 20,
            output: 4,
            cached: 80,
            context: 100,
        };
        assert_eq!(usage.cache_rate(), Some(80));
    }
}
