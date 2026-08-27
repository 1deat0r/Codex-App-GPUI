//! UI state and interaction reducer for the native client.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AsyncApp, Context, FocusHandle, KeyDownEvent, PathPromptOptions, WeakEntity, Window};
use serde_json::{json, Value};

use crate::model::{
    Automation, ChildTask, Entry, Goal, PlanStep, Project, QueuedInput, Route, Settings,
    SettingsPage, Task, Workspace,
};
use crate::persistence::{self, ShareArtifact, Snapshot};
use crate::protocol::{AppServerClient, ServerThread};

pub const MODEL_OPTIONS: &[&str] = &["5.6 Luna Max", "5.6 Sol", "5.5", "5.4 Mini"];
pub const REASONING_OPTIONS: &[&str] = &["auto", "low", "high", "max"];
pub const COMPOSER_MODES: &[&str] = &["Agent", "Chat", "Ask"];
pub const CONTENT_LAYOUTS: &[&str] = &[
    "Chat",
    "Task tabs",
    "Files",
    "Side chat",
    "Browser",
    "Review",
    "Detail",
    "Terminal",
];

fn normalize_content_layout(layout: &str) -> &str {
    CONTENT_LAYOUTS
        .iter()
        .copied()
        .find(|candidate| *candidate == layout)
        .unwrap_or("Chat")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    pub reasoning: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerCatalog {
    pub models: Vec<ModelOption>,
    pub reasoning: Vec<String>,
    pub permissions: Vec<String>,
    pub collaboration_modes: Vec<String>,
    pub apps: Vec<String>,
    pub installed_apps: Vec<String>,
    pub plugins: Vec<String>,
    pub available_plugins: Vec<String>,
    pub skills: Vec<String>,
    pub hooks: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub account_label: Option<String>,
    pub config_summary: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
    pub branch: String,
    pub author: String,
    pub review_decision: String,
    pub checks: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitHubState {
    pub repository: String,
    pub status: String,
    pub pull_requests: Vec<PullRequestSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeSummary {
    pub path: String,
    pub head: String,
    pub branch: String,
    pub is_main: bool,
}

impl ServerCatalog {
    fn from_client(client: &AppServerClient, cwd: Option<&str>) -> Self {
        let mut catalog = Self::default();
        if let Ok(value) = client.model_list() {
            catalog.models = model_options_from_value(&value);
            catalog.reasoning = catalog
                .models
                .iter()
                .flat_map(|model| model.reasoning.iter().cloned())
                .collect::<Vec<_>>();
            dedupe_strings(&mut catalog.reasoning);
        }
        if let Ok(value) = client.permission_profile_list(cwd) {
            catalog.permissions = named_values_from_data(&value, &["id", "name", "label"]);
        }
        if let Ok(value) = client.collaboration_mode_list() {
            catalog.collaboration_modes = named_values_from_data(&value, &["name", "mode", "id"]);
        }
        if let Ok(value) = client.apps_list(None) {
            catalog.apps = named_values_from_data(&value, &["name", "displayName", "id"]);
        }
        if let Ok(value) = client.apps_installed(None) {
            catalog.installed_apps = named_values_from_data(&value, &["name", "displayName", "id"]);
        }
        if let Ok(value) = client.plugin_list() {
            catalog.available_plugins =
                named_values_from_data(&value, &["name", "displayName", "id"]);
            if catalog.available_plugins.is_empty() {
                catalog.available_plugins = nested_named_values_from_data(
                    &value,
                    &["marketplaces", "plugins", "data"],
                    &["name", "displayName", "id"],
                );
            }
        }
        if let Ok(value) = client.plugin_installed(
            Some(
                &cwd.filter(|cwd| !cwd.is_empty())
                    .map(|cwd| vec![cwd.to_owned()])
                    .unwrap_or_default(),
            ),
            None,
        ) {
            catalog.plugins = nested_named_values_from_data(
                &value,
                &["plugins", "installed", "data"],
                &["name", "displayName", "id"],
            );
        }
        if catalog.plugins.is_empty() {
            catalog.plugins = catalog.available_plugins.clone();
        }
        if let Ok(value) = client.skills_list(
            &cwd.filter(|cwd| !cwd.is_empty())
                .map(|cwd| vec![cwd.to_owned()])
                .unwrap_or_default(),
        ) {
            catalog.skills = nested_named_values_from_data(
                &value,
                &["skills", "data"],
                &["name", "displayName", "id"],
            );
        }
        if let Ok(value) = client.hooks_list(Some(
            &cwd.filter(|cwd| !cwd.is_empty())
                .map(|cwd| vec![cwd.to_owned()])
                .unwrap_or_default(),
        )) {
            catalog.hooks = nested_named_values_from_data(
                &value,
                &["hooks", "data"],
                &["name", "displayName", "id", "event"],
            );
            if catalog.hooks.is_empty() {
                catalog.hooks =
                    named_values_from_data(&value, &["name", "displayName", "id", "event"]);
            }
        }
        if let Ok(value) = client.mcp_server_status_list(None) {
            catalog.mcp_servers = named_values_from_data(&value, &["name", "displayName", "id"]);
        }
        if let Ok(value) = client.account_read() {
            catalog.account_label = account_label_from_value(&value);
        }
        if let Ok(value) = client.config_read(cwd, false) {
            catalog.config_summary = config_summary_from_value(&value);
        }
        catalog
    }

    fn model_ids(&self) -> Vec<String> {
        if self.models.is_empty() {
            MODEL_OPTIONS.iter().map(|model| (*model).into()).collect()
        } else {
            self.models.iter().map(|model| model.id.clone()).collect()
        }
    }

    fn reasoning_options(&self) -> Vec<String> {
        if self.reasoning.is_empty() {
            REASONING_OPTIONS
                .iter()
                .map(|level| (*level).into())
                .collect()
        } else {
            self.reasoning.clone()
        }
    }

    pub fn model_label(&self, id: &str) -> String {
        self.models
            .iter()
            .find(|model| model.id == id)
            .map(|model| model.label.clone())
            .unwrap_or_else(|| id.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Demo,
    Connecting,
    Live,
    Offline,
}

impl ConnectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Demo => "Demo data",
            Self::Connecting => "Connecting…",
            Self::Live => "Connected to Codex",
            Self::Offline => "Offline",
        }
    }
}

/// The app-server has several server-initiated interaction contracts. Keeping
/// the request kind and original params together prevents the UI from sending
/// a command-approval payload for a permissions or MCP request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    CommandApproval,
    FileChangeApproval,
    PermissionsApproval,
    UserInput,
    McpElicitation,
    LegacyCommandApproval,
    LegacyPatchApproval,
    DynamicToolCall,
    Unknown,
}

impl InteractionKind {
    pub fn from_method(method: &str) -> Self {
        match method {
            "item/commandExecution/requestApproval" => Self::CommandApproval,
            "item/fileChange/requestApproval" => Self::FileChangeApproval,
            "item/permissions/requestApproval" => Self::PermissionsApproval,
            "item/tool/requestUserInput" => Self::UserInput,
            "mcpServer/elicitation/request" => Self::McpElicitation,
            "execCommandApproval" => Self::LegacyCommandApproval,
            "applyPatchApproval" => Self::LegacyPatchApproval,
            "item/tool/call" => Self::DynamicToolCall,
            _ => Self::Unknown,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::CommandApproval | Self::LegacyCommandApproval => "Run command",
            Self::FileChangeApproval | Self::LegacyPatchApproval => "Apply file changes",
            Self::PermissionsApproval => "Grant additional permissions",
            Self::UserInput => "Codex needs input",
            Self::McpElicitation => "MCP server needs input",
            Self::DynamicToolCall => "Dynamic tool call",
            Self::Unknown => "Codex request",
        }
    }

    pub fn can_render_decision_buttons(self) -> bool {
        !matches!(self, Self::DynamicToolCall | Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingInteraction {
    pub request_id: Value,
    pub method: String,
    pub kind: InteractionKind,
    pub thread_id: String,
    pub item_id: String,
    pub title: String,
    pub detail: String,
    pub choices: Vec<String>,
    pub params: Value,
}

impl PendingInteraction {
    fn from_event(event: &Value, params: &Value, method: &str) -> Self {
        let kind = InteractionKind::from_method(method);
        let thread_id = event_thread_id(params)
            .map(str::to_owned)
            .unwrap_or_default();
        let item_id = string_field(params, &["itemId", "callId", "approvalId"]);
        let choices = params
            .get("availableDecisions")
            .and_then(Value::as_array)
            .map(|values| values.iter().map(value_text).collect())
            .unwrap_or_default();
        let detail = match kind {
            InteractionKind::UserInput => {
                params.get("questions").map(value_text).unwrap_or_default()
            }
            InteractionKind::McpElicitation => {
                let message = string_field(params, &["message", "serverName"]);
                let mode = string_field(params, &["mode"]);
                [message, mode]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
            }
            _ => string_field(
                params,
                &[
                    "command",
                    "fileChanges",
                    "permissions",
                    "reason",
                    "message",
                    "tool",
                ],
            ),
        };
        Self {
            request_id: event.get("id").cloned().unwrap_or(Value::Null),
            method: method.into(),
            kind,
            thread_id,
            item_id,
            title: kind.title().into(),
            detail,
            choices,
            params: params.clone(),
        }
    }

    pub fn response(&self, approved: bool) -> Value {
        match self.kind {
            InteractionKind::CommandApproval | InteractionKind::FileChangeApproval => {
                let decision = if approved {
                    self.choices
                        .iter()
                        .find(|choice| matches!(choice.as_str(), "acceptForSession" | "accept"))
                        .map(String::as_str)
                        .unwrap_or("accept")
                } else {
                    "decline"
                };
                json!({ "decision": decision })
            }
            InteractionKind::PermissionsApproval => {
                if approved {
                    json!({
                        "permissions": self.params.get("permissions").cloned().unwrap_or_else(|| json!({})),
                        "scope": "turn",
                    })
                } else {
                    json!({ "permissions": {}, "scope": "turn" })
                }
            }
            InteractionKind::UserInput => {
                let mut answers = serde_json::Map::new();
                if approved {
                    if let Some(questions) = self.params.get("questions").and_then(Value::as_array)
                    {
                        for question in questions {
                            let Some(id) = question.get("id").and_then(Value::as_str) else {
                                continue;
                            };
                            let answer = question
                                .get("options")
                                .and_then(Value::as_array)
                                .and_then(|options| options.first())
                                .and_then(|option| {
                                    option
                                        .get("label")
                                        .or_else(|| option.get("value"))
                                        .and_then(Value::as_str)
                                })
                                .unwrap_or("");
                            if !answer.is_empty() {
                                answers.insert(id.into(), json!({ "answers": [answer] }));
                            }
                        }
                    }
                }
                json!({ "answers": answers })
            }
            InteractionKind::McpElicitation => json!({
                "action": if approved { "accept" } else { "cancel" }
            }),
            InteractionKind::LegacyCommandApproval | InteractionKind::LegacyPatchApproval => {
                if approved {
                    json!({ "decision": "approved" })
                } else {
                    json!({ "decision": { "denied": { "rejection": "Declined by user" } } })
                }
            }
            InteractionKind::DynamicToolCall => json!({
                "success": false,
                "contentItems": [],
            }),
            InteractionKind::Unknown => safe_server_request_response(&self.method),
        }
    }
}

fn pending_interaction_key(pending: &PendingInteraction) -> String {
    if pending.item_id.is_empty() {
        format!("request-{}", value_text(&pending.request_id))
    } else {
        pending.item_id.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenu {
    File,
    Edit,
    View,
    Help,
}

pub struct AppState {
    pub workspace: Workspace,
    pub settings: Settings,
    pub selected_project: String,
    pub selected_task: String,
    pub app_menu: Option<AppMenu>,
    pub route: Route,
    pub settings_page: SettingsPage,
    pub query: String,
    pub draft: String,
    pub caret: usize,
    pub selection_anchor: Option<usize>,
    pub streaming: bool,
    pub busy: bool,
    pub composer_mode: String,
    pub attachments: Vec<String>,
    pub menu_open: bool,
    pub view_open: bool,
    pub sidebar_collapsed: bool,
    pub show_archived: bool,
    pub content_layout: String,
    pub bottom_panel_open: bool,
    pub side_panel_open: bool,
    pub fullscreen: bool,
    pub search_open: bool,
    pub toast: Option<String>,
    pub connection: ConnectionState,
    pub live_client: Option<Arc<AppServerClient>>,
    pub catalog: ServerCatalog,
    pub active_app: Option<String>,
    pub skill_roots: Vec<String>,
    pub voice_active: bool,
    pub active_turn_id: Option<String>,
    pub pending_interaction: Option<PendingInteraction>,
    pub pending_interactions: Vec<PendingInteraction>,
    pub github: GitHubState,
    pub worktrees: Vec<WorktreeSummary>,
    pub rename_open: bool,
    pub rename_draft: String,
    pub rename_caret: usize,
    pub rename_selection_anchor: Option<usize>,
    pub settings_editor: Option<&'static str>,
    pub settings_draft: String,
    pub settings_caret: usize,
    pub settings_selection_anchor: Option<usize>,
    pub query_selection_anchor: Option<usize>,
    event_loop_started: bool,
    pub root_focus: FocusHandle,
    pub input_focus: FocusHandle,
    pub search_focus: FocusHandle,
    pub rename_focus: FocusHandle,
    pub settings_focus: FocusHandle,
}

impl AppState {
    pub fn new(snapshot: Snapshot, cx: &mut Context<Self>) -> Self {
        let mut state = Self {
            workspace: snapshot.workspace,
            settings: snapshot.settings,
            skill_roots: snapshot.skill_roots,
            selected_project: snapshot.selected_project,
            selected_task: snapshot.selected_task,
            app_menu: None,
            route: Route::Task,
            settings_page: SettingsPage::General,
            query: String::new(),
            draft: String::new(),
            caret: 0,
            selection_anchor: None,
            streaming: false,
            busy: false,
            composer_mode: "Agent".into(),
            attachments: Vec::new(),
            menu_open: false,
            view_open: false,
            sidebar_collapsed: snapshot.sidebar_collapsed,
            show_archived: snapshot.show_archived,
            content_layout: normalize_content_layout(&snapshot.content_layout).to_owned(),
            bottom_panel_open: snapshot.bottom_panel_open,
            side_panel_open: snapshot.side_panel_open,
            fullscreen: snapshot.fullscreen,
            search_open: false,
            toast: None,
            connection: ConnectionState::Demo,
            live_client: None,
            catalog: ServerCatalog::default(),
            active_app: None,
            voice_active: false,
            active_turn_id: None,
            pending_interaction: None,
            pending_interactions: Vec::new(),
            github: GitHubState {
                status: "Not loaded".into(),
                ..GitHubState::default()
            },
            worktrees: Vec::new(),
            rename_open: false,
            rename_draft: String::new(),
            rename_caret: 0,
            rename_selection_anchor: None,
            settings_editor: None,
            settings_draft: String::new(),
            settings_caret: 0,
            settings_selection_anchor: None,
            query_selection_anchor: None,
            event_loop_started: false,
            root_focus: cx.focus_handle(),
            input_focus: cx.focus_handle(),
            search_focus: cx.focus_handle(),
            rename_focus: cx.focus_handle(),
            settings_focus: cx.focus_handle(),
        };
        state.ensure_selection();
        state
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            workspace: self.workspace.clone(),
            settings: self.settings.clone(),
            skill_roots: self.skill_roots.clone(),
            selected_project: self.selected_project.clone(),
            selected_task: self.selected_task.clone(),
            sidebar_collapsed: self.sidebar_collapsed,
            show_archived: self.show_archived,
            content_layout: self.content_layout.clone(),
            bottom_panel_open: self.bottom_panel_open,
            side_panel_open: self.side_panel_open,
            fullscreen: self.fullscreen,
        }
    }

    pub fn init(&mut self, cx: &mut Context<Self>) {
        let command = std::env::var("CODEX_APP_SERVER_COMMAND")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let Some(command) = command else {
            return;
        };
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned));
        self.connection = ConnectionState::Connecting;
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || connect_live(&command, cwd.as_deref())).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok((client, threads, catalog)) => {
                        this.connection = ConnectionState::Live;
                        this.live_client = Some(client);
                        this.catalog = catalog;
                        this.normalize_catalog_defaults();
                        this.import_live_threads(threads);
                        this.start_event_loop(cx);
                        this.notify_success("Connected to the local Codex app-server", cx);
                    }
                    Err(error) => {
                        this.connection = ConnectionState::Offline;
                        this.fail(&format!("Could not connect to app-server: {error}"), cx);
                    }
                });
            },
        )
        .detach();
    }

    fn start_event_loop(&mut self, cx: &mut Context<Self>) {
        if self.event_loop_started {
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        self.event_loop_started = true;
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                loop {
                    let poll_client = client.clone();
                    let event = async_cx
                        .background_executor()
                        .spawn(async move {
                            smol::unblock(move || {
                                poll_client.next_event(Duration::from_millis(500))
                            })
                            .await
                        })
                        .await;
                    if let Some(event) = event {
                        if this
                            .update(&mut async_cx.clone(), |this, cx| {
                                this.apply_server_event(event, cx)
                            })
                            .is_err()
                        {
                            break;
                        }
                    } else if client.is_closed() {
                        let reason = client
                            .close_reason()
                            .unwrap_or_else(|| "app-server disconnected".into());
                        let _ = this.update(&mut async_cx.clone(), |this, cx| {
                            this.connection = ConnectionState::Offline;
                            this.streaming = false;
                            this.fail(&format!("App-server disconnected: {reason}"), cx);
                        });
                        break;
                    }
                }
            },
        )
        .detach();
    }

    fn load_live_thread(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let request_thread_id = thread_id.clone();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || match client.thread_read(&request_thread_id) {
                            Ok(value) => Ok(value),
                            Err(error) if is_empty_thread_read_error(&error) => {
                                client.thread_read_summary(&request_thread_id)
                            }
                            Err(error) => Err(error),
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        this.hydrate_thread(&thread_id, &value);
                        this.persist(cx);
                        cx.notify();
                    }
                    Err(error) => this.fail(&format!("Could not read live thread: {error}"), cx),
                });
            },
        )
        .detach();
    }

    fn apply_server_event(&mut self, event: Value, cx: &mut Context<Self>) {
        let method = event
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = event.get("params").cloned().unwrap_or(Value::Null);
        let thread_id = event_thread_id(&params);
        let mut persist = false;
        match method {
            "thread/started" => {
                if let Some(thread) = params.get("thread").and_then(ServerThread::from_value) {
                    self.add_server_thread(thread);
                    persist = true;
                }
            }
            "thread/name/updated" => {
                if let (Some(thread_id), Some(name)) =
                    (thread_id, params.get("name").and_then(Value::as_str))
                {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        task.title = name.into();
                        persist = true;
                    }
                }
            }
            "thread/status/changed" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = params
                        .get("status")
                        .map(status_text)
                        .unwrap_or_else(|| task.status.clone());
                    persist = true;
                }
            }
            "thread/archived" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.archived = true;
                    task.status = "archived".into();
                    persist = true;
                }
            }
            "thread/unarchived" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.archived = false;
                    task.status = "idle".into();
                    persist = true;
                }
            }
            "thread/deleted" => {
                if let Some(thread_id) = thread_id {
                    for project in &mut self.workspace.projects {
                        project.tasks.retain(|task| task.id != thread_id);
                    }
                    self.ensure_selection();
                    persist = true;
                }
            }
            "thread/closed" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = "closed".into();
                    persist = true;
                }
            }
            "thread/reverted" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = "idle".into();
                    task.entries.push(Entry::System {
                        id: format!("reverted-{}", task.entries.len()),
                        text: "Thread reverted to the selected checkpoint".into(),
                    });
                    persist = true;
                }
            }
            "thread/environment/connected" | "thread/environment/disconnected" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let connected = method == "thread/environment/connected";
                    task.entries.push(Entry::System {
                        id: format!("environment-{}-{}", task.entries.len(), connected),
                        text: format!(
                            "Execution environment {}{}",
                            if connected {
                                "connected"
                            } else {
                                "disconnected"
                            },
                            params
                                .get("environmentId")
                                .map(|value| format!(": {}", value_text(value)))
                                .unwrap_or_default()
                        ),
                    });
                    persist = true;
                }
            }
            "turn/started" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = "running".into();
                    persist = true;
                }
                if thread_id == Some(self.selected_task.as_str()) {
                    self.active_turn_id = params
                        .get("turn")
                        .and_then(|turn| turn.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    self.streaming = true;
                }
            }
            "turn/completed" => {
                let status = params
                    .get("turn")
                    .and_then(|turn| turn.get("status"))
                    .map(status_text)
                    .unwrap_or_else(|| "completed".into());
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = completed_task_status(&status).into();
                    if let Some(items) = params
                        .get("turn")
                        .and_then(|turn| turn.get("items"))
                        .and_then(Value::as_array)
                    {
                        for item in items {
                            if let Some(entry) = entry_from_server_item(item) {
                                upsert_entry(task, entry);
                            }
                        }
                    }
                }
                if thread_id == Some(self.selected_task.as_str()) {
                    self.streaming = false;
                    self.active_turn_id = None;
                    self.pending_interaction = None;
                    self.pending_interactions.clear();
                    if let Some(task) = self.current_task_mut() {
                        for entry in &mut task.entries {
                            if let Entry::Approval { requested, .. } = entry {
                                *requested = false;
                            }
                        }
                    }
                }
                persist = true;
            }
            "turn/plan/updated" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.plan = params
                        .get("plan")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|step| {
                            Some(PlanStep {
                                label: step.get("step")?.as_str()?.into(),
                                status: normalize_plan_status(
                                    step.get("status").map(status_text).unwrap_or_default(),
                                ),
                            })
                        })
                        .collect();
                    persist = true;
                }
            }
            "item/plan/delta" => {
                if let (Some(task), Some(delta)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("delta").and_then(Value::as_str),
                ) {
                    upsert_entry(
                        task,
                        Entry::Reasoning {
                            id: params
                                .get("itemId")
                                .and_then(Value::as_str)
                                .unwrap_or("plan-delta")
                                .into(),
                            text: delta.into(),
                            collapsed: false,
                        },
                    );
                    persist = true;
                }
            }
            "turn/diff/updated" => {
                if let (Some(task), Some(diff)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("diff"),
                ) {
                    if let Some(entry) = diff_entry_from_value(
                        params
                            .get("turnId")
                            .and_then(Value::as_str)
                            .unwrap_or("turn-diff"),
                        diff,
                    ) {
                        upsert_entry(task, entry);
                        persist = true;
                    }
                }
            }
            "thread/tokenUsage/updated" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    if let Some(usage) = params
                        .get("tokenUsage")
                        .or_else(|| params.get("usage"))
                        .and_then(usage_from_value)
                    {
                        task.usage = usage;
                        persist = true;
                    }
                }
            }
            "item/started" | "item/completed" => {
                if let (Some(task), Some(item)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("item"),
                ) {
                    if let Some(entry) = entry_from_server_item(item) {
                        upsert_entry(task, entry);
                    } else if let Some(entry) = generic_event_entry(
                        method,
                        &params,
                        string_field(item, &["id"]).as_str(),
                        task.entries.len(),
                    ) {
                        upsert_entry(task, entry);
                    }
                    persist = true;
                }
            }
            "item/agentMessage/delta" => {
                if let (Some(thread_id), Some(item_id), Some(delta)) = (
                    thread_id,
                    params.get("itemId").and_then(Value::as_str),
                    params.get("delta").and_then(Value::as_str),
                ) {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        append_assistant_delta(task, item_id, delta);
                        persist = true;
                    }
                }
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                if let (Some(thread_id), Some(item_id), Some(delta)) = (
                    thread_id,
                    params.get("itemId").and_then(Value::as_str),
                    params.get("delta").and_then(Value::as_str),
                ) {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        append_reasoning_delta(task, item_id, delta);
                        persist = true;
                    }
                }
            }
            "item/reasoning/summaryPartAdded" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let item_id = params
                        .get("itemId")
                        .and_then(Value::as_str)
                        .unwrap_or("reasoning-summary");
                    let index = params
                        .get("summaryIndex")
                        .map(value_text)
                        .unwrap_or_default();
                    upsert_entry(
                        task,
                        Entry::Reasoning {
                            id: format!("{item_id}-summary-{index}"),
                            text: format!("Reasoning summary part {index}"),
                            collapsed: false,
                        },
                    );
                    persist = true;
                }
            }
            "item/commandExecution/outputDelta" => {
                if let (Some(thread_id), Some(item_id), Some(delta)) = (
                    thread_id,
                    params.get("itemId").and_then(Value::as_str),
                    params.get("delta").and_then(Value::as_str),
                ) {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        append_tool_output(task, item_id, delta);
                        persist = true;
                    }
                }
            }
            "item/fileChange/outputDelta" => {
                if let (Some(thread_id), Some(item_id), Some(delta)) = (
                    thread_id,
                    params.get("itemId").and_then(Value::as_str),
                    params.get("delta").and_then(Value::as_str),
                ) {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        append_diff_delta(task, item_id, delta);
                        persist = true;
                    }
                }
            }
            "item/fileChange/patchUpdated" => {
                if let (Some(task), Some(changes)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("changes").and_then(Value::as_array),
                ) {
                    let item = json!({
                        "type": "fileChange",
                        "id": params.get("itemId").and_then(Value::as_str).unwrap_or("file-change"),
                        "changes": changes,
                    });
                    if let Some(entry) = entry_from_server_item(&item) {
                        upsert_entry(task, entry);
                        persist = true;
                    }
                }
            }
            "command/exec/outputDelta" | "process/outputDelta" => {
                if let Some(task) = self.task_mut_for_event(thread_id) {
                    let item_id = params
                        .get("processId")
                        .or_else(|| params.get("processHandle"))
                        .map(value_text)
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "process-output".into());
                    if let Some(delta) = params
                        .get("delta")
                        .or_else(|| params.get("deltaBase64"))
                        .and_then(Value::as_str)
                    {
                        append_tool_output(task, &item_id, delta);
                        persist = true;
                    }
                }
            }
            "process/exited" => {
                if let Some(task) = self.task_mut_for_event(thread_id) {
                    let item_id = params
                        .get("processHandle")
                        .or_else(|| params.get("processId"))
                        .map(value_text)
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "process-output".into());
                    let status = if params.get("exitCode").and_then(Value::as_i64) == Some(0) {
                        "complete"
                    } else {
                        "failed"
                    };
                    update_tool_status(task, &item_id, status);
                    persist = true;
                }
            }
            "item/commandExecution/terminalInteraction" => {
                if let (Some(task), Some(stdin)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("stdin").and_then(Value::as_str),
                ) {
                    append_tool_output(
                        task,
                        params
                            .get("itemId")
                            .and_then(Value::as_str)
                            .unwrap_or("terminal-interaction"),
                        stdin,
                    );
                    persist = true;
                }
            }
            "item/mcpToolCall/progress" => {
                if let (Some(task), Some(message)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("message").and_then(Value::as_str),
                ) {
                    append_tool_output(
                        task,
                        params
                            .get("itemId")
                            .and_then(Value::as_str)
                            .unwrap_or("mcp-progress"),
                        message,
                    );
                    persist = true;
                }
            }
            "thread/realtime/transcript/delta" => {
                if let (Some(thread_id), Some(delta)) =
                    (thread_id, params.get("delta").and_then(Value::as_str))
                {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        if params.get("role").and_then(Value::as_str) == Some("user") {
                            append_user_delta(task, delta);
                        } else {
                            append_assistant_delta(task, "realtime-transcript", delta);
                        }
                        persist = true;
                    }
                }
            }
            "thread/realtime/transcript/done" => {
                if let (Some(thread_id), Some(text)) =
                    (thread_id, params.get("text").and_then(Value::as_str))
                {
                    if let Some(task) = self.task_mut_by_id(thread_id) {
                        if params.get("role").and_then(Value::as_str) == Some("user") {
                            upsert_entry(
                                task,
                                Entry::User {
                                    id: "realtime-user".into(),
                                    text: text.into(),
                                    time: "Live".into(),
                                },
                            );
                        } else {
                            upsert_entry(
                                task,
                                Entry::Assistant {
                                    id: "realtime-transcript".into(),
                                    text: text.into(),
                                    time: "Live".into(),
                                },
                            );
                        }
                        persist = true;
                    }
                }
            }
            "thread/realtime/error" => {
                self.voice_active = false;
                if let Some(message) = params.get("message").and_then(Value::as_str) {
                    self.fail(message, cx);
                }
            }
            "thread/realtime/closed" => {
                self.voice_active = false;
            }
            "thread/realtime/started" => {
                self.voice_active = true;
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    append_system_event(task, method, &params, "Realtime session started");
                    persist = true;
                }
            }
            "thread/realtime/itemAdded" => {
                if let (Some(task), Some(item)) = (
                    thread_id.and_then(|id| self.task_mut_by_id(id)),
                    params.get("item"),
                ) {
                    if let Some(entry) = entry_from_server_item(item) {
                        upsert_entry(task, entry);
                    } else {
                        append_system_event(task, method, &params, "Realtime item received");
                    }
                    persist = true;
                }
            }
            "thread/realtime/outputAudio/delta" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    append_tool_output(
                        task,
                        params
                            .get("itemId")
                            .and_then(Value::as_str)
                            .unwrap_or("realtime-audio"),
                        "[audio response received]",
                    );
                    persist = true;
                }
            }
            "thread/realtime/sdp" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    append_system_event(task, method, &params, "Realtime transport negotiated");
                    persist = true;
                }
            }
            "thread/compacted" | "thread/goal/cleared" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let text = if method == "thread/compacted" {
                        "Conversation context compacted"
                    } else {
                        "Thread goal cleared"
                    };
                    if method == "thread/goal/cleared" {
                        task.goal = None;
                    }
                    append_system_event(task, method, &params, text);
                    persist = true;
                }
            }
            "thread/goal/updated" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let goal = params.get("goal").unwrap_or(&params);
                    let objective = string_field(goal, &["objective"]);
                    let status = string_field(goal, &["status"]);
                    let token_budget = goal
                        .get("tokenBudget")
                        .or_else(|| goal.get("token_budget"))
                        .and_then(Value::as_i64);
                    task.goal = Some(Goal {
                        objective: objective.clone(),
                        status: status.clone(),
                        token_budget,
                    });
                    let detail = [objective, status]
                        .into_iter()
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    append_system_event(
                        task,
                        method,
                        &params,
                        if detail.is_empty() {
                            "Thread goal updated"
                        } else {
                            &detail
                        },
                    );
                    persist = true;
                }
            }
            "thread/settings/updated" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let settings = params.get("threadSettings").unwrap_or(&params);
                    let model = string_field(settings, &["model"]);
                    if !model.is_empty() {
                        task.model = model;
                    }
                    let reasoning = string_field(settings, &["effort", "reasoningEffort"]);
                    if !reasoning.is_empty() {
                        task.reasoning = reasoning;
                    }
                    let cwd = string_field(settings, &["cwd"]);
                    if !cwd.is_empty() {
                        task.path = cwd;
                    }
                    append_system_event(task, method, &params, "Thread settings updated");
                    persist = true;
                }
            }
            "thread/queue/changed" => {
                if let Some(thread_id) = thread_id {
                    self.refresh_live_queue(thread_id.to_owned(), cx);
                }
            }
            "app/list/updated" => {
                self.catalog.apps = named_values_from_data(&params, &["name", "displayName", "id"]);
                self.catalog.installed_apps = self.catalog.apps.clone();
            }
            "mcpServer/startupStatus/updated" => {
                if let Some(name) = params.get("name").and_then(Value::as_str) {
                    if !self.catalog.mcp_servers.iter().any(|server| server == name) {
                        self.catalog.mcp_servers.push(name.into());
                    }
                }
            }
            "account/updated" => {
                let auth = string_field(&params, &["authMode"]);
                let plan = string_field(&params, &["planType"]);
                let label = [auth, plan]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>();
                if !label.is_empty() {
                    self.catalog.account_label = Some(label.join(" · "));
                }
            }
            "model/rerouted" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    let from = string_field(&params, &["fromModel"]);
                    let to = string_field(&params, &["toModel"]);
                    task.entries.push(Entry::System {
                        id: format!("model-reroute-{}", task.entries.len()),
                        text: format!("Model rerouted: {from} → {to}"),
                    });
                    persist = true;
                }
            }
            "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "mcpServer/elicitation/request"
            | "execCommandApproval"
            | "applyPatchApproval"
            | "item/tool/call" => {
                self.add_approval_request(&event, &params, method);
                persist = true;
            }
            "serverRequest/resolved" => {
                if let Some(request_id) = params.get("requestId") {
                    let removed = self
                        .pending_interactions
                        .iter()
                        .position(|pending| &pending.request_id == request_id)
                        .map(|index| self.pending_interactions.remove(index));
                    if let Some(removed) = removed {
                        self.promote_pending_interaction();
                        let task_id = if removed.thread_id.is_empty() {
                            self.selected_task.clone()
                        } else {
                            removed.thread_id.clone()
                        };
                        let interaction_id = pending_interaction_key(&removed);
                        if let Some(task) = self.task_mut_by_id(&task_id) {
                            for entry in &mut task.entries {
                                if let Entry::Approval { id, requested, .. } = entry {
                                    if id == &interaction_id {
                                        *requested = false;
                                    }
                                }
                            }
                        }
                        persist = true;
                    }
                }
            }
            "error" | "warning" => {
                if let Some(message) = params.get("message").and_then(Value::as_str) {
                    self.fail(message, cx);
                }
            }
            _ => {
                if let Some(request_id) = event.get("id").cloned() {
                    if method == "currentTime/read" {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_secs())
                            .unwrap_or_default();
                        if let Some(client) = self.live_client.clone() {
                            let _ = client.respond(request_id, json!({ "currentTimeAt": now }));
                        }
                    } else if let Some(client) = self.live_client.clone() {
                        let _ = client.respond(request_id, safe_server_request_response(method));
                    }
                } else if let Some(task) = self.task_mut_for_event(thread_id) {
                    if let Some(entry) =
                        generic_event_entry(method, &params, "", task.entries.len())
                    {
                        upsert_entry(task, entry);
                        persist = true;
                    }
                }
            }
        }
        if matches!(
            method,
            "skills/changed" | "project/changed" | "thread/project/updated"
        ) {
            self.refresh_catalog(cx);
        }
        if persist {
            self.persist(cx);
        }
        cx.notify();
    }

    fn add_approval_request(&mut self, event: &Value, params: &Value, method: &str) {
        let pending = PendingInteraction::from_event(event, params, method);
        let thread_id = (!pending.thread_id.is_empty())
            .then(|| pending.thread_id.clone())
            .unwrap_or_else(|| self.selected_task.clone());
        if self.pending_interaction.is_none() {
            self.pending_interaction = Some(pending.clone());
        }
        self.pending_interactions.push(pending.clone());
        let thread_id = thread_id;
        let entry = Entry::Approval {
            id: if pending.item_id.is_empty() {
                format!("request-{}", value_text(&pending.request_id))
            } else {
                pending.item_id.clone()
            },
            title: pending.title.clone(),
            command: string_field(
                params,
                &[
                    "command",
                    "questions",
                    "fileChanges",
                    "permissions",
                    "message",
                    "toolName",
                ],
            ),
            cwd: string_field(params, &["cwd", "environmentId"]),
            reason: string_field(params, &["reason", "message"]),
            requested: true,
            approval_kind: format!("{method}"),
            choices: pending.choices.clone(),
            request_details: pending.detail.clone(),
        };
        if let Some(task) = self.task_mut_by_id(&thread_id) {
            upsert_entry(task, entry);
            task.status = "running".into();
        }
        if thread_id == self.selected_task {
            self.streaming = true;
        }
    }

    fn hydrate_thread(&mut self, thread_id: &str, value: &Value) {
        let Some(task) = self.task_mut_by_id(thread_id) else {
            return;
        };
        let thread = value.get("thread").unwrap_or(value);
        task.status = thread
            .get("status")
            .map(status_text)
            .unwrap_or_else(|| task.status.clone());
        task.entries.clear();
        if let Some(turns) = thread.get("turns").and_then(Value::as_array) {
            for turn in turns {
                if let Some(items) = turn.get("items").and_then(Value::as_array) {
                    for item in items {
                        if let Some(entry) = entry_from_server_item(item) {
                            upsert_entry(task, entry);
                        }
                    }
                }
                if let Some(usage) = turn.get("usage").and_then(usage_from_value) {
                    task.usage = usage;
                }
            }
        }
    }

    fn task_mut_by_id(&mut self, task_id: &str) -> Option<&mut Task> {
        self.workspace
            .projects
            .iter_mut()
            .find_map(|project| project.tasks.iter_mut().find(|task| task.id == task_id))
    }

    fn task_mut_for_event(&mut self, thread_id: Option<&str>) -> Option<&mut Task> {
        match thread_id {
            Some(thread_id) => self.task_mut_by_id(thread_id),
            None => self.current_task_mut(),
        }
    }

    fn refresh_live_queue(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let request_thread_id = thread_id.clone();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            client.thread_queue_list(&request_thread_id, None, None)
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        if let Some(task) = this.task_mut_by_id(&thread_id) {
                            task.queue = queued_inputs_from_value(&value);
                            this.persist(cx);
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        this.fail(&format!("Could not refresh queued follow-ups: {error}"), cx)
                    }
                });
            },
        )
        .detach();
    }

    fn promote_pending_interaction(&mut self) {
        self.pending_interaction = self.pending_interactions.first().cloned();
    }

    pub fn current_task(&self) -> Option<&Task> {
        self.workspace
            .task(&self.selected_project, &self.selected_task)
    }

    pub fn current_task_mut(&mut self) -> Option<&mut Task> {
        self.workspace
            .task_mut(&self.selected_project, &self.selected_task)
    }

    pub fn current_project(&self) -> Option<&Project> {
        self.workspace
            .projects
            .iter()
            .find(|project| project.id == self.selected_project)
    }

    pub fn model_label(&self, model: &str) -> String {
        self.catalog.model_label(model)
    }

    pub fn model_options(&self) -> Vec<String> {
        self.catalog.model_ids()
    }

    pub fn reasoning_options(&self) -> Vec<String> {
        self.catalog.reasoning_options()
    }

    fn normalize_catalog_defaults(&mut self) {
        if let Some(model) = self.catalog.model_ids().first() {
            if !self
                .catalog
                .models
                .iter()
                .any(|option| option.id == self.settings.default_model)
            {
                self.settings.default_model = model.clone();
            }
        }
        if let Some(reasoning) = self.catalog.reasoning_options().first() {
            if !self
                .catalog
                .reasoning_options()
                .iter()
                .any(|option| option == &self.settings.default_reasoning)
            {
                self.settings.default_reasoning = reasoning.clone();
            }
        }
    }

    pub fn visible_tasks<'a>(&'a self, project: &'a Project) -> impl Iterator<Item = &'a Task> {
        let query = self.query.trim().to_lowercase();
        project.tasks.iter().filter(move |task| {
            (self.show_archived || !task.archived)
                && (query.is_empty()
                    || task.title.to_lowercase().contains(&query)
                    || project.name.to_lowercase().contains(&query))
        })
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        cx.notify();
    }

    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.caret = self.query.chars().count();
            self.query_selection_anchor = None;
            window.focus(&self.search_focus);
        } else {
            self.query.clear();
            self.caret = self.draft.chars().count();
            self.query_selection_anchor = None;
            self.selection_anchor = None;
        }
        cx.notify();
    }

    pub fn select_task(&mut self, project_id: String, task_id: String, cx: &mut Context<Self>) {
        self.selected_project = project_id;
        self.selected_task = task_id;
        self.route = Route::Task;
        self.draft.clear();
        self.caret = 0;
        self.selection_anchor = None;
        self.query_selection_anchor = None;
        self.attachments.clear();
        self.menu_open = false;
        self.app_menu = None;
        self.view_open = false;
        self.rename_open = false;
        self.rename_draft.clear();
        self.rename_selection_anchor = None;
        self.streaming = self
            .current_task()
            .map(|task| task.status == "running")
            .unwrap_or(false);
        self.persist(cx);
        if self.connection == ConnectionState::Live && self.selected_project == "live-codex" {
            self.load_live_thread(self.selected_task.clone(), cx);
        }
        cx.notify();
    }

    pub fn create_task(&mut self, cx: &mut Context<Self>) {
        let project_id = self.selected_project.clone();
        let Some(project) = self.current_project().cloned() else {
            return;
        };
        let id = format!("local-task-{}", self.workspace.all_tasks().count() + 1);
        let task = Task {
            id: id.clone(),
            title: "New task".into(),
            project_id: project_id.clone(),
            status: "idle".into(),
            path: project.path,
            branch: None,
            model: self.settings.default_model.clone(),
            reasoning: self.settings.default_reasoning.clone(),
            updated_at: "Now".into(),
            archived: false,
            pinned: false,
            entries: Vec::new(),
            plan: Vec::new(),
            usage: Default::default(),
            goal: None,
            children: Vec::new(),
            queue: Vec::new(),
        };
        if let Some(project) = self
            .workspace
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.tasks.insert(0, task);
        }
        self.select_task(project_id, id, cx);
        self.notify_success("New task", cx);
    }

    pub fn create_automation(&mut self, cx: &mut Context<Self>) {
        let task = self.current_task().cloned();
        let id = format!("automation-{}", self.workspace.automations.len() + 1);
        self.workspace.automations.push(Automation {
            id,
            name: task
                .as_ref()
                .map(|task| format!("Run {}", task.title))
                .unwrap_or_else(|| "New Codex automation".into()),
            prompt: task
                .as_ref()
                .map(|task| format!("Continue task: {}", task.title))
                .unwrap_or_default(),
            schedule: "Every day at 09:00".into(),
            status: "active".into(),
            next_run: "Next day".into(),
            project_id: self.selected_project.clone(),
            task_id: task.map(|task| task.id),
        });
        self.persist(cx);
        self.notify_success("Automation created", cx);
    }

    pub fn toggle_automation(&mut self, automation_id: String, cx: &mut Context<Self>) {
        let Some(automation) = self
            .workspace
            .automations
            .iter_mut()
            .find(|automation| automation.id == automation_id)
        else {
            return;
        };
        automation.status = if automation.status == "active" {
            "paused".into()
        } else {
            "active".into()
        };
        let status = automation.status.clone();
        self.persist(cx);
        self.notify_success(&format!("Automation {status}"), cx);
    }

    pub fn delete_automation(&mut self, automation_id: String, cx: &mut Context<Self>) {
        let before = self.workspace.automations.len();
        self.workspace
            .automations
            .retain(|automation| automation.id != automation_id);
        if self.workspace.automations.len() == before {
            return;
        }
        self.persist(cx);
        self.notify_success("Automation deleted", cx);
    }

    pub fn run_automation(&mut self, automation_id: String, cx: &mut Context<Self>) {
        let task_id = self
            .workspace
            .automations
            .iter()
            .find(|automation| automation.id == automation_id)
            .and_then(|automation| automation.task_id.clone());
        if let Some(task_id) = task_id {
            let project_id = self
                .workspace
                .task_by_id(&task_id)
                .map(|(project, _)| project.id.clone());
            if let Some(project_id) = project_id {
                self.select_task(project_id, task_id, cx);
            }
        }
        self.notify_success("Automation queued locally", cx);
    }

    pub fn create_live_task(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.create_task(cx);
            return;
        };
        let cwd = self.current_project().map(|project| project.path.clone());
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.thread_start(cwd.as_deref())).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        if let Some(thread) = value.get("thread").and_then(ServerThread::from_value)
                        {
                            this.add_server_thread(thread.clone());
                            this.select_task("live-codex".into(), thread.id, cx);
                        } else {
                            this.create_task(cx);
                        }
                    }
                    Err(error) => this.fail(&format!("New task failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn set_route(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        self.menu_open = false;
        self.app_menu = None;
        self.view_open = false;
        self.rename_open = false;
        if route == Route::PullRequests {
            self.refresh_pull_requests(cx);
        }
        cx.notify();
    }

    pub fn refresh_pull_requests(&mut self, cx: &mut Context<Self>) {
        let path = self
            .current_project()
            .map(|project| project.path.clone())
            .filter(|path| Path::new(path).is_dir());
        let Some(path) = path else {
            self.github.status = "Current project is not a local Git checkout".into();
            self.github.pull_requests.clear();
            cx.notify();
            return;
        };
        self.github.status = "Loading pull requests…".into();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move { smol::unblock(move || github_pull_requests(&path)).await })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| {
                    match result {
                        Ok((repository, pull_requests)) => {
                            this.github.repository = repository;
                            this.github.pull_requests = pull_requests;
                            this.github.status = "GitHub CLI connected".into();
                        }
                        Err(error) => {
                            this.github.status = format!("GitHub unavailable: {error}");
                            this.github.pull_requests.clear();
                        }
                    }
                    cx.notify();
                });
            },
        )
        .detach();
    }

    pub fn copy_link(&mut self, link: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
        self.notify_success("Link copied", cx);
    }

    pub fn open_settings(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.settings_page = page;
        self.route = Route::Settings;
        self.menu_open = false;
        self.app_menu = None;
        self.view_open = false;
        self.refresh_catalog(cx);
        if page == SettingsPage::Worktrees {
            self.refresh_worktrees(cx);
        }
        cx.notify();
    }

    pub fn select_settings_page(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.settings_page = page;
        self.refresh_catalog(cx);
        if page == SettingsPage::Worktrees {
            self.refresh_worktrees(cx);
        }
        cx.notify();
    }

    pub fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let cwd = self.current_task().map(|task| task.path.clone());
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let catalog = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || ServerCatalog::from_client(&client, cwd.as_deref()))
                            .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| {
                    this.catalog = catalog;
                    this.normalize_catalog_defaults();
                    cx.notify();
                });
            },
        )
        .detach();
    }

    pub fn open_app(&mut self, app_id: String, cx: &mut Context<Self>) {
        let thread_id = self.current_task().map(|task| task.id.clone());
        let Some(client) = self.live_client.clone() else {
            self.active_app = Some(format!("{app_id} · local app surface"));
            self.notify_success("App surface opened locally", cx);
            return;
        };
        let request_ids = vec![app_id.clone()];
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.apps_read(&request_ids, thread_id.as_deref()))
                            .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        let detail = serde_json::to_string(&value)
                            .unwrap_or_else(|_| "app metadata loaded".into());
                        this.active_app = Some(format!(
                            "{} · {}",
                            app_id,
                            detail.chars().take(180).collect::<String>()
                        ));
                        this.notify_success("App surface opened", cx);
                    }
                    Err(error) => this.fail(&format!("App surface failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn refresh_account(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.catalog.account_label = Some("Local account".into());
            self.notify_success("Account refreshed locally", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move { smol::unblock(move || client.account_read()).await })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        this.catalog.account_label = account_label_from_value(&value);
                        this.notify_success("Account refreshed", cx);
                    }
                    Err(error) => this.fail(&format!("Account refresh failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn start_account_login(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.fail("Connect to the Codex app-server before signing in", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            client.account_login_start(json!({ "type": "chatgptDeviceCode" }))
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        let login_id = string_field(&value, &["loginId", "id"]);
                        this.catalog.account_label = Some(if login_id.is_empty() {
                            "Login started".into()
                        } else {
                            format!("Login started · {login_id}")
                        });
                        this.notify_success("Follow the app-server login instructions", cx);
                    }
                    Err(error) => this.fail(&format!("Login failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn logout_account(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.catalog.account_label = None;
            self.notify_success("Signed out of the local account", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move { smol::unblock(move || client.account_logout()).await })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.catalog.account_label = None;
                        this.notify_success("Signed out", cx);
                    }
                    Err(error) => this.fail(&format!("Sign out failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn refresh_worktrees(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.current_project().map(|project| project.path.clone()) else {
            self.worktrees.clear();
            self.notify_success("No project selected for worktree discovery", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move { smol::unblock(move || git_worktrees(&path)).await })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(worktrees) => {
                        let count = worktrees.len();
                        this.worktrees = worktrees;
                        this.notify_success(&format!("Worktrees refreshed · {count}"), cx);
                    }
                    Err(error) => {
                        this.worktrees.clear();
                        this.fail(&format!("Worktree discovery failed: {error}"), cx);
                    }
                });
            },
        )
        .detach();
    }

    pub fn delete_worktree(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(repository) = self.current_project().map(|project| project.path.clone()) else {
            return;
        };
        let Some(worktree) = self.worktrees.iter().find(|worktree| worktree.path == path) else {
            self.fail("Worktree is not in the discovered repository list", cx);
            return;
        };
        if worktree.is_main {
            self.fail("The main worktree cannot be deleted", cx);
            return;
        }
        if self
            .current_task()
            .map(|task| task.path == path)
            .unwrap_or(false)
        {
            self.fail(
                "Select another task before deleting its active worktree",
                cx,
            );
            return;
        }
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let target = path.clone();
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || remove_git_worktree(&repository, &target)).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(()) => {
                        this.worktrees.retain(|worktree| worktree.path != path);
                        this.notify_success("Worktree deleted", cx);
                    }
                    Err(error) => this.fail(&format!("Worktree delete failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn new_chat_in_worktree(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(client) = self.live_client.clone() {
            let async_cx = cx.to_async();
            cx.spawn(
                move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                    let request_path = path.clone();
                    let result = async_cx
                        .background_executor()
                        .spawn(async move {
                            smol::unblock(move || client.thread_start(Some(&request_path))).await
                        })
                        .await;
                    let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                        Ok(value) => {
                            if let Some(thread) =
                                value.get("thread").and_then(ServerThread::from_value)
                            {
                                let id = thread.id.clone();
                                this.add_server_thread(thread);
                                this.select_task("live-codex".into(), id, cx);
                                this.notify_success("New chat in this worktree", cx);
                            } else {
                                this.fail("The app-server did not return a new thread", cx);
                            }
                        }
                        Err(error) => this.fail(&format!("New worktree chat failed: {error}"), cx),
                    });
                },
            )
            .detach();
            return;
        }

        let project_id = self
            .workspace
            .projects
            .iter()
            .find(|project| project.path == path)
            .map(|project| project.id.clone());
        let Some(project_id) = project_id else {
            self.add_local_project(PathBuf::from(path), cx);
            return;
        };
        let id = format!("worktree-task-{}", self.workspace.all_tasks().count() + 1);
        let task = Task {
            id: id.clone(),
            title: "New task".into(),
            project_id: project_id.clone(),
            status: "idle".into(),
            path,
            branch: None,
            model: self.settings.default_model.clone(),
            reasoning: self.settings.default_reasoning.clone(),
            updated_at: "Now".into(),
            archived: false,
            pinned: false,
            entries: Vec::new(),
            plan: Vec::new(),
            usage: Default::default(),
            goal: None,
            children: Vec::new(),
            queue: Vec::new(),
        };
        if let Some(project) = self
            .workspace
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.tasks.insert(0, task);
        }
        self.select_task(project_id, id, cx);
        self.notify_success("New chat in this worktree", cx);
    }

    pub fn toggle_project(&mut self, project_id: String, cx: &mut Context<Self>) {
        if let Some(project) = self
            .workspace
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.collapsed = !project.collapsed;
        }
        self.persist(cx);
        cx.notify();
    }

    pub fn send(&mut self, cx: &mut Context<Self>) {
        let text = self.draft.trim().to_string();
        let attachments = self.attachments.clone();
        if text.is_empty() && attachments.is_empty() {
            return;
        }
        if self.streaming || self.busy {
            if self.settings.queue_follow_ups {
                self.queue_current_input(cx);
            }
            return;
        }
        let project_id = self.selected_project.clone();
        let task_id = self.selected_task.clone();
        let Some(task) = self.workspace.task_mut(&project_id, &task_id) else {
            return;
        };
        if task.title == "New task" && !text.is_empty() {
            task.title = Task::title_from_prompt(&text);
        }
        if !text.is_empty() {
            task.entries.push(Entry::User {
                id: format!("user-{}", task.entries.len() + 1),
                text: text.clone(),
                time: "Now".into(),
            });
        }
        for (index, attachment) in attachments.iter().enumerate() {
            task.entries.push(Entry::Attachment {
                id: format!("attachment-{}-{}", task.entries.len() + 1, index),
                name: attachment_name(attachment),
                attachment_kind: attachment_kind(attachment).into(),
            });
        }
        task.status = "running".into();
        task.updated_at = "Now".into();
        self.draft.clear();
        self.caret = 0;
        self.selection_anchor = None;
        self.busy = false;
        self.streaming = true;
        self.attachments.clear();
        self.persist(cx);

        if self.selected_project == "live-codex" {
            if let Some(client) = self.live_client.clone() {
                let model = self
                    .current_task()
                    .map(|task| task.model.clone())
                    .unwrap_or_else(|| self.settings.default_model.clone());
                let effort = self
                    .current_task()
                    .map(|task| task.reasoning.clone())
                    .unwrap_or_else(|| self.settings.default_reasoning.clone());
                let cwd = self.current_task().map(|task| task.path.clone());
                let approval_policy = self.settings.approval_mode.clone();
                let sandbox_policy = sandbox_policy_wire(&self.settings.sandbox_mode).to_owned();
                let refresh_thread_id = task_id.clone();
                let async_cx = cx.to_async();
                cx.spawn(
                    move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                        let result = async_cx
                            .background_executor()
                            .spawn(async move {
                                smol::unblock(move || {
                                    client.turn_start_with_full_options_and_attachments(
                                        &task_id,
                                        &text,
                                        Some(&model),
                                        Some(&effort),
                                        cwd.as_deref(),
                                        Some(&approval_policy),
                                        Some(&sandbox_policy),
                                        None,
                                        None,
                                        &attachments,
                                    )
                                })
                                .await
                            })
                            .await;
                        let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                            Ok(value) => {
                                this.refresh_live_queue(refresh_thread_id, cx);
                                this.active_turn_id = value
                                    .get("turn")
                                    .and_then(|turn| turn.get("id"))
                                    .and_then(Value::as_str)
                                    .map(str::to_owned);
                                this.notify_success("Turn started", cx);
                            }
                            Err(error) => {
                                this.streaming = false;
                                if let Some(task) = this.current_task_mut() {
                                    task.status = "idle".into();
                                }
                                this.fail(&format!("Turn failed: {error}"), cx);
                            }
                        });
                    },
                )
                .detach();
            }
        } else {
            let async_cx = cx.to_async();
            cx.spawn(move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                smol::Timer::after(Duration::from_millis(900)).await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| {
                    if this.selected_task == task_id {
                        if let Some(task) = this.workspace.task_mut(&project_id, &task_id) {
                            task.entries.push(Entry::Assistant {
                                id: format!("assistant-{}", task.entries.len() + 1),
                                text: "I’m ready to continue this task. Connect `CODEX_APP_SERVER_COMMAND` to stream a live Codex turn here.".into(),
                                time: "Now".into(),
                            });
                            task.status = "idle".into();
                            task.usage.input += 18;
                            task.usage.output += 24;
                        }
                        this.streaming = false;
                        this.persist(cx);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub fn queue_current_input(&mut self, cx: &mut Context<Self>) {
        let text = self.draft.trim().to_owned();
        let attachments = self.attachments.clone();
        if text.is_empty() && attachments.is_empty() {
            return;
        }
        let id = format!(
            "queued-{}",
            self.current_task()
                .map(|task| task.queue.len() + 1)
                .unwrap_or(1)
        );
        let display = if attachments.is_empty() {
            text.clone()
        } else if text.is_empty() {
            attachments.join(", ")
        } else {
            format!("{text} · {} attachment(s)", attachments.len())
        };
        if let Some(task) = self.current_task_mut() {
            task.queue.push(QueuedInput {
                id: id.clone(),
                text: display,
            });
        }
        self.draft.clear();
        self.caret = 0;
        self.selection_anchor = None;
        self.attachments.clear();
        self.persist(cx);
        if self.connection == ConnectionState::Live && self.selected_project == "live-codex" {
            if let Some(client) = self.live_client.clone() {
                let thread_id = self.selected_task.clone();
                let refresh_thread_id = thread_id.clone();
                let mut input = Vec::new();
                if !text.is_empty() || attachments.is_empty() {
                    input.push(json!({ "type": "text", "text": text }));
                }
                input.extend(attachments.iter().filter_map(|path| {
                    let path = Path::new(path);
                    let path_text = path.to_str()?;
                    if is_image_path(path_text) {
                        Some(json!({ "type": "localImage", "path": path_text }))
                    } else {
                        Some(json!({
                            "type": "mention",
                            "name": path.file_name().and_then(|name| name.to_str()).unwrap_or(path_text),
                            "path": path_text,
                        }))
                    }
                }));
                let async_cx = cx.to_async();
                cx.spawn(
                    move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                        let result = async_cx
                            .background_executor()
                            .spawn(async move {
                                smol::unblock(move || {
                                    client.thread_queue_add(&thread_id, &id, json!(input))
                                })
                                .await
                            })
                            .await;
                        let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                            Ok(_) => this.refresh_live_queue(refresh_thread_id, cx),
                            Err(error) => {
                                this.fail(&format!("Could not queue follow-up: {error}"), cx)
                            }
                        });
                    },
                )
                .detach();
            }
        }
        self.notify_success("Follow-up queued", cx);
    }

    pub fn remove_queued_input(&mut self, queued_id: String, cx: &mut Context<Self>) {
        let thread_id = self.selected_task.clone();
        let live =
            self.connection == ConnectionState::Live && self.selected_project == "live-codex";
        let Some(task) = self.current_task_mut() else {
            return;
        };
        let before = task.queue.len();
        task.queue.retain(|item| item.id != queued_id);
        if before == task.queue.len() {
            return;
        }
        self.persist(cx);
        if live {
            if let Some(client) = self.live_client.clone() {
                let request_thread_id = thread_id.clone();
                let request_queued_id = queued_id.clone();
                let async_cx = cx.to_async();
                cx.spawn(
                    move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                        let result = smol::unblock(move || {
                            client.thread_queue_delete(&request_thread_id, &request_queued_id)
                        })
                        .await;
                        if let Err(error) = result {
                            let _ = this.update(&mut async_cx.clone(), |this, cx| {
                                this.fail(
                                    &format!("Could not remove queued follow-up: {error}"),
                                    cx,
                                )
                            });
                        }
                    },
                )
                .detach();
            }
        }
        self.notify_success("Queued follow-up removed", cx);
    }

    pub fn stop_turn(&mut self, cx: &mut Context<Self>) {
        if !self.streaming {
            return;
        }
        let task_id = self.selected_task.clone();
        let turn_id = self
            .active_turn_id
            .take()
            .unwrap_or_else(|| "current".into());
        self.streaming = false;
        if let Some(task) = self.current_task_mut() {
            task.status = "idle".into();
            task.entries.push(Entry::System {
                id: format!("stop-{}", task.entries.len()),
                text: "Turn interrupted".into(),
            });
        }
        if let Some(client) = self.live_client.clone() {
            cx.spawn(
                move |_this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                    let _ = smol::unblock(move || client.turn_interrupt(&task_id, &turn_id)).await;
                },
            )
            .detach();
        }
        self.notify_success("Turn stopped", cx);
    }

    pub fn continue_turn(&mut self, cx: &mut Context<Self>) {
        self.retry_current(cx);
    }

    pub fn cycle_model(&mut self, cx: &mut Context<Self>) {
        let options = self.catalog.model_ids();
        if options.is_empty() {
            return;
        }
        let current = self
            .current_task()
            .map(|task| task.model.clone())
            .unwrap_or_default();
        let pos = options.iter().position(|model| model == &current);
        let next = options[pos.map(|pos| (pos + 1) % options.len()).unwrap_or(0)].clone();
        if let Some(task) = self.current_task_mut() {
            task.model = next.clone();
        }
        self.settings.default_model = next.clone();
        self.persist(cx);
        self.sync_live_thread_settings(cx);
        self.notify_success(&format!("Model: {next}"), cx);
    }

    pub fn cycle_reasoning(&mut self, cx: &mut Context<Self>) {
        let options = self.catalog.reasoning_options();
        if options.is_empty() {
            return;
        }
        let current = self
            .current_task()
            .map(|task| task.reasoning.clone())
            .unwrap_or_else(|| "auto".into());
        let pos = options.iter().position(|level| level == &current);
        let next = options[pos.map(|pos| (pos + 1) % options.len()).unwrap_or(0)].clone();
        if let Some(task) = self.current_task_mut() {
            task.reasoning = next.clone();
        }
        self.settings.default_reasoning = next.clone();
        self.persist(cx);
        self.sync_live_thread_settings(cx);
        self.notify_success(&format!("Reasoning: {next}"), cx);
    }

    pub fn cycle_approval_mode(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["on-request", "never", "reject"];
        let current = self.settings.approval_mode.as_str();
        let position = OPTIONS
            .iter()
            .position(|option| *option == current)
            .unwrap_or(0);
        self.settings.approval_mode = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.approval_mode.clone();
        self.persist(cx);
        self.sync_live_thread_settings(cx);
        self.notify_success(&format!("Approval mode: {value}"), cx);
    }

    pub fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["system", "dark", "light"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.theme)
            .unwrap_or(0);
        self.settings.theme = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.theme.clone();
        self.persist(cx);
        self.notify_success(&format!("Theme: {value}"), cx);
    }

    pub fn cycle_font_size(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[u8] = &[13, 14, 15, 16, 18];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.font_size)
            .unwrap_or(1);
        self.settings.font_size = OPTIONS[(position + 1) % OPTIONS.len()];
        let value = self.settings.font_size;
        self.persist(cx);
        self.notify_success(&format!("Font size: {value} px"), cx);
    }

    pub fn cycle_code_font_size(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[u8] = &[12, 13, 14, 15, 16, 18];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.code_font_size)
            .unwrap_or(1);
        self.settings.code_font_size = OPTIONS[(position + 1) % OPTIONS.len()];
        let value = self.settings.code_font_size;
        self.persist(cx);
        self.notify_success(&format!("Code font size: {value} px"), cx);
    }

    pub fn toggle_reduced_motion(&mut self, cx: &mut Context<Self>) {
        self.settings.reduced_motion = !self.settings.reduced_motion;
        let value = if self.settings.reduced_motion {
            "on"
        } else {
            "off"
        };
        self.persist(cx);
        self.notify_success(&format!("Reduced motion: {value}"), cx);
    }

    pub fn cycle_enter_behavior(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["send", "newline"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.enter_behavior)
            .unwrap_or(0);
        self.settings.enter_behavior = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.enter_behavior.clone();
        self.persist(cx);
        self.notify_success(&format!("Enter behavior: {value}"), cx);
    }

    pub fn cycle_language(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["system", "English", "日本語", "简体中文"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.language)
            .unwrap_or(0);
        self.settings.language = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.language.clone();
        self.persist(cx);
        self.notify_success(&format!("App language: {value}"), cx);
    }

    pub fn cycle_terminal_shell(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["system", "bash", "zsh", "fish"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.terminal_shell)
            .unwrap_or(0);
        self.settings.terminal_shell = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.terminal_shell.clone();
        self.persist(cx);
        self.notify_success(&format!("Terminal shell: {value}"), cx);
    }

    pub fn cycle_worktree_keep_count(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[u8] = &[3, 5, 10, 20];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.worktree_keep_count)
            .unwrap_or(1);
        self.settings.worktree_keep_count = OPTIONS[(position + 1) % OPTIONS.len()];
        let value = self.settings.worktree_keep_count;
        self.persist(cx);
        self.notify_success(&format!("Worktree auto-delete limit: {value}"), cx);
    }

    pub fn cycle_branch_prefix(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["codex/", "feature/", "task/", ""];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.branch_prefix)
            .unwrap_or(0);
        self.settings.branch_prefix = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = if self.settings.branch_prefix.is_empty() {
            "(none)"
        } else {
            self.settings.branch_prefix.as_str()
        };
        self.persist(cx);
        self.notify_success(&format!("Branch prefix: {value}"), cx);
    }

    pub fn cycle_sandbox_mode(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["workspace-write", "read-only", "danger-full-access"];
        let current = self.settings.sandbox_mode.as_str();
        let position = OPTIONS
            .iter()
            .position(|option| *option == current)
            .unwrap_or(0);
        self.settings.sandbox_mode = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.sandbox_mode.clone();
        self.persist(cx);
        self.sync_live_thread_settings(cx);
        self.notify_success(&format!("Sandbox: {value}"), cx);
    }

    pub fn cycle_merge_method(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["merge", "squash", "rebase"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.merge_method)
            .unwrap_or(1);
        self.settings.merge_method = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.merge_method.clone();
        self.persist(cx);
        self.notify_success(&format!("Merge method: {value}"), cx);
    }

    pub fn cycle_review_delivery(&mut self, cx: &mut Context<Self>) {
        const OPTIONS: &[&str] = &["inline", "detached"];
        let position = OPTIONS
            .iter()
            .position(|option| *option == self.settings.review_delivery)
            .unwrap_or(0);
        self.settings.review_delivery = OPTIONS[(position + 1) % OPTIONS.len()].into();
        let value = self.settings.review_delivery.clone();
        self.persist(cx);
        self.notify_success(&format!("Review delivery: {value}"), cx);
    }

    fn sync_live_thread_settings(&self, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live || self.selected_project != "live-codex" {
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let thread_id = self.selected_task.clone();
        let model = self
            .current_task()
            .map(|task| task.model.clone())
            .unwrap_or_else(|| self.settings.default_model.clone());
        let effort = self
            .current_task()
            .map(|task| task.reasoning.clone())
            .unwrap_or_else(|| self.settings.default_reasoning.clone());
        let settings = json!({
            "model": model,
            "effort": effort,
            "approvalPolicy": self.settings.approval_mode,
            "sandboxPolicy": sandbox_policy_wire(&self.settings.sandbox_mode),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.thread_settings_update(&thread_id, settings))
                            .await
                    })
                    .await;
                if let Err(error) = result {
                    let _ = this.update(&mut async_cx.clone(), |this, cx| {
                        this.fail(&format!("Thread settings update failed: {error}"), cx)
                    });
                }
            },
        )
        .detach();
    }

    pub fn cycle_mode(&mut self, cx: &mut Context<Self>) {
        let pos = COMPOSER_MODES
            .iter()
            .position(|mode| *mode == self.composer_mode)
            .unwrap_or(0);
        self.composer_mode = COMPOSER_MODES[(pos + 1) % COMPOSER_MODES.len()].into();
        let mode = self.composer_mode.clone();
        self.notify_success(&format!("Mode: {mode}"), cx);
    }

    pub fn add_attachment(&mut self, cx: &mut Context<Self>) {
        self.attachments.push("attachment".into());
        self.notify_success("Attachment staged", cx);
    }

    pub fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.attachments.len() {
            return;
        }
        let name = attachment_name(&self.attachments[index]);
        self.attachments.remove(index);
        self.notify_success(&format!("Removed {name}"), cx);
    }

    pub fn pick_attachments(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = receiver.await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(Ok(Some(paths))) if !paths.is_empty() => {
                        for path in paths {
                            this.attachments.push(path.display().to_string());
                        }
                        this.notify_success("Files attached", cx);
                    }
                    Ok(Ok(Some(_))) | Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.fail(&format!("Attachment picker failed: {error}"), cx),
                    Err(error) => this.fail(&format!("Attachment picker cancelled: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn pick_worktree_root(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose worktree root".into()),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = receiver.await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.settings.worktree_root = path.display().to_string();
                            this.persist(cx);
                            this.notify_success("Worktree root updated", cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.fail(&format!("Worktree picker failed: {error}"), cx),
                    Err(error) => this.fail(&format!("Worktree picker cancelled: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn pick_projectless_task_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose projectless task folder".into()),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = receiver.await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.settings.projectless_task_folder = path.display().to_string();
                            this.persist(cx);
                            this.notify_success("Projectless task folder updated", cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.fail(&format!("Projectless folder picker failed: {error}"), cx)
                    }
                    Err(error) => {
                        this.fail(&format!("Projectless folder picker cancelled: {error}"), cx)
                    }
                });
            },
        )
        .detach();
    }

    pub fn pick_skill_root(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add skill folder".into()),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = receiver.await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            let path = path.display().to_string();
                            if !this.skill_roots.iter().any(|root| root == &path) {
                                this.skill_roots.push(path.clone());
                            }
                            let Some(client) = this.live_client.clone() else {
                                this.notify_success("Skill folder added locally", cx);
                                return;
                            };
                            let roots = this.skill_roots.clone();
                            let async_cx = cx.to_async();
                            cx.spawn(
                                move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                                    let result = async_cx
                                        .background_executor()
                                        .spawn(async move {
                                            smol::unblock(move || {
                                                client.skills_extra_roots_set(&roots)
                                            })
                                            .await
                                        })
                                        .await;
                                    let _ =
                                        this.update(
                                            &mut async_cx.clone(),
                                            |this, cx| match result {
                                                Ok(_) => {
                                                    this.refresh_catalog(cx);
                                                    this.notify_success("Skill folder added", cx);
                                                }
                                                Err(error) => this.fail(
                                                    &format!("Skill folder update failed: {error}"),
                                                    cx,
                                                ),
                                            },
                                        );
                                },
                            )
                            .detach();
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.fail(&format!("Skill folder picker failed: {error}"), cx)
                    }
                    Err(error) => this.fail(&format!("Skill folder picker cancelled: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn pick_project(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = receiver.await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.add_local_project(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.fail(&format!("Project picker failed: {error}"), cx),
                    Err(error) => this.fail(&format!("Project picker cancelled: {error}"), cx),
                });
            },
        )
        .detach();
    }

    fn add_local_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !path.is_dir() {
            self.fail("A project must be a directory", cx);
            return;
        }
        let path = path.to_string_lossy().into_owned();
        if let Some(project) = self
            .workspace
            .projects
            .iter()
            .find(|project| project.path == path)
        {
            if let Some(task) = project.tasks.first() {
                self.select_task(project.id.clone(), task.id.clone(), cx);
            }
            self.notify_success("Project already added", cx);
            return;
        }
        let id = unique_project_id(&path, &self.workspace);
        let name = Path::new(&path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("Project")
            .to_owned();
        let task_id = format!("{id}-task-1");
        let task = Task {
            id: task_id.clone(),
            title: "New task".into(),
            project_id: id.clone(),
            status: "idle".into(),
            path: path.clone(),
            branch: None,
            model: self.settings.default_model.clone(),
            reasoning: self.settings.default_reasoning.clone(),
            updated_at: "Now".into(),
            archived: false,
            pinned: false,
            entries: Vec::new(),
            plan: Vec::new(),
            usage: Default::default(),
            goal: None,
            children: Vec::new(),
            queue: Vec::new(),
        };
        self.workspace.projects.push(Project {
            id: id.clone(),
            name,
            path,
            tasks: vec![task],
            collapsed: false,
        });
        self.select_task(id, task_id, cx);
        self.notify_success("Project added", cx);
    }

    pub fn insert_mention(&mut self, cx: &mut Context<Self>) {
        replace_selection(
            &mut self.draft,
            &mut self.caret,
            &mut self.selection_anchor,
            "@",
        );
        cx.notify();
    }

    pub fn share_current(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.current_task().cloned() else {
            return;
        };
        let id = persistence::new_share_id(&task.id);
        let artifact = ShareArtifact {
            id: id.clone(),
            thread_id: task.id.clone(),
            title: task.title.clone(),
            created_at: unix_timestamp_label(),
            task,
        };
        match persistence::save_share(&artifact) {
            Ok(path) => {
                let link = format!("codex://shared-thread/{id}");
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
                self.notify_success(&format!("Thread share copied · {}", path.display()), cx);
            }
            Err(error) => self.fail(&format!("Could not create share artifact: {error}"), cx),
        }
    }

    pub fn review_current(&mut self, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live || self.selected_project != "live-codex" {
            let Some(task) = self.current_task().cloned() else {
                return;
            };
            match local_review_entry(&task.path, task.entries.len()) {
                Ok(entry) => {
                    if let Some(task) = self.current_task_mut() {
                        upsert_entry(task, entry);
                    }
                    self.persist(cx);
                    self.notify_success("Working tree review ready", cx);
                }
                Err(error) => self.fail(&format!("Local review failed: {error}"), cx),
            }
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let thread_id = self.selected_task.clone();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(
                        async move { smol::unblock(move || client.review_start(&thread_id)).await },
                    )
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => this.notify_success("Review started", cx),
                    Err(error) => this.fail(&format!("Review failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn stop_all_background_terminals(&mut self, cx: &mut Context<Self>) {
        let thread_id = self.selected_task.clone();
        let Some(client) = self
            .live_client
            .clone()
            .filter(|_| self.connection == ConnectionState::Live)
        else {
            self.notify_success("No live background terminals to stop", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.thread_background_terminals_clean(&thread_id))
                            .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => this.notify_success("Background terminals stopped", cx),
                    Err(error) => {
                        this.fail(&format!("Could not stop background terminals: {error}"), cx)
                    }
                });
            },
        )
        .detach();
    }

    pub fn copy_diff_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path));
        self.notify_success("Path copied", cx);
    }

    pub fn open_diff_path(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(task) = self.current_task() else {
            self.fail("Select a task before opening a diff path", cx);
            return;
        };
        let resolved = match resolve_open_path(&task.path, &path) {
            Ok(path) => path,
            Err(error) => {
                self.fail(&format!("Cannot open diff path: {error}"), cx);
                return;
            }
        };
        if let Err(error) = Command::new("xdg-open").arg(&resolved).spawn() {
            self.fail(
                &format!("Could not open {}: {error}", resolved.display()),
                cx,
            );
            return;
        }
        self.notify_success(&format!("Opened {}", resolved.display()), cx);
    }

    pub fn uninstall_plugin(&mut self, plugin_id: String, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live {
            let before = self.catalog.plugins.len();
            self.catalog.plugins.retain(|plugin| plugin != &plugin_id);
            if self.catalog.plugins.len() != before {
                self.notify_success("Plugin removed from the local catalog", cx);
            }
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.plugin_uninstall(&plugin_id)).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.refresh_catalog(cx);
                        this.notify_success("Plugin uninstalled", cx);
                    }
                    Err(error) => this.fail(&format!("Plugin uninstall failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn search_plugins(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.catalog.available_plugins = self.catalog.plugins.clone();
            self.notify_success("Plugin catalog refreshed locally", cx);
            return;
        };
        let cwds = self
            .current_task()
            .map(|task| vec![task.path.clone()])
            .unwrap_or_default();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            client.plugin_search("", None, Some(&cwds), Some(100), None)
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        this.catalog.available_plugins = nested_named_values_from_data(
                            &value,
                            &["plugins", "results", "data"],
                            &["name", "displayName", "id"],
                        );
                        this.notify_success("Plugin catalog refreshed", cx);
                    }
                    Err(error) => this.fail(&format!("Plugin search failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn install_plugin(&mut self, plugin_name: String, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live {
            if !self
                .catalog
                .plugins
                .iter()
                .any(|plugin| plugin == &plugin_name)
            {
                self.catalog.plugins.push(plugin_name);
                self.persist(cx);
                self.notify_success("Plugin installed in the local catalog", cx);
            }
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let install_attempt_id = format!(
            "install-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default()
        );
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            client.plugin_install_with_attempt(
                                &plugin_name,
                                Some(&install_attempt_id),
                                None,
                                None,
                            )
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.refresh_catalog(cx);
                        this.notify_success("Plugin installation started", cx);
                    }
                    Err(error) => this.fail(&format!("Plugin install failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn steer_current(&mut self, cx: &mut Context<Self>) {
        if !self.streaming {
            return;
        }
        let text = self.draft.trim().to_owned();
        let Some(turn_id) = self.active_turn_id.clone() else {
            self.notify_success("This turn is not steerable yet", cx);
            return;
        };
        if text.is_empty() {
            self.notify_success("Type a steering instruction first", cx);
            return;
        }
        let Some(client) = self.live_client.clone() else {
            self.notify_success("Steering is available for live Codex turns", cx);
            return;
        };
        let thread_id = self.selected_task.clone();
        self.draft.clear();
        self.caret = 0;
        self.selection_anchor = None;
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.turn_steer(&thread_id, &turn_id, &text)).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => this.notify_success("Turn steered", cx),
                    Err(error) => this.fail(&format!("Could not steer turn: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn compact_current(&mut self, cx: &mut Context<Self>) {
        let thread_id = self.selected_task.clone();
        if self.connection != ConnectionState::Live || self.selected_project != "live-codex" {
            self.notify_success("Compaction is available for live Codex tasks", cx);
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result =
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            smol::unblock(move || client.thread_compact(&thread_id)).await
                        })
                        .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => this.notify_success("Compaction started", cx),
                    Err(error) => this.fail(&format!("Compaction failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.current_task() else {
            return;
        };
        self.rename_draft = task.title.clone();
        self.rename_caret = self.rename_draft.chars().count();
        self.rename_selection_anchor = None;
        self.rename_open = true;
        self.menu_open = false;
        window.focus(&self.rename_focus);
        cx.notify();
    }

    fn instruction_value(&self, field: &str) -> &str {
        match field {
            "custom-instructions" => &self.settings.custom_instructions,
            "commit-instructions" => &self.settings.commit_instructions,
            "pull-request-instructions" => &self.settings.pull_request_instructions,
            "pull-request-watch-instructions" => &self.settings.pull_request_watch_instructions,
            _ => "",
        }
    }

    pub fn begin_instruction_edit(
        &mut self,
        field: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_draft = self.instruction_value(field).to_owned();
        self.settings_caret = self.settings_draft.chars().count();
        self.settings_selection_anchor = None;
        self.settings_editor = Some(field);
        window.focus(&self.settings_focus);
        cx.notify();
    }

    pub fn begin_mcp_server_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_draft =
            "{\n  \"name\": \"my-server\",\n  \"command\": \"your-command\",\n  \"args\": []\n}"
                .into();
        self.settings_caret = self.settings_draft.chars().count();
        self.settings_selection_anchor = None;
        self.settings_editor = Some("mcp-server");
        window.focus(&self.settings_focus);
        cx.notify();
    }

    pub fn cancel_instruction_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_editor = None;
        self.settings_draft.clear();
        self.settings_caret = 0;
        self.settings_selection_anchor = None;
        window.focus(&self.root_focus);
        cx.notify();
    }

    pub fn commit_instruction_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.settings_editor.take() else {
            return;
        };
        let value = self.settings_draft.trim().to_owned();
        if field == "mcp-server" {
            self.settings_draft.clear();
            self.settings_caret = 0;
            self.settings_selection_anchor = None;
            window.focus(&self.root_focus);
            self.add_mcp_server_from_json(&value, cx);
            return;
        }
        match field {
            "custom-instructions" => self.settings.custom_instructions = value,
            "commit-instructions" => self.settings.commit_instructions = value,
            "pull-request-instructions" => self.settings.pull_request_instructions = value,
            "pull-request-watch-instructions" => {
                self.settings.pull_request_watch_instructions = value
            }
            _ => {}
        }
        self.settings_draft.clear();
        self.settings_caret = 0;
        self.settings_selection_anchor = None;
        window.focus(&self.root_focus);
        self.persist(cx);
        let message = match field {
            "custom-instructions" => "Custom instructions saved",
            "commit-instructions" => "Commit instructions saved",
            "pull-request-instructions" => "Pull request instructions saved",
            "pull-request-watch-instructions" => "Pull request watch instructions saved",
            _ => "Instructions saved",
        };
        self.notify_success(message, cx);
    }

    fn add_mcp_server_from_json(&mut self, source: &str, cx: &mut Context<Self>) {
        let mut config: Value = match serde_json::from_str(source) {
            Ok(config) => config,
            Err(error) => {
                self.fail(&format!("MCP server JSON is invalid: {error}"), cx);
                return;
            }
        };
        let Some(object) = config.as_object_mut() else {
            self.fail("MCP server configuration must be a JSON object", cx);
            return;
        };
        let Some(name) = object
            .remove("name")
            .and_then(|value| value.as_str().map(str::to_owned))
        else {
            self.fail("MCP server configuration needs a name", cx);
            return;
        };
        if !valid_mcp_server_name(&name) {
            self.fail(
                "MCP server names may use letters, numbers, hyphens, and underscores",
                cx,
            );
            return;
        }
        let has_command = object
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| !command.trim().is_empty());
        let has_url = object
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| !url.trim().is_empty());
        if !has_command && !has_url {
            self.fail("MCP server configuration needs a command or URL", cx);
            return;
        }
        if self
            .catalog
            .mcp_servers
            .iter()
            .all(|server| server != &name)
        {
            self.catalog.mcp_servers.push(name.clone());
        }
        let Some(client) = self
            .live_client
            .clone()
            .filter(|_| self.connection == ConnectionState::Live)
        else {
            self.persist(cx);
            self.notify_success("MCP server added to the local catalog", cx);
            return;
        };
        let key_path = format!("mcp_servers.{name}");
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            client.config_value_write(&key_path, "upsert", config)?;
                            client.config_mcp_server_reload()
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.refresh_catalog(cx);
                        this.notify_success("MCP server added", cx);
                    }
                    Err(error) => this.fail(&format!("MCP server add failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename_open = false;
        self.rename_draft.clear();
        self.rename_caret = 0;
        self.rename_selection_anchor = None;
        cx.notify();
    }

    pub fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.rename_draft.trim().to_string();
        if name.is_empty() {
            self.cancel_rename(cx);
            window.focus(&self.input_focus);
            return;
        }
        let thread_id = self.selected_task.clone();
        let live =
            self.connection == ConnectionState::Live && self.selected_project == "live-codex";
        if let Some(task) = self.current_task_mut() {
            task.title = name.clone();
        }
        self.rename_open = false;
        self.rename_draft.clear();
        self.rename_caret = 0;
        window.focus(&self.input_focus);
        self.persist(cx);
        if live {
            if let Some(client) = self.live_client.clone() {
                let async_cx = cx.to_async();
                cx.spawn(
                    move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                        let result = async_cx
                            .background_executor()
                            .spawn(async move {
                                smol::unblock(move || client.thread_name_set(&thread_id, &name))
                                    .await
                            })
                            .await;
                        if let Err(error) = result {
                            let _ = this.update(&mut async_cx.clone(), |this, cx| {
                                this.fail(&format!("Rename failed: {error}"), cx)
                            });
                        }
                    },
                )
                .detach();
            }
        }
        self.notify_success("Task renamed", cx);
    }

    pub fn toggle_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        self.app_menu = None;
        self.view_open = false;
        cx.notify();
    }

    pub fn toggle_view_options(&mut self, cx: &mut Context<Self>) {
        self.view_open = !self.view_open;
        self.menu_open = false;
        self.app_menu = None;
        cx.notify();
    }

    pub fn toggle_app_menu(&mut self, menu: AppMenu, cx: &mut Context<Self>) {
        self.app_menu = (self.app_menu != Some(menu)).then_some(menu);
        self.menu_open = false;
        self.view_open = false;
        cx.notify();
    }

    pub fn close_app_menu(&mut self, cx: &mut Context<Self>) {
        self.app_menu = None;
        cx.notify();
    }

    pub fn clear_draft(&mut self, cx: &mut Context<Self>) {
        self.draft.clear();
        self.caret = 0;
        self.selection_anchor = None;
        self.attachments.clear();
        self.notify_success("Composer cleared", cx);
    }

    pub fn set_content_layout(&mut self, layout: &str, cx: &mut Context<Self>) {
        let layout = normalize_content_layout(layout);
        self.content_layout = layout.to_owned();
        match layout {
            "Files" | "Side chat" | "Browser" | "Review" => {
                self.side_panel_open = true;
                self.bottom_panel_open = false;
            }
            "Detail" | "Terminal" => {
                self.bottom_panel_open = true;
                self.side_panel_open = false;
            }
            _ => {
                self.side_panel_open = false;
                self.bottom_panel_open = false;
            }
        }
        self.view_open = false;
        self.persist(cx);
        self.notify_success(&format!("Content layout: {}", self.content_layout), cx);
    }

    pub fn toggle_bottom_panel(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel_open = !self.bottom_panel_open;
        if self.bottom_panel_open && self.content_layout == "Chat" {
            self.content_layout = "Detail".into();
        }
        self.persist(cx);
        self.notify_success(
            if self.bottom_panel_open {
                "Bottom panel opened"
            } else {
                "Bottom panel closed"
            },
            cx,
        );
    }

    pub fn toggle_side_panel(&mut self, cx: &mut Context<Self>) {
        self.side_panel_open = !self.side_panel_open;
        if self.side_panel_open && self.content_layout == "Chat" {
            self.content_layout = "Side chat".into();
        }
        self.persist(cx);
        self.notify_success(
            if self.side_panel_open {
                "Side panel opened"
            } else {
                "Side panel closed"
            },
            cx,
        );
    }

    pub fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fullscreen = !self.fullscreen;
        window.toggle_fullscreen();
        self.persist(cx);
        self.notify_success(
            if self.fullscreen {
                "Fullscreen layout enabled"
            } else {
                "Fullscreen layout disabled"
            },
            cx,
        );
    }

    pub fn reset_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = false;
        self.content_layout = "Chat".into();
        self.bottom_panel_open = false;
        self.side_panel_open = false;
        if self.fullscreen {
            self.fullscreen = false;
            window.toggle_fullscreen();
        }
        self.view_open = false;
        self.persist(cx);
        self.notify_success("View reset", cx);
    }

    pub fn toggle_reasoning(&mut self, reasoning_id: String, cx: &mut Context<Self>) {
        let Some(task) = self.current_task_mut() else {
            return;
        };
        if let Some(Entry::Reasoning { collapsed, .. }) = task
            .entries
            .iter_mut()
            .find(|entry| entry_id(entry) == reasoning_id)
        {
            *collapsed = !*collapsed;
            cx.notify();
        }
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.view_open = false;
        self.persist(cx);
        cx.notify();
    }

    pub fn toggle_archived_visibility(&mut self, cx: &mut Context<Self>) {
        self.show_archived = !self.show_archived;
        self.view_open = false;
        self.ensure_selection();
        self.persist(cx);
        cx.notify();
    }

    pub fn archive_current(&mut self, cx: &mut Context<Self>) {
        let task_id = self.selected_task.clone();
        let live =
            self.connection == ConnectionState::Live && self.selected_project == "live-codex";
        if let Some(task) = self.current_task_mut() {
            task.archived = true;
            task.status = "archived".into();
        }
        self.ensure_selection();
        self.persist(cx);
        self.notify_success(&format!("Archived {task_id}"), cx);
        if live {
            self.request_thread_action(task_id, ThreadAction::Archive, cx);
        }
    }

    pub fn unarchive_current(&mut self, cx: &mut Context<Self>) {
        let task_id = self.selected_task.clone();
        let live =
            self.connection == ConnectionState::Live && self.selected_project == "live-codex";
        let archived = self
            .current_task()
            .map(|task| task.archived)
            .unwrap_or(false);
        if !archived {
            return;
        }
        if let Some(task) = self.current_task_mut() {
            task.archived = false;
            task.status = "idle".into();
        }
        self.persist(cx);
        self.notify_success(&format!("Unarchived {task_id}"), cx);
        if live {
            self.request_thread_action(task_id, ThreadAction::Unarchive, cx);
        }
    }

    pub fn resume_current(&mut self, cx: &mut Context<Self>) {
        let task_id = self.selected_task.clone();
        let live =
            self.connection == ConnectionState::Live && self.selected_project == "live-codex";
        let closed = self
            .current_task()
            .map(|task| task.status == "closed")
            .unwrap_or(false);
        if !closed {
            return;
        }
        if let Some(task) = self.current_task_mut() {
            task.status = "idle".into();
        }
        self.persist(cx);
        self.notify_success("Task resumed", cx);
        if !live {
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(
                        async move { smol::unblock(move || client.thread_resume(&task_id)).await },
                    )
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        if let Some(thread) = value.get("thread").and_then(ServerThread::from_value)
                        {
                            this.add_server_thread(thread);
                            this.persist(cx);
                            cx.notify();
                        }
                    }
                    Err(error) => this.fail(&format!("Resume failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn toggle_pin_current(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.current_task_mut() else {
            return;
        };
        task.pinned = !task.pinned;
        let message = if task.pinned {
            "Task pinned"
        } else {
            "Task unpinned"
        };
        self.persist(cx);
        self.notify_success(message, cx);
    }

    pub fn delete_current(&mut self, cx: &mut Context<Self>) {
        let project_id = self.selected_project.clone();
        let task_id = self.selected_task.clone();
        let live = self.connection == ConnectionState::Live && project_id == "live-codex";
        if let Some(project) = self
            .workspace
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.tasks.retain(|task| task.id != task_id);
        }
        self.ensure_selection();
        self.persist(cx);
        self.notify_success("Task deleted", cx);
        if live {
            self.request_thread_action(task_id, ThreadAction::Delete, cx);
        }
    }

    pub fn fork_current(&mut self, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live || self.selected_project != "live-codex" {
            let Some(project) = self.current_project().cloned() else {
                return;
            };
            let Some(source) = self.current_task().cloned() else {
                return;
            };
            let id = format!("fork-{}", self.workspace.all_tasks().count() + 1);
            let mut fork = source;
            fork.id = id.clone();
            fork.title = format!("Fork of {}", fork.title);
            fork.status = "idle".into();
            fork.updated_at = "Now".into();
            fork.pinned = false;
            if let Some(project) = self
                .workspace
                .projects
                .iter_mut()
                .find(|candidate| candidate.id == project.id)
            {
                project.tasks.insert(0, fork);
            }
            self.select_task(project.id, id, cx);
            self.notify_success("Task forked in this workspace", cx);
            return;
        }
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let thread_id = self.selected_task.clone();
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(
                        async move { smol::unblock(move || client.thread_fork(&thread_id)).await },
                    )
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(value) => {
                        if let Some(thread) = value.get("thread").and_then(ServerThread::from_value)
                        {
                            let new_id = thread.id.clone();
                            this.add_server_thread(thread);
                            this.select_task("live-codex".into(), new_id, cx);
                            this.notify_success("Task forked", cx);
                        }
                    }
                    Err(error) => this.fail(&format!("Fork failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn fork_current_in_new_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.current_project().cloned() else {
            return;
        };
        let Some(source) = self.current_task().cloned() else {
            return;
        };
        let repository = project.path;
        let root = if self.settings.worktree_root.is_empty() {
            PathBuf::from(&repository).join(".codex-worktrees")
        } else {
            PathBuf::from(&self.settings.worktree_root)
        };
        let suffix = format!(
            "fork-{}-{}",
            self.workspace.all_tasks().count() + 1,
            std::process::id()
        );
        let branch = format!("{}{}", self.settings.branch_prefix, suffix);
        let live_client = self
            .live_client
            .clone()
            .filter(|_| self.connection == ConnectionState::Live);
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || {
                            let path = create_git_worktree(&repository, &root, &branch)?;
                            let thread = if let Some(client) = live_client {
                                let value = client.thread_start(path.to_str());
                                match value {
                                    Ok(value) => value
                                        .get("thread")
                                        .and_then(ServerThread::from_value)
                                        .map(Ok)
                                        .unwrap_or_else(|| {
                                            Err(anyhow::anyhow!(
                                                "app-server did not return a thread for the worktree"
                                            ))
                                        })?,
                                    Err(error) => return Err(error),
                                }
                            } else {
                                return Ok((path, branch, None));
                            };
                            Ok((path, branch, Some(thread)))
                        })
                        .await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok((path, _branch, Some(mut thread))) => {
                        if thread.cwd.is_empty() {
                            thread.cwd = path.to_string_lossy().into_owned();
                        }
                        let id = thread.id.clone();
                        this.add_server_thread(thread);
                        this.select_task("live-codex".into(), id, cx);
                        this.refresh_worktrees(cx);
                        this.notify_success("Forked chat in a new worktree", cx);
                    }
                    Ok((path, branch, None)) => {
                        this.add_local_fork_project(path, &branch, &source, cx);
                        this.refresh_worktrees(cx);
                    }
                    Err(error) => this.fail(&format!("New worktree fork failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    fn add_local_fork_project(
        &mut self,
        path: PathBuf,
        branch: &str,
        source: &Task,
        cx: &mut Context<Self>,
    ) {
        let path_text = path.to_string_lossy().into_owned();
        let project_id = unique_project_id(&path_text, &self.workspace);
        let task_id = format!("{project_id}-task-1");
        let mut task = source.clone();
        task.id = task_id.clone();
        task.project_id = project_id.clone();
        task.path = path_text.clone();
        task.branch = Some(branch.to_owned());
        task.title = format!("Fork of {}", source.title);
        task.status = "idle".into();
        task.updated_at = "Now".into();
        task.pinned = false;
        self.workspace.projects.push(Project {
            id: project_id.clone(),
            name: Path::new(&path_text)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Worktree")
                .to_owned(),
            path: path_text,
            tasks: vec![task],
            collapsed: false,
        });
        self.select_task(project_id, task_id, cx);
        self.notify_success("Forked chat in a new worktree", cx);
    }

    fn request_thread_action(
        &self,
        thread_id: String,
        action: ThreadAction,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.live_client.clone() else {
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || match action {
                            ThreadAction::Archive => client.thread_archive(&thread_id),
                            ThreadAction::Unarchive => client.thread_unarchive(&thread_id),
                            ThreadAction::Delete => client.thread_delete(&thread_id),
                        })
                        .await
                    })
                    .await;
                if let Err(error) = result {
                    let _ = this.update(&mut async_cx.clone(), |this, cx| {
                        this.fail(&format!("Task action failed: {error}"), cx)
                    });
                }
            },
        )
        .detach();
    }

    pub fn approve_current(&mut self, approved: bool, cx: &mut Context<Self>) {
        let Some(interaction_id) = self
            .pending_interaction
            .as_ref()
            .map(pending_interaction_key)
        else {
            return;
        };
        self.approve_interaction(&interaction_id, approved, cx);
    }

    pub fn approve_interaction(
        &mut self,
        interaction_id: &str,
        approved: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .pending_interactions
            .iter()
            .position(|pending| pending_interaction_key(pending) == interaction_id)
        else {
            return;
        };
        let pending = self.pending_interactions.remove(index);
        let task_id = if pending.thread_id.is_empty() {
            self.selected_task.clone()
        } else {
            pending.thread_id.clone()
        };
        if let Some(task) = self.task_mut_by_id(&task_id) {
            let label = if approved { "Approved" } else { "Declined" };
            for entry in &mut task.entries {
                if let Entry::Approval { id, requested, .. } = entry {
                    if id == interaction_id {
                        *requested = false;
                    }
                }
            }
            task.entries.push(Entry::System {
                id: format!("approval-{}", task.entries.len()),
                text: format!("{label} by user"),
            });
        }
        if let Some(client) = self.live_client.clone() {
            let response = pending.response(approved);
            if let Err(error) = client.respond(pending.request_id, response) {
                self.fail(&format!("Approval response failed: {error}"), cx);
            }
        }
        self.promote_pending_interaction();
        self.persist(cx);
        self.notify_success(
            if approved {
                "Approval accepted"
            } else {
                "Approval declined"
            },
            cx,
        );
    }

    pub fn toggle_bool_setting(&mut self, setting: &str, cx: &mut Context<Self>) {
        match setting {
            "notifications" => self.settings.notifications = !self.settings.notifications,
            "sound" => self.settings.sound_effects = !self.settings.sound_effects,
            "reduced-motion" => self.settings.reduced_motion = !self.settings.reduced_motion,
            "context-usage" => self.settings.show_context_usage = !self.settings.show_context_usage,
            "bottom-panel-control" => {
                self.settings.show_bottom_panel_control = !self.settings.show_bottom_panel_control
            }
            "full-access" => self.settings.show_full_access = !self.settings.show_full_access,
            "educational-tips" => {
                self.settings.show_educational_tips = !self.settings.show_educational_tips
            }
            "ambient-suggestions" => {
                self.settings.ambient_suggestions = !self.settings.ambient_suggestions
            }
            "queue-follow-ups" => self.settings.queue_follow_ups = !self.settings.queue_follow_ups,
            "worktree-auto-fetch" => {
                self.settings.worktree_auto_fetch = !self.settings.worktree_auto_fetch
            }
            "worktree-auto-cleanup" => {
                self.settings.worktree_auto_cleanup = !self.settings.worktree_auto_cleanup
            }
            "git-review" => self.settings.git_review_enabled = !self.settings.git_review_enabled,
            "git-review-disabled" => {
                self.settings.git_review_enabled = !self.settings.git_review_enabled
            }
            "force-push" => self.settings.force_push = !self.settings.force_push,
            "draft-prs" => self.settings.draft_prs = !self.settings.draft_prs,
            "auto-merge" => self.settings.auto_merge = !self.settings.auto_merge,
            "watch-pull-requests" => {
                self.settings.watch_and_fix_pull_requests =
                    !self.settings.watch_and_fix_pull_requests
            }
            "voice" => self.settings.voice_enabled = !self.settings.voice_enabled,
            "analytics" => self.settings.analytics_enabled = !self.settings.analytics_enabled,
            "debug-logging" => self.settings.debug_logging = !self.settings.debug_logging,
            "hooks" => self.settings.hooks_enabled = !self.settings.hooks_enabled,
            "cloud" => self.settings.cloud_enabled = !self.settings.cloud_enabled,
            "computer-use" => {
                self.settings.computer_use_enabled = !self.settings.computer_use_enabled
            }
            "browser-use" => self.settings.browser_use_enabled = !self.settings.browser_use_enabled,
            "plugin-auto-update" => {
                self.settings.plugin_auto_update = !self.settings.plugin_auto_update
            }
            _ => {}
        }
        self.persist(cx);
        cx.notify();
    }

    pub fn reload_mcp_servers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.notify_success("MCP server list refreshed locally", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result =
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            smol::unblock(move || client.config_mcp_server_reload()).await
                        })
                        .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.refresh_catalog(cx);
                        this.notify_success("MCP servers reloaded", cx);
                    }
                    Err(error) => this.fail(&format!("MCP reload failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn refresh_marketplaces(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.live_client.clone() else {
            self.notify_success("Marketplace list refreshed locally", cx);
            return;
        };
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result =
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            smol::unblock(move || client.marketplace_upgrade(None)).await
                        })
                        .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.refresh_catalog(cx);
                        this.notify_success("Marketplaces refreshed", cx);
                    }
                    Err(error) => this.fail(&format!("Marketplace refresh failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn toggle_voice(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self
            .current_task()
            .filter(|_| self.selected_project == "live-codex")
            .map(|_| self.selected_task.clone())
        else {
            self.notify_success("Voice input is available for live Codex tasks", cx);
            return;
        };
        let Some(client) = self.live_client.clone() else {
            self.notify_success("Connect to Codex before starting voice input", cx);
            return;
        };
        if self.voice_active {
            self.voice_active = false;
            cx.spawn(
                move |_this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                    let _ = smol::unblock(move || client.realtime_stop(&thread_id)).await;
                },
            )
            .detach();
            self.notify_success("Voice input stopped", cx);
            return;
        }
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                let result = async_cx
                    .background_executor()
                    .spawn(async move {
                        smol::unblock(move || client.realtime_start(&thread_id, "text")).await
                    })
                    .await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                    Ok(_) => {
                        this.voice_active = true;
                        this.notify_success("Voice input started", cx);
                    }
                    Err(error) => this.fail(&format!("Voice input failed: {error}"), cx),
                });
            },
        )
        .detach();
    }

    pub fn retry_current(&mut self, cx: &mut Context<Self>) {
        if self.streaming || self.busy {
            return;
        }
        let text = self.current_task().and_then(|task| {
            task.entries.iter().rev().find_map(|entry| match entry {
                Entry::User { text, .. } => Some(text.clone()),
                _ => None,
            })
        });
        let Some(text) = text else {
            self.notify_success("There is no completed user turn to retry", cx);
            return;
        };
        self.draft = text;
        self.caret = self.draft.chars().count();
        self.selection_anchor = None;
        self.send(cx);
    }

    pub fn handle_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = &event.keystroke;
        if key.key == "f2" {
            self.begin_rename(window, cx);
            return true;
        }
        if handle_editor_shortcut(
            &mut self.draft,
            &mut self.caret,
            &mut self.selection_anchor,
            key,
            cx,
        ) {
            return true;
        }
        if self.handle_command_shortcut(key, window, cx) {
            return true;
        }
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return false;
        }
        let action = apply_input_edit_with_enter(
            &mut self.draft,
            &mut self.caret,
            &mut self.selection_anchor,
            &key.key,
            key.key_char.as_deref(),
            key.modifiers.shift,
            self.streaming,
            self.settings.enter_behavior != "newline",
        );
        if action == InputAction::Send {
            self.send(cx);
        } else {
            cx.notify();
        }
        false
    }

    pub fn handle_global_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        if key.key == "escape" {
            if self.rename_open {
                self.cancel_rename(cx);
            } else if self.settings_editor.is_some() {
                self.cancel_instruction_edit(window, cx);
            } else if self.menu_open {
                self.menu_open = false;
                cx.notify();
            } else if self.view_open {
                self.view_open = false;
                cx.notify();
            } else if self.app_menu.is_some() {
                self.app_menu = None;
                cx.notify();
            } else if self.search_open {
                self.search_open = false;
                self.query.clear();
                cx.notify();
            }
            return;
        }
        if self.handle_command_shortcut(key, window, cx) {
            return;
        }
        if key.key == "f2" && self.route == Route::Task {
            self.begin_rename(window, cx);
        }
    }

    fn handle_command_shortcut(
        &mut self,
        key: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let command = key.modifiers.platform || key.modifiers.control;
        if !command {
            return false;
        }
        match (key.modifiers.shift, key.key.as_str()) {
            (false, "k") => self.toggle_search(window, cx),
            (false, "n") => self.create_live_task(cx),
            (false, ",") | (false, "comma") => {
                self.open_settings(SettingsPage::General, cx);
                window.focus(&self.root_focus);
            }
            (true, "a") if self.pending_interaction.is_some() => self.approve_current(true, cx),
            (true, "d") if self.pending_interaction.is_some() => self.approve_current(false, cx),
            (true, "b") => self.toggle_sidebar(cx),
            (true, "e") => self.add_attachment(cx),
            (true, "f") => self.insert_mention(cx),
            (true, "m") => self.cycle_model(cx),
            (true, "p") => self.toggle_pin_current(cx),
            (true, "r") => self.cycle_reasoning(cx),
            (true, "s") => self.share_current(cx),
            (true, "x") => self.stop_turn(cx),
            (true, "z") => self.archive_current(cx),
            (true, "delete") => self.delete_current(cx),
            _ => return false,
        }
        true
    }

    pub fn handle_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = &event.keystroke;
        if key.key == "escape" {
            self.search_open = false;
            self.query.clear();
            self.query_selection_anchor = None;
            window.focus(&self.input_focus);
            cx.notify();
            return true;
        }
        if self.handle_command_shortcut(key, window, cx) {
            return true;
        }
        if handle_editor_shortcut(
            &mut self.query,
            &mut self.caret,
            &mut self.query_selection_anchor,
            key,
            cx,
        ) {
            return true;
        }
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return false;
        }
        let _ = apply_input_edit_with_selection(
            &mut self.query,
            &mut self.caret,
            &mut self.query_selection_anchor,
            &key.key,
            key.key_char.as_deref(),
            key.modifiers.shift,
            false,
        );
        if key.key == "enter" {
            window.focus(&self.search_focus);
        }
        cx.notify();
        false
    }

    pub fn handle_rename_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        if key.key == "escape" {
            self.cancel_rename(cx);
            window.focus(&self.input_focus);
            return;
        }
        if handle_editor_shortcut(
            &mut self.rename_draft,
            &mut self.rename_caret,
            &mut self.rename_selection_anchor,
            key,
            cx,
        ) {
            return;
        }
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return;
        }
        if apply_input_edit_with_selection(
            &mut self.rename_draft,
            &mut self.rename_caret,
            &mut self.rename_selection_anchor,
            &key.key,
            key.key_char.as_deref(),
            key.modifiers.shift,
            false,
        ) == InputAction::Send
        {
            self.commit_rename(window, cx);
        } else {
            cx.notify();
        }
    }

    pub fn handle_instruction_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        if key.key == "escape" {
            self.cancel_instruction_edit(window, cx);
            return;
        }
        if key.key == "enter"
            && !key.modifiers.shift
            && (key.modifiers.platform || key.modifiers.control)
        {
            self.commit_instruction_edit(window, cx);
            return;
        }
        if handle_editor_shortcut(
            &mut self.settings_draft,
            &mut self.settings_caret,
            &mut self.settings_selection_anchor,
            key,
            cx,
        ) {
            return;
        }
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return;
        }
        let _ = apply_input_edit_with_selection(
            &mut self.settings_draft,
            &mut self.settings_caret,
            &mut self.settings_selection_anchor,
            &key.key,
            key.key_char.as_deref(),
            key.modifiers.shift,
            false,
        );
        cx.notify();
    }

    fn ensure_selection(&mut self) {
        let selected_exists = self
            .workspace
            .task(&self.selected_project, &self.selected_task)
            .map(|task| self.show_archived || !task.archived)
            .is_some();
        if selected_exists {
            return;
        }
        if let Some((project, task)) = self
            .workspace
            .all_tasks()
            .find(|(_, task)| self.show_archived || !task.archived)
        {
            self.selected_project = project.id.clone();
            self.selected_task = task.id.clone();
        }
    }

    fn persist(&self, _cx: &mut Context<Self>) {
        let _ = persistence::save(&self.snapshot());
    }

    pub fn notify_success(&mut self, message: &str, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        self.clear_toast_later(cx);
        cx.notify();
    }

    fn fail(&mut self, message: &str, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        self.clear_toast_later(cx);
        cx.notify();
    }

    fn clear_toast_later(&mut self, cx: &mut Context<Self>) {
        let async_cx = cx.to_async();
        cx.spawn(
            move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                smol::Timer::after(Duration::from_secs(4)).await;
                let _ = this.update(&mut async_cx.clone(), |this, cx| {
                    this.toast = None;
                    cx.notify();
                });
            },
        )
        .detach();
    }

    fn import_live_threads(&mut self, threads: Vec<ServerThread>) {
        self.workspace
            .projects
            .retain(|project| project.id == "codex-app-gpui");
        let mut live = Project {
            id: "live-codex".into(),
            name: "Live Codex".into(),
            path: threads
                .first()
                .map(|thread| thread.cwd.clone())
                .unwrap_or_default(),
            collapsed: false,
            tasks: Vec::new(),
        };
        for thread in threads {
            if !live.tasks.iter().any(|task: &Task| task.id == thread.id) {
                live.tasks.push(task_from_server(&thread));
            }
        }
        self.workspace.projects.insert(0, live);
        if let Some(task) = self
            .workspace
            .projects
            .first()
            .and_then(|project| project.tasks.first())
        {
            self.selected_project = "live-codex".into();
            self.selected_task = task.id.clone();
        }
    }

    fn add_server_thread(&mut self, thread: ServerThread) {
        if !self
            .workspace
            .projects
            .iter()
            .any(|project| project.id == "live-codex")
        {
            self.workspace.projects.insert(
                0,
                Project {
                    id: "live-codex".into(),
                    name: "Live Codex".into(),
                    path: thread.cwd.clone(),
                    collapsed: false,
                    tasks: Vec::new(),
                },
            );
        }
        if let Some(project) = self
            .workspace
            .projects
            .iter_mut()
            .find(|project| project.id == "live-codex")
        {
            if let Some(task) = project.tasks.iter_mut().find(|task| task.id == thread.id) {
                task.title = thread.title;
                task.status = thread.status;
                task.path = thread.cwd;
                if !thread.model.is_empty() {
                    task.model = thread.model;
                }
                if !thread.updated_at.is_empty() {
                    task.updated_at = thread.updated_at;
                }
                task.archived = thread.archived;
                if thread.archived {
                    task.status = "archived".into();
                }
            } else {
                project.tasks.insert(0, task_from_server(&thread));
            }
        }
    }
}

