//! UI state and interaction reducer for the native client.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AsyncApp, Context, FocusHandle, KeyDownEvent, PathPromptOptions, WeakEntity, Window};
use serde_json::{json, Value};

use crate::model::{
    ChildTask, Entry, PlanStep, Project, Route, Settings, SettingsPage, Task, Workspace,
};
use crate::persistence::{self, Snapshot};
use crate::protocol::{AppServerClient, ServerThread};

pub const MODEL_OPTIONS: &[&str] = &["5.6 Luna Max", "5.6 Sol", "5.5", "5.4 Mini"];
pub const REASONING_OPTIONS: &[&str] = &["auto", "low", "high", "max"];
pub const COMPOSER_MODES: &[&str] = &["Agent", "Chat", "Ask"];

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
    pub search_open: bool,
    pub toast: Option<String>,
    pub connection: ConnectionState,
    pub live_client: Option<Arc<AppServerClient>>,
    pub active_turn_id: Option<String>,
    pub pending_approval_id: Option<Value>,
    pub rename_open: bool,
    pub rename_draft: String,
    pub rename_caret: usize,
    pub rename_selection_anchor: Option<usize>,
    pub query_selection_anchor: Option<usize>,
    event_loop_started: bool,
    pub root_focus: FocusHandle,
    pub input_focus: FocusHandle,
    pub search_focus: FocusHandle,
    pub rename_focus: FocusHandle,
}