fn model_options_from_value(value: &Value) -> Vec<ModelOption> {
    let mut options = Vec::new();
    collect_model_options(value, &mut options);
    let mut seen = HashSet::new();
    options.retain(|option| seen.insert(option.id.clone()));
    options
}

fn collect_model_options(value: &Value, options: &mut Vec<ModelOption>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_model_options(value, options);
            }
        }
        Value::Object(object) => {
            let id = ["model", "id", "slug"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_str))
                .filter(|id| !id.is_empty());
            if let Some(id) = id {
                let label = ["displayName", "display_name", "name", "title"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(Value::as_str))
                    .filter(|label| !label.is_empty())
                    .unwrap_or(id)
                    .to_owned();
                let mut reasoning = Vec::new();
                if let Some(values) = object
                    .get("supportedReasoningEfforts")
                    .or_else(|| object.get("supported_reasoning_efforts"))
                    .and_then(Value::as_array)
                {
                    for value in values {
                        let effort = value
                            .as_str()
                            .or_else(|| value.get("reasoningEffort").and_then(Value::as_str))
                            .or_else(|| value.get("reasoning_effort").and_then(Value::as_str))
                            .or_else(|| value.get("id").and_then(Value::as_str));
                        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
                            reasoning.push(effort.to_owned());
                        }
                    }
                }
                dedupe_strings(&mut reasoning);
                options.push(ModelOption {
                    id: id.to_owned(),
                    label,
                    reasoning,
                });
            }
            for value in object.values() {
                collect_model_options(value, options);
            }
        }
        _ => {}
    }
}

fn named_values_from_data(value: &Value, keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    collect_named_values(value, keys, &mut values);
    dedupe_strings(&mut values);
    values
}

fn nested_named_values_from_data(value: &Value, containers: &[&str], keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    collect_nested_named_values(value, containers, keys, &mut values);
    dedupe_strings(&mut values);
    values
}

fn collect_named_values(value: &Value, keys: &[&str], values: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_named_values(item, keys, values);
            }
        }
        Value::Object(object) => {
            for key in keys {
                if let Some(name) = object.get(*key).and_then(Value::as_str) {
                    if !name.is_empty() {
                        values.push(name.to_owned());
                    }
                }
            }
            for item in object.values() {
                collect_named_values(item, keys, values);
            }
        }
        _ => {}
    }
}

fn collect_nested_named_values(
    value: &Value,
    containers: &[&str],
    keys: &[&str],
    values: &mut Vec<String>,
) {
    if containers.is_empty() {
        collect_named_values(value, keys, values);
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_nested_named_values(item, containers, keys, values);
            }
        }
        Value::Object(object) => {
            for container in containers {
                if let Some(value) = object.get(*container) {
                    collect_named_values(value, keys, values);
                    collect_nested_named_values(value, containers, keys, values);
                }
            }
            for value in object.values() {
                collect_nested_named_values(value, containers, keys, values);
            }
        }
        _ => {}
    }
}