impl AppState {
    pub fn new(snapshot: Snapshot, cx: &mut Context<Self>) -> Self {
        let mut state = Self {
            workspace: snapshot.workspace,
            settings: snapshot.settings,
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
            search_open: false,
            toast: None,
            connection: ConnectionState::Demo,
            live_client: None,
            active_turn_id: None,
            pending_approval_id: None,
            rename_open: false,
            rename_draft: String::new(),
            rename_caret: 0,
            rename_selection_anchor: None,
            query_selection_anchor: None,
            event_loop_started: false,
            root_focus: cx.focus_handle(),
            input_focus: cx.focus_handle(),
            search_focus: cx.focus_handle(),
            rename_focus: cx.focus_handle(),
        };
        state.ensure_selection();
        state
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            workspace: self.workspace.clone(),
            settings: self.settings.clone(),
            selected_project: self.selected_project.clone(),
            selected_task: self.selected_task.clone(),
            sidebar_collapsed: self.sidebar_collapsed,
            show_archived: self.show_archived,
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
                    Ok((client, threads)) => {
                        this.connection = ConnectionState::Live;
                        this.live_client = Some(client);
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
            "turn/started" => {
                if let Some(task) = thread_id.and_then(|id| self.task_mut_by_id(id)) {
                    task.status = "running".into();
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
                    self.pending_approval_id = None;
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
                        persist = method == "item/completed";
                    }
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
                    }
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
                    }
                }
            }
            "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput" => {
                self.add_approval_request(&event, &params, method);
                persist = true;
            }
            "serverRequest/resolved" => {
                if params.get("requestId") == self.pending_approval_id.as_ref() {
                    self.pending_approval_id = None;
                    if let Some(task) = self.current_task_mut() {
                        for entry in &mut task.entries {
                            if let Entry::Approval { requested, .. } = entry {
                                *requested = false;
                            }
                        }
                    }
                    persist = true;
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
                        let _ = client.respond(request_id, json!({ "decision": "decline" }));
                    }
                }
            }
        }
        if persist {
            self.persist(cx);
        }
        cx.notify();
    }

    fn add_approval_request(&mut self, event: &Value, params: &Value, method: &str) {
        self.pending_approval_id = event.get("id").cloned();
        let thread_id = event_thread_id(params)
            .map(str::to_owned)
            .unwrap_or_else(|| self.selected_task.clone());
        let item_id = params
            .get("itemId")
            .and_then(Value::as_str)
            .unwrap_or("approval");
        let entry = Entry::Approval {
            id: item_id.into(),
            title: if method.contains("fileChange") {
                "Apply file changes"
            } else if method.contains("permissions") {
                "Grant additional permissions"
            } else if method.contains("requestUserInput") {
                "Codex needs input"
            } else {
                "Run command"
            }
            .into(),
            command: string_field(params, &["command", "questions", "message", "toolName"]),
            cwd: string_field(params, &["cwd", "environmentId"]),
            reason: string_field(params, &["reason", "message"]),
            requested: true,
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
            children: Vec::new(),
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
        cx.notify();
    }

    pub fn open_settings(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.settings_page = page;
        self.route = Route::Settings;
        self.menu_open = false;
        self.app_menu = None;
        self.view_open = false;
        cx.notify();
    }

    pub fn select_settings_page(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.settings_page = page;
        cx.notify();
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
        if (text.is_empty() && attachments.is_empty()) || self.streaming || self.busy {
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
                let async_cx = cx.to_async();
                cx.spawn(
                    move |this: WeakEntity<Self>, _cx: &mut AsyncApp| async move {
                        let result = async_cx
                            .background_executor()
                            .spawn(async move {
                                smol::unblock(move || {
                                    client.turn_start_with_options_and_attachments(
                                        &task_id,
                                        &text,
                                        Some(&model),
                                        Some(&effort),
                                        cwd.as_deref(),
                                        Some(&approval_policy),
                                        &attachments,
                                    )
                                })
                                .await
                            })
                            .await;
                        let _ = this.update(&mut async_cx.clone(), |this, cx| match result {
                            Ok(value) => {
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
        if self.streaming {
            return;
        }
        if let Some(task) = self.current_task_mut() {
            task.status = "running".into();
            task.entries.push(Entry::System {
                id: format!("continue-{}", task.entries.len()),
                text: "Continuing the turn".into(),
            });
        }
        self.streaming = true;
        self.notify_success("Continuing", cx);
    }

    pub fn cycle_model(&mut self, cx: &mut Context<Self>) {
        let current = self
            .current_task()
            .map(|task| task.model.clone())
            .unwrap_or_default();
        let pos = MODEL_OPTIONS
            .iter()
            .position(|model| *model == current)
            .unwrap_or(0);
        let next = MODEL_OPTIONS[(pos + 1) % MODEL_OPTIONS.len()].to_string();
        if let Some(task) = self.current_task_mut() {
            task.model = next.clone();
        }
        self.settings.default_model = next.clone();
        self.persist(cx);
        self.notify_success(&format!("Model: {next}"), cx);
    }

    pub fn cycle_reasoning(&mut self, cx: &mut Context<Self>) {
        let current = self
            .current_task()
            .map(|task| task.reasoning.clone())
            .unwrap_or_else(|| "auto".into());
        let pos = REASONING_OPTIONS
            .iter()
            .position(|level| *level == current)
            .unwrap_or(0);
        let next = REASONING_OPTIONS[(pos + 1) % REASONING_OPTIONS.len()].to_string();
        if let Some(task) = self.current_task_mut() {
            task.reasoning = next.clone();
        }
        self.settings.default_reasoning = next.clone();
        self.persist(cx);
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
        self.notify_success(&format!("Approval mode: {value}"), cx);
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
        self.notify_success(&format!("Sandbox: {value}"), cx);
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
        let link = format!("codex://thread/{}", self.selected_task);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
        self.notify_success("Thread link copied", cx);
    }

    pub fn review_current(&mut self, cx: &mut Context<Self>) {
        if self.connection != ConnectionState::Live || self.selected_project != "live-codex" {
            self.notify_success("Review is available for live Codex tasks", cx);
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
            self.notify_success("Fork is available for live Codex tasks", cx);
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
        if let Some(task) = self.current_task_mut() {
            let label = if approved { "Approved" } else { "Declined" };
            for entry in &mut task.entries {
                if let Entry::Approval { requested, .. } = entry {
                    *requested = false;
                }
            }
            task.entries.push(Entry::System {
                id: format!("approval-{}", task.entries.len()),
                text: format!("{label} by user"),
            });
        }
        if let (Some(client), Some(request_id)) =
            (self.live_client.clone(), self.pending_approval_id.take())
        {
            if let Err(error) = client.respond(
                request_id,
                json!({ "decision": if approved { "accept" } else { "decline" } }),
            ) {
                self.fail(&format!("Approval response failed: {error}"), cx);
            }
        }
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
            _ => {}
        }
        self.persist(cx);
        cx.notify();
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
        let action = apply_input_edit_with_selection(
            &mut self.draft,
            &mut self.caret,
            &mut self.selection_anchor,
            &key.key,
            key.key_char.as_deref(),
            key.modifiers.shift,
            self.streaming,
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
            (true, "a") if self.pending_approval_id.is_some() => self.approve_current(true, cx),
            (true, "d") if self.pending_approval_id.is_some() => self.approve_current(false, cx),
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
        if threads.is_empty() {
            return;
        }
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
            live.tasks.push(task_from_server(&thread));
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
            } else {
                project.tasks.insert(0, task_from_server(&thread));
            }
        }
    }
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
) -> anyhow::Result<(Arc<AppServerClient>, Vec<ServerThread>)> {
    let client = Arc::new(AppServerClient::spawn(command)?);
    client.initialize()?;
    let mut threads = client.thread_list(None)?;
    if threads.is_empty() && std::env::var_os("CODEX_APP_GPUI_CREATE_LIVE_THREAD").is_some() {
        if let Some(thread) = client
            .thread_start(cwd)?
            .get("thread")
            .and_then(ServerThread::from_value)
        {
            threads.push(thread);
        }
    }
    Ok((client, threads))
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
        archived: false,
        pinned: false,
        entries: vec![Entry::System {
            id: format!("system-{}", thread.id),
            text: "Connected to Codex app-server. Select this task to continue.".into(),
        }],
        plan: Vec::new(),
        usage: Default::default(),
        children: Vec::new(),
    }
}

fn event_thread_id(params: &Value) -> Option<&str> {
    params
        .get("threadId")
        .and_then(Value::as_str)
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
    let input = number_field(value, &["inputTokens", "input_tokens", "input"]);
    let output = number_field(value, &["outputTokens", "output_tokens", "output"]);
    let cached = number_field(
        value,
        &["cachedInputTokens", "cached_input_tokens", "cached"],
    );
    let context = number_field(
        value,
        &["totalTokens", "total_tokens", "contextTokens", "context"],
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

fn is_empty_thread_read_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_lowercase();
    text.contains("not materialized") || text.contains("includeturns is unavailable")
}

fn entry_from_server_item(item: &Value) -> Option<Entry> {
    let id = string_field(item, &["id"]);
    let item_type = item.get("type").and_then(Value::as_str)?;
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
        "mcpToolCall" | "collabToolCall" | "collabAgentToolCall" | "webSearch" => {
            Some(Entry::Tool {
                id,
                name: item_type.into(),
                status: normalize_item_status(string_field(item, &["status"])),
                detail: string_field(item, &["tool", "server", "query", "prompt"]),
                output: string_field(item, &["result", "error"]),
            })
        }
        "imageGeneration" | "imageView" => Some(Entry::Attachment {
            id,
            name: string_field(item, &["savedPath", "path", "revisedPrompt"]),
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
        _ => None,
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
    apply_input_edit_with_selection(
        draft,
        caret,
        &mut selection_anchor,
        key,
        key_char,
        shift,
        streaming,
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
        "enter" => return InputAction::Send,
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
}