fn account_label_from_value(value: &Value) -> Option<String> {
    let object = value
        .get("account")
        .or_else(|| value.get("user"))
        .unwrap_or(value);
    ["email", "name", "id", "type"]
        .iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn config_summary_from_value(value: &Value) -> Vec<String> {
    let object = value.get("config").unwrap_or(value);
    let mut keys = object
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn valid_mcp_server_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn resolve_open_path(root: &str, requested: &str) -> anyhow::Result<PathBuf> {
    let root = Path::new(root)
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("task directory is unavailable: {error}"))?;
    let requested = requested.trim();
    let candidate = if requested.is_empty() || requested == "Working tree" {
        root.clone()
    } else {
        let requested_path = Path::new(requested);
        if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            root.join(requested_path)
        }
    };
    let candidate = candidate
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("path does not exist: {error}"))?;
    if candidate != root && !candidate.starts_with(&root) {
        return Err(anyhow::anyhow!("path is outside the task directory"));
    }
    Ok(candidate)
}

#[derive(Debug, Clone, Copy)]
enum ThreadAction {
    Archive,
    Unarchive,
    Delete,
}

fn connect_live(
    command: &str,
    cwd: Option<&str>,
) -> anyhow::Result<(Arc<AppServerClient>, Vec<ServerThread>, ServerCatalog)> {
    let client = Arc::new(AppServerClient::spawn(command)?);
    client.initialize()?;
    let mut threads = client.thread_list(None)?;
    let archived_threads = client
        .thread_list_with_options(None, Some(true))
        .unwrap_or_default();
    merge_threads(&mut threads, archived_threads);
    if threads.is_empty() && std::env::var_os("CODEX_APP_GPUI_CREATE_LIVE_THREAD").is_some() {
        if let Some(thread) = client
            .thread_start(cwd)?
            .get("thread")
            .and_then(ServerThread::from_value)
        {
            threads.push(thread);
        }
    }
    let catalog = ServerCatalog::from_client(&client, cwd);
    Ok((client, threads, catalog))
}

fn merge_threads(existing: &mut Vec<ServerThread>, additional: Vec<ServerThread>) {
    let mut known = existing
        .iter()
        .map(|thread| thread.id.clone())
        .collect::<HashSet<_>>();
    for thread in additional {
        if !known.contains(&thread.id) {
            known.insert(thread.id.clone());
            existing.push(thread);
        }
    }
}

fn task_from_server(thread: &ServerThread) -> Task {
    Task {
        id: thread.id.clone(),
        title: thread.title.clone(),
        project_id: "live-codex".into(),
        status: thread.status.clone(),
        path: thread.cwd.clone(),
        branch: None,
        model: if thread.model.is_empty() {
            "5.6 Luna Max".into()
        } else {
            thread.model.clone()
        },
        reasoning: "auto".into(),
        updated_at: if thread.updated_at.is_empty() {
            "Live".into()
        } else {
            thread.updated_at.clone()
        },
        archived: thread.archived,
        pinned: false,
        entries: vec![Entry::System {
            id: format!("system-{}", thread.id),
            text: "Connected to Codex app-server. Select this task to continue.".into(),
        }],
        plan: Vec::new(),
        usage: Default::default(),
        goal: None,
        children: Vec::new(),
        queue: Vec::new(),
    }
}

fn event_thread_id(params: &Value) -> Option<&str> {
    params
        .get("threadId")
        .and_then(Value::as_str)
        .or_else(|| params.get("conversationId").and_then(Value::as_str))
        .or_else(|| {
            params
                .get("thread")
                .and_then(|thread| thread.get("id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            params
                .get("turn")
                .and_then(|turn| turn.get("threadId"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            params
                .get("item")
                .and_then(|item| item.get("threadId"))
                .and_then(Value::as_str)
        })
}

fn status_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.get("type").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| value_text(value))
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Array(values) => values
            .iter()
            .map(value_text)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("summary"))
            .or_else(|| object.get("content"))
            .map(value_text)
            .unwrap_or_else(|| value.to_string()),
    }
}

fn string_field(value: &Value, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| value.get(*name).map(value_text))
        .unwrap_or_default()
}

fn queued_inputs_from_value(value: &Value) -> Vec<QueuedInput> {
    let Some(submissions) = value
        .get("data")
        .or_else(|| value.get("queue"))
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    submissions
        .iter()
        .enumerate()
        .filter_map(|(index, submission)| {
            let id = string_field(submission, &["id", "queuedSubmissionId"]);
            let id = if id.is_empty() {
                format!("queued-{}", index + 1)
            } else {
                id
            };
            let text = queued_input_display(submission.get("input").unwrap_or(submission));
            (!text.is_empty()).then_some(QueuedInput { id, text })
        })
        .collect()
}

fn queued_input_display(value: &Value) -> String {
    let values = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(std::slice::from_ref(value));
    values
        .iter()
        .filter_map(|item| {
            let kind = item.get("type").and_then(Value::as_str).unwrap_or_default();
            let text = match kind {
                "text" => string_field(item, &["text"]),
                "mention" | "skill" => {
                    let name = string_field(item, &["name"]);
                    if name.is_empty() {
                        string_field(item, &["path"])
                    } else {
                        format!("@{name}")
                    }
                }
                "localImage" | "localAudio" => {
                    let path = string_field(item, &["path"]);
                    if path.is_empty() {
                        kind.to_owned()
                    } else {
                        format!("{kind}: {path}")
                    }
                }
                "image" | "audio" => {
                    let url = string_field(item, &["url"]);
                    if url.is_empty() {
                        kind.to_owned()
                    } else {
                        format!("{kind}: {url}")
                    }
                }
                _ => value_text(item),
            };
            (!text.is_empty()).then_some(text)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn safe_server_request_response(method: &str) -> Value {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            json!({ "decision": "decline" })
        }
        "item/permissions/requestApproval" => {
            json!({ "permissions": {}, "scope": "turn" })
        }
        "item/tool/requestUserInput" => json!({ "answers": {} }),
        "mcpServer/elicitation/request" => json!({ "action": "cancel" }),
        "item/tool/call" => json!({ "success": false, "contentItems": [] }),
        "execCommandApproval" | "applyPatchApproval" => json!({
            "decision": { "denied": { "rejection": "Declined by user" } }
        }),
        _ => json!({}),
    }
}

fn append_system_event(task: &mut Task, method: &str, params: &Value, text: &str) {
    let id = event_entry_id(method, params, task.entries.len());
    upsert_entry(
        task,
        Entry::System {
            id,
            text: text.to_owned(),
        },
    );
}

fn generic_event_entry(
    method: &str,
    params: &Value,
    item_id: &str,
    entry_index: usize,
) -> Option<Entry> {
    if method.is_empty() {
        return None;
    }
    let id = if item_id.is_empty() {
        event_entry_id(method, params, entry_index)
    } else {
        format!("event-{}-{item_id}", method.replace('/', "-"))
    };
    let label = method.replace('/', " · ");
    let detail = notification_detail(params);
    if method.contains("outputDelta") || method.contains("delta") {
        Some(Entry::Tool {
            id,
            name: label,
            status: "running".into(),
            detail,
            output: String::new(),
        })
    } else {
        Some(Entry::System {
            id,
            text: if detail.is_empty() {
                label
            } else {
                format!("{label}: {detail}")
            },
        })
    }
}

fn event_entry_id(method: &str, params: &Value, _entry_index: usize) -> String {
    let correlation = string_field(
        params,
        &[
            "itemId",
            "turnId",
            "reviewId",
            "processId",
            "processHandle",
            "watchId",
            "environmentId",
        ],
    );
    if correlation.is_empty() {
        format!(
            "event-{}-{:016x}",
            method.replace('/', "-"),
            stable_hash(&(method.to_owned() + "\0" + &params.to_string()))
        )
    } else {
        format!("event-{}-{correlation}", method.replace('/', "-"))
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn notification_detail(params: &Value) -> String {
    for name in [
        "message",
        "reason",
        "status",
        "name",
        "kind",
        "mode",
        "serverName",
        "environmentId",
        "reviewId",
        "turnId",
        "watchId",
    ] {
        if let Some(value) = params.get(name) {
            let text = value_text(value);
            if !text.is_empty() {
                return truncate_text(&text, 240);
            }
        }
    }
    String::new()
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn normalize_item_status(status: String) -> String {
    match status.as_str() {
        "inProgress" | "running" => "running".into(),
        "completed" | "complete" => "complete".into(),
        "failed" => "failed".into(),
        "declined" => "declined".into(),
        _ if status.is_empty() => "running".into(),
        _ => status,
    }
}

fn sandbox_policy_wire(mode: &str) -> &str {
    match mode {
        "danger-full-access" => "dangerFullAccess",
        "read-only" => "readOnly",
        "workspace-write" => "workspaceWrite",
        other => other,
    }
}

fn normalize_plan_status(status: String) -> String {
    match status.as_str() {
        "completed" | "complete" => "complete".into(),
        "inProgress" | "running" => "running".into(),
        _ => "pending".into(),
    }
}

fn completed_task_status(status: &str) -> &'static str {
    match status {
        "completed" | "complete" => "idle",
        "interrupted" => "idle",
        "failed" => "error",
        _ => "idle",
    }
}

fn usage_from_value(value: &Value) -> Option<crate::model::Usage> {
    let total = value.get("total").unwrap_or(value);
    let last = value.get("last").unwrap_or(total);
    let input = number_field(last, &["inputTokens", "input_tokens", "input"]);
    let output = number_field(last, &["outputTokens", "output_tokens", "output"]);
    let cached = number_field(
        last,
        &["cachedInputTokens", "cached_input_tokens", "cached"],
    );
    let context = number_field(
        value,
        &[
            "modelContextWindow",
            "totalTokens",
            "total_tokens",
            "contextTokens",
            "context",
        ],
    );
    (input > 0 || output > 0 || cached > 0 || context > 0).then_some(crate::model::Usage {
        input,
        output,
        cached,
        context,
    })
}

fn number_field(value: &Value, names: &[&str]) -> u64 {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
        .unwrap_or_default()
}

fn diff_entry_from_value(id: &str, value: &Value) -> Option<Entry> {
    let changes = value
        .get("changes")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| value.as_array().cloned());
    if let Some(changes) = changes {
        let mut additions = 0;
        let mut deletions = 0;
        let path = changes
            .first()
            .map(|change| string_field(change, &["path"]))
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| "File changes".into());
        let summary = changes
            .iter()
            .map(|change| {
                let diff = string_field(change, &["diff", "patch"]);
                for line in diff.lines() {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        additions += 1;
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        deletions += 1;
                    }
                }
                string_field(change, &["kind", "path"])
            })
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return Some(Entry::Diff {
            id: id.into(),
            path,
            additions,
            deletions,
            summary,
        });
    }
    let summary = value_text(value);
    (!summary.is_empty()).then_some(Entry::Diff {
        id: id.into(),
        path: "Working tree".into(),
        additions: summary.lines().filter(|line| line.starts_with('+')).count() as u32,
        deletions: summary.lines().filter(|line| line.starts_with('-')).count() as u32,
        summary,
    })
}

fn git_worktrees(path: &str) -> anyhow::Result<Vec<WorktreeSummary>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(path)
        .output()
        .map_err(|error| anyhow::anyhow!("start git worktree list: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git worktree list exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(parse_git_worktrees(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn create_git_worktree(repository: &str, root: &Path, branch: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(root)
        .map_err(|error| anyhow::anyhow!("create worktree root {}: {error}", root.display()))?;
    let component = branch
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let component = if component.is_empty() {
        "codex-worktree".into()
    } else {
        component
    };
    let path = root.join(component);
    if path.exists() {
        return Err(anyhow::anyhow!(
            "worktree destination already exists: {}",
            path.display()
        ));
    }
    let path_text = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("worktree destination is not valid UTF-8"))?;
    let output = Command::new("git")
        .args(["worktree", "add", "-b", branch, path_text])
        .current_dir(repository)
        .output()
        .map_err(|error| anyhow::anyhow!("start git worktree add: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git worktree add exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(path)
}

fn remove_git_worktree(repository: &str, path: &str) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(["worktree", "remove", path])
        .current_dir(repository)
        .output()
        .map_err(|error| anyhow::anyhow!("start git worktree remove: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git worktree remove exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn parse_git_worktrees(output: &str) -> Vec<WorktreeSummary> {
    let mut records = Vec::new();
    let mut current: Option<WorktreeSummary> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(WorktreeSummary {
                path: path.to_owned(),
                ..WorktreeSummary::default()
            });
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            if let Some(record) = current.as_mut() {
                record.head = head.to_owned();
            }
        } else if let Some(branch) = line.strip_prefix("branch ") {
            if let Some(record) = current.as_mut() {
                record.branch = branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned();
            }
        } else if line == "detached" {
            if let Some(record) = current.as_mut() {
                record.branch = "(detached)".into();
            }
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    for (index, record) in records.iter_mut().enumerate() {
        record.is_main = index == 0;
        if record.branch.is_empty() {
            record.branch = "(unknown)".into();
        }
    }
    records
}

fn local_review_entry(path: &str, entry_index: usize) -> anyhow::Result<Entry> {
    let output = Command::new("git")
        .args(["diff", "--no-ext-diff", "--unified=0", "--"])
        .current_dir(path)
        .output()
        .map_err(|error| anyhow::anyhow!("start git diff: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("git diff exited with {}", output.status));
    }
    let diff = String::from_utf8_lossy(&output.stdout);
    let additions = diff
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count() as u32;
    let deletions = diff
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .count() as u32;
    let path = diff
        .lines()
        .find_map(|line| line.strip_prefix("+++ b/"))
        .filter(|value| *value != "/dev/null")
        .unwrap_or("Working tree")
        .to_owned();
    let summary = if diff.trim().is_empty() {
        "No uncommitted changes".into()
    } else {
        format!("{additions} additions, {deletions} deletions")
    };
    Ok(Entry::Diff {
        id: format!("local-review-{entry_index}"),
        path,
        additions,
        deletions,
        summary,
    })
}

fn github_pull_requests(path: &str) -> anyhow::Result<(String, Vec<PullRequestSummary>)> {
    let repository = command_text(
        Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "--jq",
                ".nameWithOwner",
            ])
            .current_dir(path),
    )?;
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--limit",
            "50",
            "--json",
            "number,title,state,url,headRefName,author,reviewDecision,statusCheckRollup",
        ])
        .current_dir(path)
        .env("GH_PAGER", "cat")
        .env("GH_FORCE_TTY", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| anyhow::anyhow!("start gh pr list: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("gh pr list exited with {}", output.status));
    }
    let values: Vec<Value> = serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("decode gh PR list: {error}"))?;
    let pull_requests = values
        .iter()
        .map(|value| PullRequestSummary {
            number: value
                .get("number")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            title: string_field(value, &["title"]),
            state: string_field(value, &["state"]),
            url: string_field(value, &["url"]),
            branch: string_field(value, &["headRefName"]),
            author: value
                .get("author")
                .map(|author| string_field(author, &["login", "name"]))
                .unwrap_or_default(),
            review_decision: string_field(value, &["reviewDecision"]),
            checks: check_summary(value.get("statusCheckRollup")),
        })
        .collect();
    Ok((repository, pull_requests))
}

fn command_text(command: &mut std::process::Command) -> anyhow::Result<String> {
    let output = command
        .env("GH_PAGER", "cat")
        .env("GH_FORCE_TTY", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| anyhow::anyhow!("start GitHub CLI: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("GitHub CLI exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn check_summary(value: Option<&Value>) -> String {
    let Some(checks) = value.and_then(Value::as_array) else {
        return "No checks".into();
    };
    if checks.is_empty() {
        return "No checks".into();
    }
    let completed = checks
        .iter()
        .filter(|check| {
            matches!(
                string_field(check, &["conclusion", "state"]).as_str(),
                "SUCCESS" | "success" | "COMPLETED" | "completed"
            )
        })
        .count();
    format!("{completed}/{} checks passing", checks.len())
}

fn is_empty_thread_read_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_lowercase();
    text.contains("not materialized") || text.contains("includeturns is unavailable")
}

fn entry_from_server_item(item: &Value) -> Option<Entry> {
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let id = string_field(item, &["id"]);
    let id = if id.is_empty() {
        format!(
            "item-{item_type}-{:016x}",
            stable_hash(&(item_type.to_owned() + "\0" + &item.to_string()))
        )
    } else {
        id
    };
    match item_type {
        "userMessage" => Some(Entry::User {
            id,
            text: string_field(item, &["text", "content"]),
            time: "Live".into(),
        }),
        "agentMessage" => Some(Entry::Assistant {
            id,
            text: string_field(item, &["text"]),
            time: "Live".into(),
        }),
        "hookPrompt" => Some(Entry::Tool {
            id,
            name: "Hook prompt".into(),
            status: "complete".into(),
            detail: string_field(item, &["fragments", "text"]),
            output: String::new(),
        }),
        "plan" => Some(Entry::Reasoning {
            id,
            text: string_field(item, &["text"]),
            collapsed: false,
        }),
        "reasoning" => Some(Entry::Reasoning {
            id,
            text: string_field(item, &["summary", "content"]),
            collapsed: false,
        }),
        "commandExecution" => {
            let command = string_field(item, &["command"]);
            let cwd = string_field(item, &["cwd"]);
            let status = normalize_item_status(string_field(item, &["status"]));
            Some(Entry::Tool {
                id,
                name: if command.is_empty() {
                    "Command execution".into()
                } else {
                    command
                },
                status: status.clone(),
                detail: if cwd.is_empty() {
                    status
                } else {
                    format!("{cwd} · {status}")
                },
                output: string_field(item, &["aggregatedOutput", "output"]),
            })
        }
        "fileChange" => {
            let changes = item
                .get("changes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let path = changes
                .first()
                .map(|change| string_field(change, &["path"]))
                .unwrap_or_else(|| "File changes".into());
            let mut additions = 0;
            let mut deletions = 0;
            let summary = changes
                .iter()
                .map(|change| {
                    let diff = string_field(change, &["diff"]);
                    for line in diff.lines() {
                        if line.starts_with('+') && !line.starts_with("+++") {
                            additions += 1;
                        } else if line.starts_with('-') && !line.starts_with("---") {
                            deletions += 1;
                        }
                    }
                    string_field(change, &["kind", "path"])
                })
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            Some(Entry::Diff {
                id,
                path,
                additions,
                deletions,
                summary,
            })
        }
        "mcpToolCall"
        | "dynamicToolCall"
        | "collabToolCall"
        | "collabAgentToolCall"
        | "subAgentActivity"
        | "webSearch"
        | "sleep" => Some(Entry::Tool {
            id,
            name: item_type.into(),
            status: normalize_item_status(string_field(item, &["status"])),
            detail: string_field(
                item,
                &[
                    "tool",
                    "server",
                    "query",
                    "prompt",
                    "agentPath",
                    "kind",
                    "durationMs",
                ],
            ),
            output: string_field(item, &["result", "error", "agentsStates", "message"]),
        }),
        "imageGeneration" | "imageView" => Some(Entry::Attachment {
            id,
            name: {
                let name = string_field(item, &["savedPath", "path", "revisedPrompt"]);
                if name.is_empty() {
                    item_type.into()
                } else {
                    name
                }
            },
            attachment_kind: item_type.into(),
        }),
        "contextCompaction" | "compacted" => Some(Entry::System {
            id,
            text: "Conversation context compacted".into(),
        }),
        "enteredReviewMode" => Some(Entry::System {
            id,
            text: format!("Review started: {}", string_field(item, &["review"])),
        }),
        "exitedReviewMode" => Some(Entry::Assistant {
            id,
            text: string_field(item, &["review"]),
            time: "Live".into(),
        }),
        _ => Some(Entry::Tool {
            id,
            name: item_type.into(),
            status: normalize_item_status(string_field(item, &["status"])),
            detail: truncate_text(
                &string_field(
                    item,
                    &["message", "reason", "name", "query", "prompt", "tool"],
                ),
                320,
            ),
            output: truncate_text(&item.to_string(), 640),
        }),
    }
}

fn entry_id(entry: &Entry) -> &str {
    match entry {
        Entry::User { id, .. }
        | Entry::Assistant { id, .. }
        | Entry::Reasoning { id, .. }
        | Entry::Tool { id, .. }
        | Entry::Code { id, .. }
        | Entry::Diff { id, .. }
        | Entry::Approval { id, .. }
        | Entry::Attachment { id, .. }
        | Entry::System { id, .. } => id,
    }
}

fn upsert_entry(task: &mut Task, entry: Entry) {
    if let Some(existing) = task
        .entries
        .iter_mut()
        .find(|existing| entry_id(existing) == entry_id(&entry))
    {
        *existing = entry;
    } else {
        task.entries.push(entry);
    }
}

fn append_assistant_delta(task: &mut Task, item_id: &str, delta: &str) {
    if let Some(Entry::Assistant { text, .. }) = task
        .entries
        .iter_mut()
        .find(|entry| entry_id(entry) == item_id)
    {
        text.push_str(delta);
    } else {
        task.entries.push(Entry::Assistant {
            id: item_id.into(),
            text: delta.into(),
            time: "Live".into(),
        });
    }
}

fn append_user_delta(task: &mut Task, delta: &str) {
    if let Some(Entry::User { text, .. }) = task
        .entries
        .iter_mut()
        .rev()
        .find(|entry| matches!(entry, Entry::User { .. }))
    {
        text.push_str(delta);
    } else {
        task.entries.push(Entry::User {
            id: "realtime-user".into(),
            text: delta.into(),
            time: "Live".into(),
        });
    }
}

fn append_reasoning_delta(task: &mut Task, item_id: &str, delta: &str) {
    if let Some(Entry::Reasoning { text, .. }) = task
        .entries
        .iter_mut()
        .find(|entry| entry_id(entry) == item_id)
    {
        text.push_str(delta);
    } else {
        task.entries.push(Entry::Reasoning {
            id: item_id.into(),
            text: delta.into(),
            collapsed: false,
        });
    }
}

fn append_tool_output(task: &mut Task, item_id: &str, delta: &str) {
    if let Some(Entry::Tool { output, .. }) = task
        .entries
        .iter_mut()
        .find(|entry| entry_id(entry) == item_id)
    {
        output.push_str(delta);
    } else {
        task.entries.push(Entry::Tool {
            id: item_id.into(),
            name: "Command execution".into(),
            status: "running".into(),
            detail: String::new(),
            output: delta.into(),
        });
    }
}

fn update_tool_status(task: &mut Task, item_id: &str, status: &str) {
    if let Some(Entry::Tool {
        status: current, ..
    }) = task
        .entries
        .iter_mut()
        .find(|entry| entry_id(entry) == item_id)
    {
        *current = status.into();
    }
}

fn append_diff_delta(task: &mut Task, item_id: &str, delta: &str) {
    if let Some(Entry::Diff { summary, .. }) = task
        .entries
        .iter_mut()
        .find(|entry| entry_id(entry) == item_id)
    {
        summary.push_str(delta);
    } else {
        task.entries.push(Entry::Diff {
            id: item_id.into(),
            path: "File changes".into(),
            additions: delta.lines().filter(|line| line.starts_with('+')).count() as u32,
            deletions: delta.lines().filter(|line| line.starts_with('-')).count() as u32,
            summary: delta.into(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    None,
    Send,
}

pub fn apply_input_edit(
    draft: &mut String,
    caret: &mut usize,
    key: &str,
    key_char: Option<&str>,
    shift: bool,
    streaming: bool,
) -> InputAction {
    let mut selection_anchor = None;
    apply_input_edit_with_enter(
        draft,
        caret,
        &mut selection_anchor,
        key,
        key_char,
        shift,
        streaming,
        true,
    )
}

pub fn apply_input_edit_with_selection(
    draft: &mut String,
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
    key: &str,
    key_char: Option<&str>,
    shift: bool,
    streaming: bool,
) -> InputAction {
    apply_input_edit_with_enter(
        draft,
        caret,
        selection_anchor,
        key,
        key_char,
        shift,
        streaming,
        true,
    )
}

pub fn apply_input_edit_with_enter(
    draft: &mut String,
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
    key: &str,
    key_char: Option<&str>,
    shift: bool,
    streaming: bool,
    enter_sends: bool,
) -> InputAction {
    if streaming {
        return InputAction::None;
    }
    let len = draft.chars().count();
    *caret = (*caret).min(len);
    if let Some(anchor) = selection_anchor.as_mut() {
        *anchor = (*anchor).min(len);
    }
    match key {
        "enter" if shift => replace_selection(draft, caret, selection_anchor, "\n"),
        "enter" if enter_sends => return InputAction::Send,
        "enter" => replace_selection(draft, caret, selection_anchor, "\n"),
        "backspace" => {
            if !delete_selection(draft, caret, selection_anchor) && *caret > 0 {
                *caret -= 1;
                let byte = char_byte_index(draft, *caret);
                draft.remove(byte);
            }
        }
        "delete" => {
            if !delete_selection(draft, caret, selection_anchor) && *caret < len {
                let byte = char_byte_index(draft, *caret);
                draft.remove(byte);
            }
        }
        "space" => replace_selection(draft, caret, selection_anchor, " "),
        "tab" => replace_selection(draft, caret, selection_anchor, "\t"),
        "left" => move_caret(
            caret,
            selection_anchor,
            caret.saturating_sub(1),
            shift,
            true,
        ),
        "right" => move_caret(caret, selection_anchor, (*caret + 1).min(len), shift, false),
        "home" => move_caret(caret, selection_anchor, 0, shift, true),
        "end" => move_caret(caret, selection_anchor, len, shift, false),
        _ => {
            if let Some(chars) = key_char {
                replace_selection(draft, caret, selection_anchor, chars);
            }
        }
    }
    InputAction::None
}

fn handle_editor_shortcut(
    draft: &mut String,
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
    key: &gpui::Keystroke,
    cx: &mut Context<AppState>,
) -> bool {
    if !(key.modifiers.platform || key.modifiers.control)
        || key.modifiers.alt
        || key.modifiers.shift
    {
        return false;
    }
    match key.key.as_str() {
        "a" => {
            let len = draft.chars().count();
            *caret = len;
            *selection_anchor = (len > 0).then_some(0);
            cx.notify();
            true
        }
        "c" => {
            if let Some(text) = selected_text(draft, *caret, *selection_anchor) {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            true
        }
        "x" => {
            if let Some(text) = selected_text(draft, *caret, *selection_anchor) {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                delete_selection(draft, caret, selection_anchor);
                cx.notify();
            }
            true
        }
        "v" => {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                replace_selection(draft, caret, selection_anchor, &text);
                cx.notify();
            }
            true
        }
        _ => false,
    }
}

fn move_caret(
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
    target: usize,
    shift: bool,
    toward_start: bool,
) {
    if shift {
        let anchor = selection_anchor.unwrap_or(*caret);
        *caret = target;
        *selection_anchor = (anchor != target).then_some(anchor);
    } else if let Some(anchor) = selection_anchor.take() {
        *caret = if toward_start {
            anchor.min(*caret)
        } else {
            anchor.max(*caret)
        };
    } else {
        *caret = target;
    }
}

fn selection_bounds(caret: usize, selection_anchor: Option<usize>) -> Option<(usize, usize)> {
    let anchor = selection_anchor?;
    let start = anchor.min(caret);
    let end = anchor.max(caret);
    (start < end).then_some((start, end))
}

fn selected_text(draft: &str, caret: usize, selection_anchor: Option<usize>) -> Option<String> {
    let (start, end) = selection_bounds(caret, selection_anchor)?;
    Some(draft.chars().skip(start).take(end - start).collect())
}

fn delete_selection(
    draft: &mut String,
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
) -> bool {
    let Some((start, end)) = selection_bounds(*caret, *selection_anchor) else {
        *selection_anchor = None;
        return false;
    };
    let start_byte = char_byte_index(draft, start);
    let end_byte = char_byte_index(draft, end);
    draft.replace_range(start_byte..end_byte, "");
    *caret = start;
    *selection_anchor = None;
    true
}

fn replace_selection(
    draft: &mut String,
    caret: &mut usize,
    selection_anchor: &mut Option<usize>,
    text: &str,
) {
    if let Some((start, end)) = selection_bounds(*caret, *selection_anchor) {
        let start_byte = char_byte_index(draft, start);
        let end_byte = char_byte_index(draft, end);
        draft.replace_range(start_byte..end_byte, text);
        *caret = start + text.chars().count();
    } else {
        let byte = char_byte_index(draft, *caret);
        draft.insert_str(byte, text);
        *caret += text.chars().count();
    }
    *selection_anchor = None;
}

fn attachment_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| path.to_owned())
}

fn attachment_kind(path: &str) -> &'static str {
    if is_image_path(path) {
        "image"
    } else {
        "file"
    }
}

fn unix_timestamp_label() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn is_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("bmp" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "webp")
    )
}

fn unique_project_id(path: &str, workspace: &Workspace) -> String {
    let base = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    let base = if base.is_empty() {
        "project".to_owned()
    } else {
        base
    };
    if !workspace.projects.iter().any(|project| project.id == base) {
        return base;
    }
    for suffix in 2..=10_000 {
        let candidate = format!("{base}-{suffix}");
        if !workspace
            .projects
            .iter()
            .any(|project| project.id == candidate)
        {
            return candidate;
        }
    }
    format!("{base}-{}", workspace.projects.len() + 1)
}

fn char_byte_index(s: &str, n: usize) -> usize {
    s.char_indices()
        .nth(n)
        .map(|(index, _)| index)
        .unwrap_or(s.len())
}

pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub fn plan_progress(plan: &[PlanStep]) -> (usize, usize) {
    let complete = plan.iter().filter(|step| step.status == "complete").count();
    (complete, plan.len())
}

pub fn child_status_counts(children: &[ChildTask]) -> (usize, usize) {
    let running = children
        .iter()
        .filter(|child| child.status == "running")
        .count();
    (running, children.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_supports_unicode_and_newline_semantics() {
        let mut draft = "hé".to_string();
        let mut caret = 2;
        assert_eq!(
            apply_input_edit(&mut draft, &mut caret, "space", None, false, false),
            InputAction::None
        );
        assert_eq!(draft, "hé ");
        assert_eq!(
            apply_input_edit(&mut draft, &mut caret, "enter", None, true, false),
            InputAction::None
        );
        assert_eq!(draft, "hé \n");
        assert_eq!(
            apply_input_edit(&mut draft, &mut caret, "enter", None, false, false),
            InputAction::Send
        );
    }

    #[test]
    fn streaming_blocks_editor_mutations() {
        let mut draft = "hello".to_string();
        let mut caret = 5;
        assert_eq!(
            apply_input_edit(&mut draft, &mut caret, "x", Some("x"), false, true),
            InputAction::None
        );
        assert_eq!(draft, "hello");
    }

    #[test]
    fn selection_editing_replaces_unicode_ranges_and_collapses_on_navigation() {
        let mut draft = "héllo".to_string();
        let mut caret = draft.chars().count();
        let mut selection_anchor = Some(1);
        assert_eq!(
            apply_input_edit_with_selection(
                &mut draft,
                &mut caret,
                &mut selection_anchor,
                "x",
                Some("X"),
                false,
                false,
            ),
            InputAction::None
        );
        assert_eq!(draft, "hX");
        assert_eq!(caret, 2);
        assert_eq!(selection_anchor, None);

        selection_anchor = Some(0);
        caret = draft.chars().count();
        apply_input_edit_with_selection(
            &mut draft,
            &mut caret,
            &mut selection_anchor,
            "left",
            None,
            false,
            false,
        );
        assert_eq!(caret, 0);
        assert_eq!(selection_anchor, None);
    }

    #[test]
    fn shift_navigation_extends_and_backspace_removes_selection() {
        let mut draft = "hello".to_string();
        let mut caret = 1;
        let mut selection_anchor = None;
        apply_input_edit_with_selection(
            &mut draft,
            &mut caret,
            &mut selection_anchor,
            "right",
            None,
            true,
            false,
        );
        apply_input_edit_with_selection(
            &mut draft,
            &mut caret,
            &mut selection_anchor,
            "right",
            None,
            true,
            false,
        );
        assert_eq!(selection_anchor, Some(1));
        assert_eq!(caret, 3);
        apply_input_edit_with_selection(
            &mut draft,
            &mut caret,
            &mut selection_anchor,
            "backspace",
            None,
            false,
            false,
        );
        assert_eq!(draft, "hlo");
        assert_eq!(caret, 1);
        assert_eq!(selection_anchor, None);
    }

    #[test]
    fn formatting_and_progress_are_compact() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(1_250), "1.2k");
        assert_eq!(
            plan_progress(&[PlanStep {
                label: "a".into(),
                status: "complete".into()
            }]),
            (1, 1)
        );
    }

    #[test]
    fn editor_enter_preference_switches_between_send_and_newline() {
        let mut draft = "hello".to_string();
        let mut caret = draft.chars().count();
        let mut selection = None;
        assert_eq!(
            apply_input_edit_with_enter(
                &mut draft,
                &mut caret,
                &mut selection,
                "enter",
                None,
                false,
                false,
                false,
            ),
            InputAction::None
        );
        assert_eq!(draft, "hello\n");
        assert_eq!(
            apply_input_edit_with_enter(
                &mut draft,
                &mut caret,
                &mut selection,
                "enter",
                None,
                false,
                false,
                true,
            ),
            InputAction::Send
        );
        assert_eq!(draft, "hello\n");
    }

    #[test]
    fn queued_submission_shapes_preserve_order_and_user_input_labels() {
        let queued = queued_inputs_from_value(&json!({
            "data": [
                {
                    "id": "server-1",
                    "input": [
                        { "type": "text", "text": "first" },
                        { "type": "mention", "name": "README.md", "path": "/tmp/README.md" }
                    ]
                },
                {
                    "id": "server-2",
                    "input": [{ "type": "localImage", "path": "/tmp/example.png" }]
                }
            ]
        }));
        assert_eq!(
            queued,
            vec![
                QueuedInput {
                    id: "server-1".into(),
                    text: "first · @README.md".into(),
                },
                QueuedInput {
                    id: "server-2".into(),
                    text: "localImage: /tmp/example.png".into(),
                },
            ]
        );
    }

    #[test]
    fn queue_migration_defaults_missing_server_data_to_empty() {
        assert!(queued_inputs_from_value(&json!({ "nextCursor": null })).is_empty());
        assert_eq!(
            queued_input_display(&json!({ "type": "text", "text": "single" })),
            "single"
        );
    }

    #[test]
    fn server_catalog_extracts_models_reasoning_and_nested_capabilities() {
        let value = json!({
            "data": [
                {
                    "id": "gpt-test",
                    "displayName": "GPT Test",
                    "supportedReasoningEfforts": [
                        { "reasoningEffort": "low", "description": "fast" },
                        { "reasoningEffort": "high", "description": "deep" }
                    ]
                }
            ]
        });
        let models = model_options_from_value(&value);
        assert_eq!(models[0].id, "gpt-test");
        assert_eq!(models[0].label, "GPT Test");
        assert_eq!(models[0].reasoning, ["low", "high"]);
        let names = nested_named_values_from_data(
            &json!({ "marketplaces": [{ "name": "main", "plugins": [{ "id": "p1", "name": "Plugin" }] }] }),
            &["marketplaces", "plugins"],
            &["name", "id"],
        );
        assert!(names.contains(&"main".into()));
        assert!(names.contains(&"p1".into()));
        assert!(names.contains(&"Plugin".into()));
    }

    #[test]
    fn server_items_cover_tool_diff_and_realtime_shapes() {
        let command = entry_from_server_item(&json!({
            "id": "command-1",
            "type": "commandExecution",
            "command": "cargo test",
            "cwd": "/tmp",
            "status": "completed",
            "aggregatedOutput": "ok"
        }))
        .unwrap();
        assert!(matches!(command, Entry::Tool { status, .. } if status == "complete"));
        let diff = diff_entry_from_value("diff-1", &json!("+added\n-removed")).unwrap();
        assert!(matches!(
            diff,
            Entry::Diff {
                additions: 1,
                deletions: 1,
                ..
            }
        ));
        let collab = entry_from_server_item(&json!({
            "id": "agent-1",
            "type": "collabAgentToolCall",
            "tool": "spawnAgent",
            "status": "completed",
            "receiverThreadIds": ["child-1"],
            "senderThreadId": "parent",
            "agentsStates": {}
        }))
        .unwrap();
        assert!(matches!(collab, Entry::Tool { name, .. } if name == "collabAgentToolCall"));
    }

    #[test]
    fn every_reference_item_family_is_preserved_when_fields_evolve() {
        let item_types = [
            "userMessage",
            "agentMessage",
            "hookPrompt",
            "plan",
            "reasoning",
            "commandExecution",
            "fileChange",
            "mcpToolCall",
            "dynamicToolCall",
            "collabToolCall",
            "collabAgentToolCall",
            "subAgentActivity",
            "webSearch",
            "sleep",
            "imageGeneration",
            "imageView",
            "contextCompaction",
            "compacted",
            "enteredReviewMode",
            "exitedReviewMode",
            "futureReferenceItem",
        ];
        for (index, item_type) in item_types.iter().enumerate() {
            let entry = entry_from_server_item(&json!({
                "id": format!("item-{index}"),
                "type": item_type,
                "status": "completed",
                "text": "fixture",
                "message": "fixture",
                "command": "true",
                "cwd": "/tmp",
                "changes": [{ "path": "fixture.txt", "diff": "+a\n-b" }],
                "savedPath": "/tmp/fixture.png",
                "review": "fixture review",
            }));
            assert!(entry.is_some(), "item type was dropped: {item_type}");
        }
    }

    #[test]
    fn sandbox_policy_matches_official_wire_names() {
        assert_eq!(sandbox_policy_wire("workspace-write"), "workspaceWrite");
        assert_eq!(sandbox_policy_wire("read-only"), "readOnly");
        assert_eq!(
            sandbox_policy_wire("danger-full-access"),
            "dangerFullAccess"
        );
    }

    #[test]
    fn server_request_responses_preserve_each_official_contract() {
        let command = PendingInteraction::from_event(
            &json!({ "id": 1 }),
            &json!({
                "threadId": "thread-1",
                "itemId": "item-1",
                "command": "printf safe",
                "availableDecisions": ["accept", "decline"]
            }),
            "item/commandExecution/requestApproval",
        );
        assert_eq!(command.kind, InteractionKind::CommandApproval);
        assert_eq!(command.response(true), json!({ "decision": "accept" }));
        assert_eq!(command.response(false), json!({ "decision": "decline" }));

        let permissions = PendingInteraction::from_event(
            &json!({ "id": 2 }),
            &json!({
                "threadId": "thread-1",
                "itemId": "item-2",
                "cwd": "/tmp",
                "permissions": {
                    "fileSystem": {
                        "entries": [{
                            "access": "write",
                            "path": { "type": "path", "path": "/tmp" }
                        }]
                    }
                }
            }),
            "item/permissions/requestApproval",
        );
        assert_eq!(
            permissions.response(true)["permissions"]["fileSystem"]["entries"][0]["access"],
            "write"
        );
        assert_eq!(
            permissions.response(false),
            json!({
                "permissions": {},
                "scope": "turn"
            })
        );

        let input = PendingInteraction::from_event(
            &json!({ "id": 3 }),
            &json!({
                "threadId": "thread-1",
                "itemId": "item-3",
                "isBlocking": true,
                "questions": [{ "id": "q1", "header": "Choice", "question": "Pick one" }]
            }),
            "item/tool/requestUserInput",
        );
        assert_eq!(input.response(true), json!({ "answers": {} }));

        let mcp = PendingInteraction::from_event(
            &json!({ "id": 4 }),
            &json!({
                "threadId": "thread-1",
                "serverName": "fixture",
                "mode": "form",
                "message": "Confirm"
            }),
            "mcpServer/elicitation/request",
        );
        assert_eq!(mcp.response(true), json!({ "action": "accept" }));
        assert_eq!(mcp.response(false), json!({ "action": "cancel" }));

        let dynamic = PendingInteraction::from_event(
            &json!({ "id": 5 }),
            &json!({
                "threadId": "thread-1",
                "callId": "call-1",
                "tool": "fixture"
            }),
            "item/tool/call",
        );
        assert_eq!(
            dynamic.response(true),
            json!({ "success": false, "contentItems": [] })
        );
    }

    #[test]
    fn all_reference_notification_methods_have_a_safe_reducer_fallback() {
        let methods = [
            "account/login/completed",
            "account/rateLimits/updated",
            "account/updated",
            "app/list/updated",
            "autoApprovalReview/strictReviewRequired",
            "command/exec/outputDelta",
            "configWarning",
            "deprecationNotice",
            "error",
            "externalAgentConfig/import/completed",
            "externalAgentConfig/import/progress",
            "fs/changed",
            "fuzzyFileSearch/sessionCompleted",
            "fuzzyFileSearch/sessionUpdated",
            "guardianWarning",
            "hook/completed",
            "hook/started",
            "item/agentMessage/delta",
            "item/autoApprovalReview/completed",
            "item/autoApprovalReview/started",
            "item/commandExecution/outputDelta",
            "item/commandExecution/terminalInteraction",
            "item/fileChange/outputDelta",
            "item/fileChange/patchUpdated",
            "item/mcpToolCall/progress",
            "item/plan/delta",
            "item/reasoning/summaryPartAdded",
            "item/reasoning/summaryTextDelta",
            "item/reasoning/textDelta",
            "item/started",
            "item/completed",
            "mcpServer/oauthLogin/completed",
            "mcpServer/startupStatus/updated",
            "model/rerouted",
            "model/safetyBuffering/updated",
            "model/verification",
            "process/exited",
            "process/outputDelta",
            "project/changed",
            "remoteControl/status/changed",
            "serverRequest/resolved",
            "skills/changed",
            "thread/archived",
            "thread/closed",
            "thread/compacted",
            "thread/deleted",
            "thread/environment/connected",
            "thread/environment/disconnected",
            "thread/goal/cleared",
            "thread/goal/updated",
            "thread/name/updated",
            "thread/project/updated",
            "thread/queue/changed",
            "thread/realtime/closed",
            "thread/realtime/error",
            "thread/realtime/itemAdded",
            "thread/realtime/outputAudio/delta",
            "thread/realtime/sdp",
            "thread/realtime/started",
            "thread/realtime/transcript/delta",
            "thread/realtime/transcript/done",
            "thread/reverted",
            "thread/settings/updated",
            "thread/status/changed",
            "thread/started",
            "thread/tokenUsage/updated",
            "thread/unarchived",
            "turn/completed",
            "turn/diff/updated",
            "turn/moderationMetadata",
            "turn/plan/updated",
            "turn/started",
            "warning",
            "windowsSandbox/setupCompleted",
            "windows/worldWritableWarning",
        ];
        for method in methods {
            let entry = generic_event_entry(
                method,
                &json!({ "threadId": "thread-1", "message": "fixture" }),
                "",
                0,
            );
            assert!(entry.is_some(), "missing fallback for {method}");
        }
    }

    #[test]
    fn idless_items_use_stable_content_ids_without_collisions() {
        let first = entry_from_server_item(&json!({
            "type": "futureItem",
            "message": "first"
        }))
        .unwrap();
        let first_again = entry_from_server_item(&json!({
            "type": "futureItem",
            "message": "first"
        }))
        .unwrap();
        let second = entry_from_server_item(&json!({
            "type": "futureItem",
            "message": "second"
        }))
        .unwrap();
        assert_eq!(entry_id(&first), entry_id(&first_again));
        assert_ne!(entry_id(&first), entry_id(&second));

        let event = json!({ "message": "same" });
        assert_eq!(
            event_entry_id("future/event", &event, 0),
            event_entry_id("future/event", &event, 99)
        );
        assert_ne!(
            event_entry_id("future/event", &event, 0),
            event_entry_id("future/other", &event, 0)
        );
    }

    #[test]
    fn git_worktree_parser_preserves_paths_branches_and_main_marker() {
        let worktrees = parse_git_worktrees(
            "worktree /repo/main\nHEAD abc123\nbranch refs/heads/main\n\nworktree /repo/feature one\nHEAD def456\nbranch refs/heads/feature/one\n\nworktree /repo/detached\nHEAD 789abc\ndetached\n",
        );
        assert_eq!(worktrees.len(), 3);
        assert_eq!(worktrees[0].path, "/repo/main");
        assert_eq!(worktrees[0].head, "abc123");
        assert_eq!(worktrees[0].branch, "main");
        assert!(worktrees[0].is_main);
        assert_eq!(worktrees[1].path, "/repo/feature one");
        assert_eq!(worktrees[1].branch, "feature/one");
        assert!(!worktrees[1].is_main);
        assert_eq!(worktrees[2].branch, "(detached)");
    }

    #[test]
    fn git_worktree_create_list_and_remove_round_trip_in_temp_repository() {
        let root = std::env::temp_dir().join(format!(
            "codex-app-gpui-worktree-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repository = root.join("repository");
        let worktree_root = root.join("worktrees");
        std::fs::create_dir_all(&repository).unwrap();
        let run_git = |arguments: &[&str], cwd: &Path| {
            let output = Command::new("git")
                .args(arguments)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                arguments,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run_git(&["init", "-q"], &repository);
        std::fs::write(repository.join("fixture.txt"), "fixture\n").unwrap();
        run_git(&["add", "fixture.txt"], &repository);
        run_git(
            &[
                "-c",
                "user.name=Codex Fixture",
                "-c",
                "user.email=codex-fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
                "-q",
            ],
            &repository,
        );

        let created =
            create_git_worktree(repository.to_str().unwrap(), &worktree_root, "codex/fork")
                .unwrap();
        let listed = git_worktrees(repository.to_str().unwrap()).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|worktree| {
            worktree.path == created.to_string_lossy() && worktree.branch == "codex/fork"
        }));
        remove_git_worktree(repository.to_str().unwrap(), created.to_str().unwrap()).unwrap();
        assert_eq!(
            git_worktrees(repository.to_str().unwrap()).unwrap().len(),
            1
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn diff_path_resolution_is_contained_by_the_task_directory() {
        let root = std::env::temp_dir().join(format!(
            "codex-app-gpui-open-path-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            root.with_file_name("codex-app-gpui-open-path-sibling.txt"),
            "outside\n",
        )
        .unwrap();
        assert_eq!(
            resolve_open_path(root.to_str().unwrap(), "src/main.rs").unwrap(),
            root.join("src/main.rs").canonicalize().unwrap()
        );
        assert_eq!(
            resolve_open_path(root.to_str().unwrap(), "Working tree").unwrap(),
            root.canonicalize().unwrap()
        );
        assert!(resolve_open_path(
            root.to_str().unwrap(),
            "../codex-app-gpui-open-path-sibling.txt"
        )
        .is_err());
        let outside_link = root.join("src/outside-link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            root.with_file_name("codex-app-gpui-open-path-sibling.txt"),
            &outside_link,
        )
        .unwrap();
        #[cfg(unix)]
        assert!(resolve_open_path(root.to_str().unwrap(), "src/outside-link").is_err());
        std::fs::remove_dir_all(&root).unwrap();
        let _ = std::fs::remove_file(root.with_file_name("codex-app-gpui-open-path-sibling.txt"));
    }

    #[test]
    fn mcp_server_names_accept_only_safe_wire_keys() {
        for name in ["filesystem", "my-server_2", "A1"] {
            assert!(valid_mcp_server_name(name), "expected valid name: {name}");
        }
        for name in ["", "my server", "../server", "server.name", "mcp/one", "é"] {
            assert!(
                !valid_mcp_server_name(name),
                "expected invalid name: {name}"
            );
        }
    }
}
