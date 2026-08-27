//! Native Codex App shell: navigation, transcript, composer, destinations,
//! and settings. The implementation keeps controls close to the reference
//! layout while making every primary affordance keyboard/click reachable.

use std::path::Path;

use gpui::*;

use crate::model::{Entry, Route, SettingsPage, Task};
use crate::state::{
    child_status_counts, format_tokens, plan_progress, AppMenu, AppState, InteractionKind,
};
use crate::theme;

pub fn root(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    theme::set_active(&state.settings.theme);
    div()
        .id("app-root")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme::bg_base())
        .text_color(theme::text())
        .relative()
        .track_focus(&state.root_focus)
        .on_key_down(
            window.listener_for(&cx.entity(), |this, event, window, cx| {
                this.handle_global_key(event, window, cx);
            }),
        )
        .child(menu_bar(state, window, cx))
        .children(
            state
                .app_menu
                .map(|menu| app_menu_popup(menu, state, window, cx)),
        )
        .child(
            div()
                .id("app-body")
                .flex()
                .flex_1()
                .min_h_0()
                .child(sidebar(state, window, cx))
                .child(main_panel(state, window, cx)),
        )
        .children(state.toast.as_ref().map(|message| {
            div()
                .id("toast")
                .absolute()
                .right(px(20.0))
                .bottom(px(20.0))
                .max_w(px(360.0))
                .bg(theme::bg_surface_2())
                .border_1()
                .border_color(theme::border())
                .rounded_lg()
                .px_3()
                .py_2()
                .text_size(rems(0.78))
                .text_color(theme::text())
                .child(message.clone())
        }))
}

fn menu_bar(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    div()
        .id("menu-bar")
        .h(px(28.0))
        .w_full()
        .flex()
        .items_center()
        .gap_4()
        .px_3()
        .bg(theme::bg_base())
        .border_b_1()
        .border_color(theme::border())
        .text_size(rems(0.74))
        .text_color(theme::text_muted())
        .children([
            top_menu_button(
                "menu-file",
                "File",
                state.app_menu == Some(AppMenu::File),
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_app_menu(AppMenu::File, cx);
                }),
            ),
            top_menu_button(
                "menu-edit",
                "Edit",
                state.app_menu == Some(AppMenu::Edit),
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_app_menu(AppMenu::Edit, cx);
                }),
            ),
            top_menu_button(
                "menu-view",
                "View",
                state.app_menu == Some(AppMenu::View),
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_app_menu(AppMenu::View, cx);
                }),
            ),
            top_menu_button(
                "menu-help",
                "Help",
                state.app_menu == Some(AppMenu::Help),
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_app_menu(AppMenu::Help, cx);
                }),
            ),
        ])
}

fn top_menu_button(
    id: &'static str,
    label: &'static str,
    active: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_1()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(if active {
            Hsla::from(theme::bg_selected())
        } else {
            gpui::transparent_black()
        })
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
        .child(label)
        .on_click(listener)
}

fn app_menu_popup(
    menu: AppMenu,
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let left = match menu {
        AppMenu::File => 8.0,
        AppMenu::Edit => 48.0,
        AppMenu::View => 88.0,
        AppMenu::Help => 132.0,
    };
    let title = match menu {
        AppMenu::File => "File",
        AppMenu::Edit => "Edit",
        AppMenu::View => "View",
        AppMenu::Help => "Help",
    };
    let actions = match menu {
        AppMenu::File => vec![
            menu_action(
                "app-file-new",
                "New chat",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.create_live_task(cx);
                }),
            ),
            menu_action(
                "app-file-settings",
                "Settings",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.open_settings(SettingsPage::General, cx);
                }),
            ),
            menu_action(
                "app-file-close",
                "Close menu",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.close_app_menu(cx);
                }),
            ),
        ],
        AppMenu::Edit => vec![
            menu_action(
                "app-edit-search",
                "Search tasks",
                window.listener_for(&cx.entity(), |this, _event, window, cx| {
                    this.toggle_search(window, cx);
                }),
            ),
            menu_action(
                "app-edit-rename",
                "Rename task",
                window.listener_for(&cx.entity(), |this, _event, window, cx| {
                    this.begin_rename(window, cx);
                }),
            ),
            menu_action(
                "app-edit-clear",
                "Clear composer",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.clear_draft(cx);
                }),
            ),
        ],
        AppMenu::View => vec![
            menu_action(
                "app-view-sidebar",
                if state.sidebar_collapsed {
                    "Expand sidebar"
                } else {
                    "Collapse sidebar"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_sidebar(cx);
                }),
            ),
            menu_action(
                "app-view-options",
                "View options",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_view_options(cx);
                }),
            ),
            menu_action(
                "app-view-archived",
                if state.show_archived {
                    "Hide archived tasks"
                } else {
                    "Show archived tasks"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_archived_visibility(cx);
                }),
            ),
        ],
        AppMenu::Help => vec![
            menu_action(
                "app-help-shortcuts",
                "Keyboard shortcuts",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.open_settings(SettingsPage::Keybindings, cx);
                }),
            ),
            menu_action(
                "app-help-about",
                "About Codex App GPUI",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.notify_success("Codex App GPUI · native GPUI client", cx);
                }),
            ),
        ],
    };
    div()
        .id("app-menu-popup")
        .absolute()
        .top(px(28.0))
        .left(px(left))
        .w(px(190.0))
        .flex()
        .flex_col()
        .gap_1()
        .p_1()
        .bg(theme::bg_surface_2())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .shadow_lg()
        .child(
            div()
                .px_2()
                .py_1()
                .text_size(rems(0.68))
                .text_color(theme::text_faint())
                .child(title),
        )
        .children(actions)
}

fn sidebar(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    if state.sidebar_collapsed {
        compact_sidebar(state, window, cx)
    } else {
        expanded_sidebar(state, window, cx)
    }
}

fn expanded_sidebar(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    div()
        .id("sidebar")
        .w(px(theme::SIDEBAR_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .bg(theme::bg_sidebar())
        .border_r_1()
        .border_color(theme::border())
        .px_2()
        .py_2()
        .gap_1()
        .child(
            div()
                .id("brand-row")
                .h(px(32.0))
                .flex()
                .items_center()
                .px_2()
                .gap_2()
                .child(
                    div()
                        .text_size(rems(0.95))
                        .text_color(theme::text())
                        .child("Codex⌄"),
                )
                .child(div().flex_1().child(""))
                .child(icon_button(
                    "sidebar-search",
                    "⌕",
                    "Search tasks",
                    window.listener_for(&cx.entity(), |this, _event, window, cx| {
                        this.toggle_search(window, cx);
                    }),
                ))
                .child(icon_button(
                    "sidebar-notifications",
                    "♧",
                    "Notifications",
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.notify_success("No new notifications", cx);
                    }),
                )),
        )
        .children(if state.search_open {
            Some(search_box(state, window, cx))
        } else {
            None
        })
        .child(
            div()
                .id("primary-navigation")
                .flex()
                .flex_col()
                .gap(px(theme::NAV_GAP))
                .children([
                    nav_item(
                        "nav-new-chat",
                        "✎",
                        "New chat",
                        false,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.create_live_task(cx);
                        }),
                    ),
                    nav_item(
                        "nav-pull-requests",
                        "⌘",
                        "Pull requests",
                        state.route == Route::PullRequests,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.set_route(Route::PullRequests, cx);
                        }),
                    ),
                    nav_item(
                        "nav-sites",
                        "⊞",
                        "Sites",
                        state.route == Route::Sites,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.set_route(Route::Sites, cx);
                        }),
                    ),
                    nav_item(
                        "nav-scheduled",
                        "◷",
                        "Scheduled",
                        state.route == Route::Scheduled,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.set_route(Route::Scheduled, cx);
                        }),
                    ),
                    nav_item(
                        "nav-plugins",
                        "◉",
                        "Plugins",
                        state.route == Route::Plugins,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.set_route(Route::Plugins, cx);
                        }),
                    ),
                ]),
        )
        .child(
            div()
                .id("projects-label")
                .mt_3()
                .px_2()
                .text_size(rems(0.72))
                .text_color(theme::text_faint())
                .child("Projects"),
        )
        .child(
            div()
                .id("project-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .gap_1()
                .children(state.workspace.projects.iter().map(|project| {
                    let project_id = project.id.clone();
                    let project_active = state.selected_project == project.id;
                    let collapsed = project.collapsed;
                    let task_rows = if collapsed {
                        Vec::new()
                    } else {
                        state
                            .visible_tasks(project)
                            .map(|task| task_row(task, project_id.clone(), state, window, cx))
                            .collect::<Vec<_>>()
                    };
                    div()
                        .id(ElementId::Name(format!("project-{}", project.id).into()))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(project_row(
                            project,
                            project_active,
                            window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                                this.toggle_project(project_id.clone(), cx);
                            }),
                        ))
                        .children(task_rows)
                }))
                .children((state.query.trim().is_empty()).then(|| recent_tasks(state, window, cx))),
        )
        .child(account_footer(state, window, cx))
}

fn compact_sidebar(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    div()
        .id("sidebar-compact")
        .w(px(theme::SIDEBAR_COLLAPSED_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .bg(theme::bg_sidebar())
        .border_r_1()
        .border_color(theme::border())
        .px_2()
        .py_2()
        .gap_1()
        .child(icon_button(
            "sidebar-expand",
            "C",
            "Expand sidebar",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.toggle_sidebar(cx);
            }),
        ))
        .child(icon_button(
            "compact-search",
            "⌕",
            "Search tasks",
            window.listener_for(&cx.entity(), |this, _event, window, cx| {
                this.toggle_search(window, cx);
            }),
        ))
        .child(icon_button(
            "compact-new-chat",
            "✎",
            "New chat",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.create_live_task(cx);
            }),
        ))
        .child(icon_button(
            "compact-pull-requests",
            "⌘",
            "Pull requests",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.set_route(Route::PullRequests, cx);
            }),
        ))
        .child(icon_button(
            "compact-sites",
            "⊞",
            "Sites",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.set_route(Route::Sites, cx);
            }),
        ))
        .child(icon_button(
            "compact-scheduled",
            "◷",
            "Scheduled",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.set_route(Route::Scheduled, cx);
            }),
        ))
        .child(icon_button(
            "compact-plugins",
            "◉",
            "Plugins",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.set_route(Route::Plugins, cx);
            }),
        ))
        .child(
            div()
                .id("compact-task-list")
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .mt_2()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(
                    state
                        .workspace
                        .all_tasks()
                        .filter(|(_, task)| state.show_archived || !task.archived)
                        .take(24)
                        .map(|(project, task)| {
                            let project_id = project.id.clone();
                            let task_id = task.id.clone();
                            let active = state.selected_task == task.id;
                            div()
                                .id(ElementId::Name(format!("compact-task-{}", task.id).into()))
                                .size_7()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(if active {
                                    Hsla::from(theme::bg_selected())
                                } else {
                                    gpui::transparent_black()
                                })
                                .child(div().size_2().rounded_full().bg(
                                    if task.status == "running" {
                                        theme::accent()
                                    } else {
                                        theme::text_faint()
                                    },
                                ))
                                .on_click(window.listener_for(
                                    &cx.entity(),
                                    move |this, _event, _window, cx| {
                                        this.select_task(project_id.clone(), task_id.clone(), cx);
                                    },
                                ))
                                .hover(|style| style.bg(theme::bg_hover()))
                        }),
                ),
        )
        .child(
            div()
                .id("compact-account-footer")
                .border_t_1()
                .border_color(theme::border())
                .pt_2()
                .child(icon_button(
                    "compact-account",
                    "MU",
                    "Account",
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.open_settings(SettingsPage::Account, cx);
                    }),
                )),
        )
}

fn search_box(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    div()
        .id("search-box")
        .flex()
        .items_center()
        .gap_2()
        .mx_1()
        .mb_1()
        .px_2()
        .py_1p5()
        .rounded_md()
        .bg(theme::bg_surface())
        .border_1()
        .border_color(theme::border())
        .child(div().text_color(theme::text_faint()).child("⌕"))
        .child(
            div()
                .id("search-input")
                .flex_1()
                .text_size(rems(0.78))
                .text_color(theme::text())
                .track_focus(&state.search_focus)
                .tab_index(0)
                .cursor_text()
                .child(if state.query.is_empty() {
                    "Search tasks…".into()
                } else {
                    state.query.clone()
                })
                .on_click(
                    window.listener_for(&cx.entity(), |this, _event, window, _cx| {
                        window.focus(&this.search_focus);
                    }),
                )
                .on_key_down(
                    window.listener_for(&cx.entity(), |this, event, window, cx| {
                        if this.handle_search_key(event, window, cx) {
                            cx.stop_propagation();
                        }
                    }),
                ),
        )
        .child(icon_button(
            "search-close",
            "×",
            "Close search",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.search_open = false;
                this.query.clear();
                cx.notify();
            }),
        ))
}

fn nav_item(
    id: impl Into<ElementId>,
    icon: &'static str,
    label: &'static str,
    active: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(31.0))
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .bg(if active {
            Hsla::from(theme::bg_selected())
        } else {
            gpui::transparent_black()
        })
        .text_color(theme::text_color(active))
        .text_size(rems(0.82))
        .child(
            div()
                .w(px(17.0))
                .text_center()
                .text_color(if active {
                    theme::text()
                } else {
                    theme::text_muted()
                })
                .child(icon),
        )
        .child(label)
        .on_click(listener)
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}

fn project_row(
    project: &crate::model::Project,
    active: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(ElementId::Name(
            format!("project-row-{}", project.id).into(),
        ))
        .h(px(28.0))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .text_size(rems(0.78))
        .text_color(if active {
            theme::text()
        } else {
            theme::text_muted()
        })
        .child(
            div()
                .text_color(theme::text_faint())
                .child(if project.collapsed { "▸" } else { "⌄" }),
        )
        .child(div().text_color(theme::text_muted()).child("▱"))
        .child(project.name.clone())
        .on_click(listener)
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}

fn task_row(
    task: &Task,
    project_id: String,
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let active = state.selected_project == project_id && state.selected_task == task.id;
    let task_id = task.id.clone();
    let status_color = if task.status == "running" {
        theme::accent()
    } else {
        theme::text_faint()
    };
    div()
        .id(ElementId::Name(format!("task-row-{}", task.id).into()))
        .min_h(px(30.0))
        .flex()
        .items_center()
        .gap_2()
        .ml_5()
        .mr_1()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .bg(if active {
            Hsla::from(theme::bg_selected())
        } else {
            gpui::transparent_black()
        })
        .text_color(if active {
            theme::text()
        } else {
            theme::text_muted()
        })
        .text_size(rems(0.76))
        .child(
            div()
                .size_2()
                .rounded_full()
                .bg(status_color)
                .flex_shrink_0(),
        )
        .child(div().flex_1().truncate().child(task.title.clone()))
        .children(
            task.pinned
                .then(|| div().text_color(theme::warning()).child("◆")),
        )
        .children((task.status == "running").then(|| div().text_color(theme::accent()).child("•")))
        .on_click(
            window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                this.select_task(project_id.clone(), task_id.clone(), cx);
            }),
        )
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}

fn recent_tasks(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let rows = state
        .workspace
        .all_tasks()
        .filter(|(_, task)| {
            task.project_id != state.selected_project && (state.show_archived || !task.archived)
        })
        .take(18)
        .map(|(project, task)| task_row(task, project.id.clone(), state, window, cx))
        .collect::<Vec<_>>();
    div()
        .id("recents")
        .mt_3()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .px_2()
                .text_size(rems(0.72))
                .text_color(theme::text_faint())
                .child("Recents"),
        )
        .children(rows)
}

fn account_footer(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    div()
        .id("account-footer")
        .h(px(42.0))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .border_t_1()
        .border_color(theme::border())
        .text_size(rems(0.8))
        .child(
            div()
                .size_6()
                .rounded_full()
                .bg(theme::accent_soft())
                .flex()
                .items_center()
                .justify_center()
                .text_size(rems(0.62))
                .text_color(theme::text())
                .child("MU"),
        )
        .child(
            div()
                .flex_1()
                .text_color(theme::text_muted())
                .child("mustbearnold"),
        )
        .child(icon_button(
            "account-help",
            "?",
            "Help",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.notify_success("Codex App GPUI help", cx);
            }),
        ))
        .children(
            (state.connection != crate::state::ConnectionState::Demo).then(|| {
                div()
                    .text_size(rems(0.6))
                    .text_color(theme::success())
                    .child("●")
            }),
        )
}

fn main_panel(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    div()
        .id("main-panel")
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .bg(theme::bg_base())
        .child(thread_header(state, window, cx))
        .child(match state.route {
            Route::Task => thread_view(state, window, cx),
            Route::PullRequests => pull_requests_view(state, window, cx),
            Route::Sites => sites_view(state, window, cx),
            Route::Scheduled => scheduled_view(state, window, cx),
            Route::Plugins => plugins_view(state, window, cx),
            Route::Settings => settings_view(state, window, cx),
        })
}

fn thread_header(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let (eyebrow, title) = match state.route {
        Route::Settings => ("Codex".to_string(), "Settings".to_string()),
        Route::Task => (
            state
                .current_project()
                .map(|project| project.name.clone())
                .unwrap_or_else(|| "Codex".into()),
            state
                .current_task()
                .map(|task| task.title.clone())
                .unwrap_or_else(|| "New task".into()),
        ),
        Route::PullRequests => ("Codex".into(), "Pull requests".into()),
        Route::Sites => ("Codex".into(), "Sites".into()),
        Route::Scheduled => ("Codex".into(), "Scheduled".into()),
        Route::Plugins => ("Codex".into(), "Plugins".into()),
    };
    let title_view = if state.rename_open {
        div()
            .id("rename-input")
            .min_w(px(240.0))
            .px_2()
            .py_1()
            .rounded_md()
            .bg(theme::bg_surface())
            .border_1()
            .border_color(theme::accent())
            .track_focus(&state.rename_focus)
            .tab_index(0)
            .cursor_text()
            .text_size(rems(0.82))
            .text_color(theme::text())
            .child(render_with_caret(&state.rename_draft, state.rename_caret))
            .on_click(
                window.listener_for(&cx.entity(), |this, _event, window, _cx| {
                    window.focus(&this.rename_focus);
                }),
            )
            .on_key_down(
                window.listener_for(&cx.entity(), |this, event, window, cx| {
                    this.handle_rename_key(event, window, cx);
                }),
            )
    } else {
        div()
            .id("title-view")
            .flex()
            .flex_col()
            .gap_0p5()
            .child(
                div()
                    .text_size(rems(0.72))
                    .text_color(theme::text_faint())
                    .child(eyebrow),
            )
            .child(
                div()
                    .text_size(rems(0.82))
                    .text_color(theme::text())
                    .truncate()
                    .child(title),
            )
    };
    div()
        .id("thread-header")
        .h(px(48.0))
        .flex()
        .items_center()
        .gap_2()
        .px_4()
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .text_color(theme::text_muted())
                .text_size(rems(0.8))
                .child("▱"),
        )
        .child(title_view)
        .child(div().flex_1().child(""))
        .children((state.route == Route::Task).then(|| {
            div()
                .text_size(rems(0.7))
                .text_color(theme::text_faint())
                .child(state.connection.label())
        }))
        .child(text_button(
            "header-share",
            "↗ Share",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.share_current(cx);
            }),
        ))
        .child(icon_button(
            "header-view",
            "☷",
            "View options",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.toggle_view_options(cx);
            }),
        ))
        .child(icon_button(
            "header-menu",
            "…",
            "More actions",
            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                this.toggle_menu(cx);
            }),
        ))
        .children(state.view_open.then(|| view_menu(state, window, cx)))
        .children(state.menu_open.then(|| header_menu(state, window, cx)))
}

fn view_menu(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    div()
        .id("view-menu-popover")
        .absolute()
        .top(px(42.0))
        .right(px(48.0))
        .w(px(210.0))
        .flex()
        .flex_col()
        .gap_1()
        .p_1()
        .bg(theme::bg_surface_2())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .shadow_lg()
        .children([
            menu_action(
                "view-toggle-sidebar",
                if state.sidebar_collapsed {
                    "Expand sidebar"
                } else {
                    "Collapse sidebar"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_sidebar(cx);
                }),
            ),
            menu_action(
                "view-search",
                "Search tasks",
                window.listener_for(&cx.entity(), |this, _event, window, cx| {
                    this.toggle_search(window, cx);
                }),
            ),
            menu_action(
                "view-toggle-archived",
                if state.show_archived {
                    "Hide archived tasks"
                } else {
                    "Show archived tasks"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_archived_visibility(cx);
                }),
            ),
            menu_action(
                "view-reset",
                "Reset view",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    if this.sidebar_collapsed {
                        this.sidebar_collapsed = false;
                    }
                    this.view_open = false;
                    cx.notify();
                }),
            ),
        ])
}

fn header_menu(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    div()
        .id("header-menu-popover")
        .absolute()
        .top(px(42.0))
        .right(px(12.0))
        .w(px(210.0))
        .flex()
        .flex_col()
        .gap_1()
        .p_1()
        .bg(theme::bg_surface_2())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .shadow_lg()
        .children([
            menu_action(
                "menu-rename",
                "Rename task",
                window.listener_for(&cx.entity(), |this, _event, window, cx| {
                    this.begin_rename(window, cx);
                }),
            ),
            menu_action(
                "menu-fork",
                "Fork task",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.fork_current(cx);
                }),
            ),
            menu_action(
                "menu-review",
                "Review changes",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.review_current(cx);
                }),
            ),
            menu_action(
                "menu-compact",
                "Compact context",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.compact_current(cx);
                }),
            ),
            menu_action(
                "menu-retry",
                "Retry last turn",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.retry_current(cx);
                }),
            ),
            menu_action(
                "menu-pin",
                if state
                    .current_task()
                    .map(|task| task.pinned)
                    .unwrap_or(false)
                {
                    "Unpin task"
                } else {
                    "Pin task"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.toggle_pin_current(cx);
                }),
            ),
            menu_action(
                "menu-archive",
                if state
                    .current_task()
                    .map(|task| task.archived)
                    .unwrap_or(false)
                {
                    "Unarchive task"
                } else {
                    "Archive task"
                },
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    if this
                        .current_task()
                        .map(|task| task.archived)
                        .unwrap_or(false)
                    {
                        this.unarchive_current(cx);
                    } else {
                        this.archive_current(cx);
                    }
                }),
            ),
            menu_action(
                "menu-delete",
                "Delete task",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.delete_current(cx);
                }),
            ),
            menu_action(
                "menu-settings",
                "Open settings",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.open_settings(SettingsPage::General, cx);
                }),
            ),
        ])
        .children(
            state
                .current_task()
                .filter(|task| task.status == "closed")
                .map(|_| {
                    menu_action(
                        "menu-resume",
                        "Resume task",
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.resume_current(cx);
                        }),
                    )
                }),
        )
        .children((state.route == Route::Task && state.streaming).then(|| {
            menu_action(
                "menu-stop",
                "Stop turn",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.stop_turn(cx);
                }),
            )
        }))
        .children((state.route == Route::Task && !state.streaming).then(|| {
            menu_action(
                "menu-continue",
                "Continue / retry turn",
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.retry_current(cx);
                }),
            )
        }))
}

fn thread_view(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    let task = state.current_task();
    div()
        .id("thread-view")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id("transcript-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    div()
                        .id("transcript-content")
                        .w_full()
                        .max_w(px(theme::CONTENT_MAX_WIDTH))
                        .mx_auto()
                        .px_8()
                        .pt_6()
                        .pb_4()
                        .flex()
                        .flex_col()
                        .gap_5()
                        .children(task.map(|task| thread_entries(task, state, window, cx)))
                        .children((task.is_some() && state.streaming).then(|| streaming_status()))
                        .child(div().h(px(10.0)).child("")),
                ),
        )
        .child(composer(state, window, cx))
}

fn thread_entries(
    task: &Task,
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let (complete, total) = plan_progress(&task.plan);
    let (running_children, total_children) = child_status_counts(&task.children);
    div()
        .id("thread-entries")
        .flex()
        .flex_col()
        .gap_5()
        .children((!task.plan.is_empty()).then(|| {
            div()
                .id("plan-progress")
                .flex()
                .items_center()
                .gap_2()
                .bg(theme::bg_surface())
                .border_1()
                .border_color(theme::border())
                .rounded_full()
                .px_3()
                .py_1p5()
                .text_size(rems(0.72))
                .text_color(theme::text_muted())
                .child(format!("Step {} / {}", (complete + 1).min(total), total))
        }))
        .children((total_children > 0).then(|| {
            div()
                .id("child-task-summary")
                .flex()
                .items_center()
                .gap_2()
                .text_size(rems(0.72))
                .text_color(theme::text_faint())
                .child(format!(
                    "{} subtask{} · {} active",
                    total_children,
                    if total_children == 1 { "" } else { "s" },
                    running_children
                ))
        }))
        .children(task.goal.as_ref().map(|goal| {
            div()
                .id("thread-goal")
                .bg(theme::accent_soft())
                .border_1()
                .border_color(theme::border())
                .rounded_lg()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .text_size(rems(0.72))
                .child(div().text_color(theme::accent()).child("Goal"))
                .child(
                    div()
                        .flex_1()
                        .text_color(theme::text())
                        .child(goal.objective.clone()),
                )
                .child(
                    div()
                        .text_color(theme::text_faint())
                        .child(if goal.status.is_empty() {
                            "active".to_string()
                        } else {
                            goal.status.clone()
                        }),
                )
        }))
        .children(
            task.entries
                .iter()
                .map(|entry| entry_view(entry, state, window, cx)),
        )
}

fn entry_view(
    entry: &Entry,
    _state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    match entry {
        Entry::User { id, text, time } => div()
            .id(ElementId::Name(format!("entry-user-{id}").into()))
            .flex()
            .justify_end()
            .child(
                div()
                    .max_w(DefiniteLength::Fraction(0.78))
                    .bg(theme::user_bubble())
                    .rounded_xl()
                    .px_4()
                    .py_3()
                    .text_size(rems(0.86))
                    .text_color(theme::text())
                    .whitespace_normal()
                    .child(text.clone())
                    .child(
                        div()
                            .mt_2()
                            .text_size(rems(0.64))
                            .text_color(theme::text_faint())
                            .child(time.clone()),
                    ),
            ),
        Entry::Assistant { id, text, time } => div()
            .id(ElementId::Name(format!("entry-assistant-{id}").into()))
            .flex()
            .flex_col()
            .gap_2()
            .max_w(px(760.0))
            .child(
                div()
                    .text_size(rems(0.72))
                    .text_color(theme::text_faint())
                    .child("Codex"),
            )
            .child(
                div()
                    .text_size(rems(0.9))
                    .text_color(theme::text())
                    .whitespace_normal()
                    .child(text.clone()),
            )
            .child(
                div()
                    .text_size(rems(0.64))
                    .text_color(theme::text_faint())
                    .child(time.clone()),
            ),
        Entry::Reasoning {
            id,
            text,
            collapsed,
        } => {
            let reasoning_id = id.clone();
            let mut view = div()
                .id(ElementId::Name(format!("entry-reasoning-{id}").into()))
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .text_size(rems(0.74))
                .text_color(theme::text_faint())
                .child(if *collapsed {
                    "▸ reasoning"
                } else {
                    "▾ reasoning"
                });
            if !*collapsed {
                view = view.child(text.clone());
            }
            view.on_click(
                window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                    this.toggle_reasoning(reasoning_id.clone(), cx);
                }),
            )
        }
        Entry::Tool {
            id,
            name,
            status,
            detail,
            output,
        } => div()
            .id(ElementId::Name(format!("entry-tool-{id}").into()))
            .bg(theme::bg_surface())
            .border_1()
            .border_color(theme::border())
            .rounded_lg()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::border())
                    .text_size(rems(0.75))
                    .child(div().text_color(theme::accent()).child("⌁"))
                    .child(
                        div()
                            .flex_1()
                            .text_color(theme::text_muted())
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_color(if status == "complete" {
                                theme::success()
                            } else {
                                theme::accent()
                            })
                            .child(status.clone()),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(rems(0.78))
                    .text_color(theme::text_muted())
                    .child(detail.clone()),
            )
            .children((!output.is_empty()).then(|| {
                div()
                    .px_3()
                    .pb_3()
                    .text_size(rems(0.72))
                    .text_color(theme::text_faint())
                    .child(output.clone())
            })),
        Entry::Code {
            id,
            language,
            code,
            output,
            exit_code,
        } => div()
            .id(ElementId::Name(format!("entry-code-{id}").into()))
            .bg(theme::code_bg())
            .border_1()
            .border_color(theme::border())
            .rounded_lg()
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(rems(0.72))
                    .text_color(theme::text_faint())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(language.clone()),
            )
            .child(
                div()
                    .px_3()
                    .py_3()
                    .text_size(rems(0.75))
                    .text_color(theme::text_muted())
                    .whitespace_normal()
                    .child(code.clone()),
            )
            .children((!output.is_empty()).then(|| {
                div()
                    .px_3()
                    .py_2()
                    .text_size(rems(0.72))
                    .text_color(theme::text())
                    .child(output.clone())
            }))
            .children(exit_code.map(|code| {
                div()
                    .px_3()
                    .pb_3()
                    .text_size(rems(0.68))
                    .text_color(if code == 0 {
                        theme::success()
                    } else {
                        theme::danger()
                    })
                    .child(format!("exit {code}"))
            })),
        Entry::Diff {
            id,
            path,
            additions,
            deletions,
            summary,
        } => div()
            .id(ElementId::Name(format!("entry-diff-{id}").into()))
            .cursor_pointer()
            .bg(theme::bg_surface())
            .border_1()
            .border_color(theme::border())
            .rounded_lg()
            .px_3()
            .py_2()
            .text_size(rems(0.76))
            .child(div().text_color(theme::text()).child(path.clone()))
            .child(
                div()
                    .mt_1()
                    .text_color(theme::text_muted())
                    .child(summary.clone()),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(theme::success())
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .text_color(theme::danger())
                    .child(format!("−{deletions}")),
            )
            .on_click({
                let path = path.clone();
                window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                    this.copy_diff_path(path.clone(), cx);
                })
            }),
        Entry::Approval {
            id,
            title,
            command,
            cwd,
            reason,
            requested,
            approval_kind,
            choices,
            request_details,
        } => {
            let requested = *requested;
            let approval_id = id.clone();
            let interactive =
                InteractionKind::from_method(approval_kind).can_render_decision_buttons();
            let allow_label = if approval_kind == "item/permissions/requestApproval" {
                "Grant"
            } else if approval_kind == "mcpServer/elicitation/request" {
                "Accept"
            } else {
                "Allow"
            };
            let deny_label = if approval_kind == "item/tool/requestUserInput"
                || approval_kind == "mcpServer/elicitation/request"
            {
                "Cancel"
            } else {
                "Deny"
            };
            div()
                .id(ElementId::Name(format!("entry-approval-{id}").into()))
                .bg(theme::bg_surface())
                .border_1()
                .border_color(if requested {
                    theme::warning()
                } else {
                    theme::border()
                })
                .rounded_lg()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_color(theme::warning())
                        .text_size(rems(0.78))
                        .child(if requested {
                            "Approval required"
                        } else {
                            "Approval resolved"
                        }),
                )
                .child(
                    div()
                        .text_color(theme::text())
                        .text_size(rems(0.84))
                        .child(title.clone()),
                )
                .child(
                    div()
                        .bg(theme::code_bg())
                        .rounded_md()
                        .px_2()
                        .py_2()
                        .text_size(rems(0.72))
                        .text_color(theme::text_muted())
                        .child(command.clone()),
                )
                .child(
                    div()
                        .text_size(rems(0.68))
                        .text_color(theme::text_faint())
                        .child(format!("{} · {}", cwd, reason)),
                )
                .children((!request_details.is_empty()).then(|| {
                    div()
                        .text_size(rems(0.7))
                        .text_color(theme::text_faint())
                        .child(request_details.clone())
                }))
                .children((!choices.is_empty()).then(|| {
                    div().flex().gap_1().children(choices.iter().map(|choice| {
                        div()
                            .bg(theme::bg_hover())
                            .rounded_sm()
                            .px_2()
                            .py_1()
                            .text_size(rems(0.64))
                            .text_color(theme::text_muted())
                            .child(choice.clone())
                    }))
                }))
                .children((requested && interactive).then(|| {
                    div()
                        .flex()
                        .gap_2()
                        .child(text_button(
                            ElementId::Name(format!("approval-allow-{approval_id}").into()),
                            allow_label,
                            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                                this.approve_current(true, cx)
                            }),
                        ))
                        .child(text_button(
                            ElementId::Name(format!("approval-deny-{approval_id}").into()),
                            deny_label,
                            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                                this.approve_current(false, cx)
                            }),
                        ))
                }))
        }
        Entry::Attachment {
            id,
            name,
            attachment_kind,
        } => div()
            .id(ElementId::Name(format!("entry-attachment-{id}").into()))
            .flex()
            .items_center()
            .gap_2()
            .bg(theme::bg_surface())
            .border_1()
            .border_color(theme::border())
            .rounded_md()
            .px_3()
            .py_2()
            .text_size(rems(0.76))
            .child(div().text_color(theme::warning()).child("▧"))
            .child(name.clone())
            .child(
                div()
                    .text_color(theme::text_faint())
                    .child(attachment_kind.clone()),
            ),
        Entry::System { id, text } => div()
            .id(ElementId::Name(format!("entry-system-{id}").into()))
            .w_full()
            .text_center()
            .text_size(rems(0.72))
            .text_color(theme::text_faint())
            .child(text.clone()),
    }
}

fn streaming_status() -> Stateful<Div> {
    div()
        .id("streaming-status")
        .flex()
        .items_center()
        .gap_2()
        .text_size(rems(0.8))
        .text_color(theme::text_muted())
        .child(div().text_color(theme::accent()).child("◌"))
        .child("Working…")
}

fn composer(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    let task = state.current_task();
    let running = state.streaming || state.busy;
    let model_id = task
        .map(|task| task.model.clone())
        .unwrap_or_else(|| state.settings.default_model.clone());
    let model = state.model_label(&model_id);
    let reasoning = task
        .map(|task| task.reasoning.clone())
        .unwrap_or_else(|| state.settings.default_reasoning.clone());
    let usage = task.map(|task| task.usage).unwrap_or_default();
    let placeholder = if task.is_none() {
        "Select a task to start…"
    } else if running {
        "Codex is working…"
    } else {
        "Do anything"
    };

    div().id("composer").w_full().px_8().pb_5().child(
        div()
            .w_full()
            .max_w(px(theme::COMPOSER_MAX_WIDTH))
            .mx_auto()
            .bg(theme::bg_surface())
            .border_1()
            .border_color(theme::border())
            .rounded_xl()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .children((!state.attachments.is_empty()).then(|| {
                div()
                    .flex()
                    .gap_1()
                    .children(state.attachments.iter().enumerate().map(|(index, name)| {
                        let attachment_index = index;
                        let label = Path::new(name)
                            .file_name()
                            .and_then(|file_name| file_name.to_str())
                            .filter(|file_name| !file_name.is_empty())
                            .unwrap_or(name);
                        div()
                            .id(ElementId::Name(format!("attachment-pill-{index}").into()))
                            .bg(theme::accent_soft())
                            .rounded_md()
                            .px_2()
                            .py_1()
                            .text_size(rems(0.68))
                            .text_color(theme::text())
                            .child(format!("▧ {label}"))
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("attachment-remove-{index}").into(),
                                    ))
                                    .px_1()
                                    .cursor_pointer()
                                    .text_color(theme::text_faint())
                                    .child("×")
                                    .on_click(window.listener_for(
                                        &cx.entity(),
                                        move |this, _event, _window, cx| {
                                            this.remove_attachment(attachment_index, cx);
                                        },
                                    ))
                                    .hover(|style| style.text_color(theme::text())),
                            )
                    }))
            }))
            .child(
                div()
                    .id("composer-input")
                    .min_h(px(62.0))
                    .max_h(px(220.0))
                    .w_full()
                    .px_2()
                    .py_2()
                    .cursor_text()
                    .track_focus(&state.input_focus)
                    .tab_index(0)
                    .text_size(rems(0.88))
                    .text_color(theme::text())
                    .whitespace_normal()
                    .child(if state.draft.is_empty() {
                        placeholder.to_string()
                    } else {
                        render_with_caret(&state.draft, state.caret)
                    })
                    .on_click(
                        window.listener_for(&cx.entity(), |this, _event, window, _cx| {
                            window.focus(&this.input_focus);
                        }),
                    )
                    .on_key_down(
                        window.listener_for(&cx.entity(), |this, event, window, cx| {
                            if this.handle_input_key(event, window, cx) {
                                cx.stop_propagation();
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(icon_button(
                        "composer-attach",
                        "+",
                        "Attach file",
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.pick_attachments(cx)
                        }),
                    ))
                    .child(icon_button(
                        "composer-mention",
                        "@",
                        "Mention file",
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.insert_mention(cx)
                        }),
                    ))
                    .child(text_button(
                        "composer-mode",
                        &state.composer_mode,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.cycle_mode(cx)
                        }),
                    ))
                    .child(text_button(
                        "composer-sandbox",
                        &format!("sandbox · {}", state.settings.sandbox_mode),
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.cycle_sandbox_mode(cx)
                        }),
                    ))
                    .child(text_button(
                        "composer-approval",
                        &format!("approval · {}", state.settings.approval_mode),
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.cycle_approval_mode(cx)
                        }),
                    ))
                    .child(text_button(
                        "composer-tools",
                        "tools",
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.open_settings(SettingsPage::Apps, cx)
                        }),
                    ))
                    .child(div().flex_1().child(""))
                    .child(
                        div()
                            .text_size(rems(0.66))
                            .text_color(theme::text_faint())
                            .child(format!(
                                "↑{} ↓{}{}",
                                format_tokens(usage.input),
                                format_tokens(usage.output),
                                usage
                                    .cache_rate()
                                    .map(|rate| format!(" · cache {rate}%"))
                                    .unwrap_or_default(),
                            )),
                    )
                    .child(text_button(
                        "composer-model",
                        &model,
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.cycle_model(cx)
                        }),
                    ))
                    .child(text_button(
                        "composer-reasoning",
                        &format!("reasoning · {reasoning}"),
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.cycle_reasoning(cx)
                        }),
                    ))
                    .child(icon_button(
                        "composer-mic",
                        if state.voice_active { "■" } else { "♩" },
                        if state.voice_active {
                            "Stop voice input"
                        } else {
                            "Voice input"
                        },
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.toggle_voice(cx)
                        }),
                    ))
                    .children(if running {
                        Some(text_button(
                            "composer-stop",
                            "■",
                            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                                this.stop_turn(cx)
                            }),
                        ))
                    } else {
                        Some(text_button(
                            "composer-send",
                            "↑",
                            window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                                this.send(cx)
                            }),
                        ))
                    }),
            )
            .child(
                div()
                    .text_size(rems(0.64))
                    .text_color(theme::text_faint())
                    .child("Enter to send · Shift+Enter for newline"),
            ),
    )
}

fn render_with_caret(text: &str, caret: usize) -> String {
    let mut chars = text.chars();
    let head = chars.by_ref().take(caret).collect::<String>();
    let tail = chars.collect::<String>();
    format!("{head}▍{tail}")
}

fn destination_view(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    cards: Vec<Stateful<Div>>,
) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("destination-{id}").into()))
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(theme::CONTENT_MAX_WIDTH))
                .mx_auto()
                .px_8()
                .py_8()
                .flex()
                .flex_col()
                .gap_5()
                .child(
                    div()
                        .text_size(rems(1.3))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(rems(0.84))
                        .text_color(theme::text_muted())
                        .child(description),
                )
                .children(cards),
        )
}

fn destination_card(
    id: String,
    title: String,
    detail: String,
    action: Option<(String, Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>)>,
) -> Stateful<Div> {
    let card = div()
        .id(ElementId::Name(format!("destination-card-{id}").into()))
        .bg(theme::bg_surface())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .px_4()
        .py_4()
        .flex()
        .items_center()
        .gap_3()
        .text_size(rems(0.84))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_color(theme::text()).child(title))
                .child(div().text_color(theme::text_muted()).child(detail)),
        );
    if let Some((label, listener)) = action {
        card.child(text_button(
            ElementId::Name(format!("destination-action-{id}").into()),
            &label,
            listener,
        ))
    } else {
        card
    }
}

fn empty_destination_card(
    id: impl Into<String>,
    message: impl Into<String>,
    action: Option<(String, Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>)>,
) -> Stateful<Div> {
    destination_card(
        id.into(),
        message.into(),
        "Nothing is configured yet".into(),
        action,
    )
}

fn pull_requests_view(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let mut cards = vec![destination_card(
        "github-status".into(),
        if state.github.repository.is_empty() {
            "GitHub pull request inbox".into()
        } else {
            state.github.repository.clone()
        },
        state.github.status.clone(),
        Some((
            "Refresh".into(),
            Box::new(
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.refresh_pull_requests(cx);
                }),
            ),
        )),
    )];
    for pull_request in &state.github.pull_requests {
        let url = pull_request.url.clone();
        cards.push(destination_card(
            format!("github-pr-{}", pull_request.number),
            format!("#{} {}", pull_request.number, pull_request.title),
            format!(
                "{} · {} · {} · {}",
                pull_request.state,
                pull_request.branch,
                if pull_request.author.is_empty() {
                    "unknown author"
                } else {
                    &pull_request.author
                },
                pull_request.checks
            ),
            (!url.is_empty()).then(|| {
                (
                    "Copy link".into(),
                    Box::new(
                        window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                            this.copy_link(url.clone(), cx);
                        }),
                    ) as Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
                )
            }),
        ));
    }
    for project in &state.workspace.projects {
        for task in &project.tasks {
            if task.archived || task.branch.is_none() {
                continue;
            }
            let project_id = project.id.clone();
            let task_id = task.id.clone();
            let title = task.title.clone();
            let branch = task.branch.clone().unwrap_or_default();
            cards.push(destination_card(
                format!("pr-{}", task.id),
                title,
                format!("{} · {}", project.name, branch),
                Some((
                    "Open task".into(),
                    Box::new(window.listener_for(
                        &cx.entity(),
                        move |this, _event, _window, cx| {
                            this.select_task(project_id.clone(), task_id.clone(), cx);
                        },
                    )),
                )),
            ));
        }
    }
    if cards.is_empty() {
        cards.push(empty_destination_card(
            "pr-empty",
            "No pull requests need attention",
            Some((
                "Review current task".into(),
                Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.set_route(Route::Task, cx);
                        this.review_current(cx);
                    }),
                ),
            )),
        ));
    }
    destination_view(
        "pull-requests",
        "Pull requests",
        "Review branches and change requests from your projects",
        cards,
    )
}

fn sites_view(state: &AppState, window: &mut Window, cx: &mut Context<AppState>) -> Stateful<Div> {
    let mut cards = state
        .catalog
        .apps
        .iter()
        .enumerate()
        .map(|(index, app)| {
            empty_destination_card(
                format!("site-available-{index}"),
                "Connected app surface",
                Some((
                    app.clone(),
                    Box::new(
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.open_settings(SettingsPage::Apps, cx);
                        }),
                    ),
                )),
            )
        })
        .collect::<Vec<_>>();
    if cards.is_empty() {
        cards.push(empty_destination_card(
            "sites-empty",
            "No sites configured yet",
            Some((
                "Configure apps".into(),
                Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.open_settings(SettingsPage::Apps, cx);
                    }),
                ),
            )),
        ));
    }
    destination_view(
        "sites",
        "Sites",
        "Open connected sites and hosted project surfaces",
        cards,
    )
}

fn scheduled_view(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let mut cards = state
        .workspace
        .automations
        .iter()
        .map(|automation| {
            let automation_id = automation.id.clone();
            let action_label = if automation.status == "active" {
                "Pause"
            } else {
                "Resume"
            };
            destination_card(
                format!("scheduled-{}", automation.id),
                automation.name.clone(),
                format!(
                    "{} · {} · next run {}",
                    automation.schedule, automation.status, automation.next_run
                ),
                Some((
                    action_label.into(),
                    Box::new(window.listener_for(
                        &cx.entity(),
                        move |this, _event, _window, cx| {
                            this.toggle_automation(automation_id.clone(), cx);
                        },
                    )),
                )),
            )
        })
        .collect::<Vec<_>>();
    if cards.is_empty() {
        cards.push(empty_destination_card(
            "scheduled-empty",
            "No scheduled tasks",
            Some((
                "Create automation".into(),
                Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.create_automation(cx);
                    }),
                ),
            )),
        ));
    }
    cards.push(destination_card(
        "scheduled-create".into(),
        "Create from current task".into(),
        "Save a local recurring prompt and attach it to the selected task".into(),
        Some((
            "Create".into(),
            Box::new(
                window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                    this.create_automation(cx);
                }),
            ),
        )),
    ));
    destination_view(
        "scheduled",
        "Scheduled",
        "Automations and recurring Codex tasks",
        cards,
    )
}

fn plugins_view(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let mut cards = state
        .catalog
        .plugins
        .iter()
        .map(|plugin| {
            destination_card(
                format!("plugin-{}", plugin),
                plugin.clone(),
                "Installed by the local app-server".into(),
                Some((
                    "Manage".into(),
                    Box::new(
                        window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                            this.open_settings(SettingsPage::Plugins, cx);
                        }),
                    ),
                )),
            )
        })
        .collect::<Vec<_>>();
    if cards.is_empty() {
        cards.push(empty_destination_card(
            "plugins-empty",
            "No plugins installed",
            Some((
                "Open settings".into(),
                Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.open_settings(SettingsPage::Plugins, cx);
                    }),
                ),
            )),
        ));
    }
    destination_view(
        "plugins",
        "Plugins",
        "Installed capabilities and available extensions",
        cards,
    )
}

fn settings_view(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    div()
        .id("settings-view")
        .flex_1()
        .min_h_0()
        .flex()
        .overflow_hidden()
        .child(
            div()
                .id("settings-navigation")
                .w(px(220.0))
                .h_full()
                .overflow_y_scroll()
                .p_4()
                .flex()
                .flex_col()
                .gap_1()
                .children(SettingsPage::ALL.iter().map(|page| {
                    let page = *page;
                    nav_item(
                        ElementId::Name(format!("settings-{:?}", page).into()),
                        page.icon(),
                        page.title(),
                        state.settings_page == page,
                        window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                            this.select_settings_page(page, cx)
                        }),
                    )
                })),
        )
        .child(settings_panel(state, window, cx))
}

fn settings_panel(
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    let page = state.settings_page;
    div()
        .id("settings-panel")
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_y_scroll()
        .border_l_1()
        .border_color(theme::border())
        .child(
            div()
                .id("settings-content")
                .max_w(px(780.0))
                .w_full()
                .px_8()
                .py_8()
                .flex()
                .flex_col()
                .gap_5()
                .child(
                    div()
                        .text_size(rems(1.25))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(page.title()),
                )
                .child(
                    div()
                        .text_size(rems(0.82))
                        .text_color(theme::text_muted())
                        .child(settings_description(page)),
                )
                .child(settings_page_body(page, state, window, cx)),
        )
}

fn settings_description(page: SettingsPage) -> &'static str {
    match page {
        SettingsPage::General => "Control how Codex starts tasks and stores local UI state.",
        SettingsPage::Account => "Account and authentication status for this desktop client.",
        SettingsPage::Appearance => "Theme, density, and motion preferences.",
        SettingsPage::Notifications => "Choose which task and turn events can interrupt you.",
        SettingsPage::Apps => "Connected apps and browser-facing capabilities.",
        SettingsPage::Mcp => "Model Context Protocol servers available to Codex.",
        SettingsPage::Skills => "Reusable skills and instruction sources.",
        SettingsPage::Plugins => "Installed plugins, permissions, and marketplaces.",
        SettingsPage::Keybindings => "Keyboard shortcuts for navigation and task control.",
        SettingsPage::Worktrees => "Git worktree defaults and environment selection.",
        SettingsPage::Git => "Git identity, attribution, and repository behavior.",
    }
}

fn settings_page_body(
    page: SettingsPage,
    state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Div {
    match page {
        SettingsPage::General => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Default model",
                "Model used for new tasks",
                state.settings.default_model.clone(),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_model(cx)
                    }),
                )),
            ))
            .child(setting_row(
                "Reasoning effort",
                "Default thinking budget",
                state.settings.default_reasoning.clone(),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_reasoning(cx)
                    }),
                )),
            ))
            .child(setting_row(
                "Approval mode",
                "When Codex asks before an external action",
                state.settings.approval_mode.clone(),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_approval_mode(cx)
                    }),
                )),
            ))
            .child(setting_row(
                "Sandbox",
                "Execution policy for workspace tasks",
                state.settings.sandbox_mode.clone(),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_sandbox_mode(cx)
                    }),
                )),
            ))
            .child(setting_row(
                "State file",
                "Local UI state is separate from CODEX_HOME",
                crate::persistence::state_path().display().to_string(),
                None,
            )),
        SettingsPage::Account => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Signed in",
                "The local desktop account",
                state
                    .catalog
                    .account_label
                    .clone()
                    .unwrap_or_else(|| "Local account".into()),
                None,
            ))
            .child(setting_row(
                "Provider",
                "Authentication is delegated to Codex app-server",
                "OpenAI Codex".into(),
                None,
            ))
            .child(setting_row(
                "Data boundary",
                "Credentials never enter the UI snapshot",
                "Protected".into(),
                None,
            )),
        SettingsPage::Appearance => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Theme",
                "System, dark, or light appearance",
                state.settings.theme.clone(),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_theme(cx)
                    }),
                )),
            ))
            .child(setting_row(
                "Font size",
                "Base interface size",
                format!("{} px", state.settings.font_size),
                Some(Box::new(
                    window.listener_for(&cx.entity(), |this, _event, _window, cx| {
                        this.cycle_font_size(cx)
                    }),
                )),
            ))
            .child(setting_toggle(
                "Reduced motion",
                "Reduce animated transitions and progress effects",
                state.settings.reduced_motion,
                "reduced-motion",
                state,
                window,
                cx,
            )),
        SettingsPage::Notifications => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_toggle(
                "Task notifications",
                "Show turn and approval notifications",
                state.settings.notifications,
                "notifications",
                state,
                window,
                cx,
            ))
            .child(setting_toggle(
                "Sound effects",
                "Play a subtle completion sound",
                state.settings.sound_effects,
                "sound",
                state,
                window,
                cx,
            )),
        SettingsPage::Apps => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Installed apps",
                "Apps exposed by the local app-server",
                if state.catalog.installed_apps.is_empty() {
                    "None reported".into()
                } else {
                    state.catalog.installed_apps.join(", ")
                },
                None,
            ))
            .child(setting_row(
                "Discoverable apps",
                "Apps available to the current thread",
                if state.catalog.apps.is_empty() {
                    "None reported".into()
                } else {
                    state.catalog.apps.join(", ")
                },
                None,
            ))
            .child(setting_row(
                "Sites",
                "Hosted surfaces",
                if state.catalog.apps.is_empty() {
                    "Not configured".into()
                } else {
                    format!("{} app-backed surface(s)", state.catalog.apps.len())
                },
                None,
            ))
            .child(setting_row(
                "Refresh catalog",
                "Reload apps and connector metadata",
                "↻".into(),
                Some(Box::new(window.listener_for(
                    &cx.entity(),
                    |this, _event, _window, cx| {
                        this.refresh_catalog(cx);
                    },
                ))),
            )),
        SettingsPage::Mcp => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "MCP servers",
                "Server status reported by the app-server",
                if state.catalog.mcp_servers.is_empty() {
                    "None reported".into()
                } else {
                    state.catalog.mcp_servers.join(", ")
                },
                None,
            ))
            .child(setting_row(
                "Capabilities",
                "MCP access stays within app-server approvals",
                if state.connection == crate::state::ConnectionState::Live {
                    "Live"
                } else {
                    "Demo"
                }
                .into(),
                None,
            ))
            .child(setting_row(
                "Add server",
                "Configure an MCP server",
                "＋".into(),
                None,
            ))
            .child(setting_row(
                "Refresh servers",
                "Reload MCP status from Codex",
                "↻".into(),
                Some(Box::new(window.listener_for(
                    &cx.entity(),
                    |this, _event, _window, cx| {
                        this.refresh_catalog(cx);
                    },
                ))),
            )),
        SettingsPage::Skills => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Installed skills",
                "Skills loaded for this environment",
                if state.catalog.skills.is_empty() {
                    "None reported".into()
                } else {
                    state.catalog.skills.join(", ")
                },
                None,
            ))
            .child(setting_row(
                "Refresh",
                "Reload skill metadata",
                "↻".into(),
                Some(Box::new(window.listener_for(
                    &cx.entity(),
                    |this, _event, _window, cx| {
                        this.refresh_catalog(cx);
                    },
                ))),
            )),
        SettingsPage::Plugins => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Installed plugins",
                "Plugins reported by the local app-server",
                if state.catalog.plugins.is_empty() {
                    "None reported".into()
                } else {
                    state.catalog.plugins.join(", ")
                },
                None,
            ))
            .child(setting_row(
                "Marketplaces",
                "Plugin sources",
                format!("{} source(s)", state.catalog.plugins.len()),
                None,
            )),
        SettingsPage::Keybindings => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row("⌘ K", "Search tasks", "⌘ K".into(), None))
            .child(setting_row(
                "Enter",
                "Send composer message",
                "Enter".into(),
                None,
            ))
            .child(setting_row(
                "Shift + Enter",
                "Insert a newline",
                "Shift + Enter".into(),
                None,
            ))
            .child(setting_row(
                "Escape",
                "Close menus and search",
                "Escape".into(),
                None,
            ))
            .child(setting_row("⌘ ⇧ B", "Toggle sidebar", "⌘ ⇧ B".into(), None))
            .child(setting_row("F2", "Rename current task", "F2".into(), None))
            .child(setting_row("⌘ N", "Create a new task", "⌘ N".into(), None))
            .child(setting_row(
                "⌘ ⇧ M/R",
                "Cycle model/reasoning",
                "⌘ ⇧ M/R".into(),
                None,
            ))
            .child(setting_row(
                "⌘ ⇧ E/F",
                "Attach or mention",
                "⌘ ⇧ E/F".into(),
                None,
            ))
            .child(setting_row(
                "⌘ ⇧ X/Z",
                "Stop or archive",
                "⌘ ⇧ X/Z".into(),
                None,
            )),
        SettingsPage::Worktrees => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Environment",
                "Where a task executes",
                "Local workspace".into(),
                None,
            ))
            .child(setting_row(
                "Worktree root",
                "Optional root for isolated branches",
                if state.settings.worktree_root.is_empty() {
                    "Not configured".into()
                } else {
                    state.settings.worktree_root.clone()
                },
                None,
            ))
            .child(setting_row(
                "Auto setup",
                "Prepare dependencies when entering a worktree",
                "On".into(),
                None,
            )),
        SettingsPage::Git => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(setting_row(
                "Attribution",
                "Preserve Codex authorship metadata",
                "Enabled".into(),
                None,
            ))
            .child(setting_row(
                "Repository",
                "Current project path",
                state
                    .current_project()
                    .map(|project| project.path.clone())
                    .unwrap_or_default(),
                None,
            ))
            .child(setting_row(
                "Review",
                "Open changes in the native review surface",
                "Available".into(),
                None,
            )),
    }
}

fn setting_row(
    title: &'static str,
    description: &'static str,
    value: String,
    listener: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
) -> Stateful<Div> {
    let row = div()
        .id(ElementId::Name(
            format!("setting-{}", title.replace(' ', "-")).into(),
        ))
        .flex()
        .items_center()
        .gap_3()
        .bg(theme::bg_surface())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .px_4()
        .py_3()
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(rems(0.82))
                        .text_color(theme::text())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(rems(0.7))
                        .text_color(theme::text_faint())
                        .child(description),
                ),
        )
        .child(
            div()
                .max_w(px(260.0))
                .text_size(rems(0.75))
                .text_color(theme::text_muted())
                .truncate()
                .child(value),
        );
    if let Some(listener) = listener {
        row.on_click(listener)
    } else {
        row
    }
}

fn setting_toggle(
    title: &'static str,
    description: &'static str,
    enabled: bool,
    key: &'static str,
    _state: &AppState,
    window: &mut Window,
    cx: &mut Context<AppState>,
) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("setting-toggle-{key}").into()))
        .flex()
        .items_center()
        .gap_3()
        .bg(theme::bg_surface())
        .border_1()
        .border_color(theme::border())
        .rounded_lg()
        .px_4()
        .py_3()
        .cursor_pointer()
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(rems(0.82))
                        .text_color(theme::text())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(rems(0.7))
                        .text_color(theme::text_faint())
                        .child(description),
                ),
        )
        .child(
            div()
                .text_size(rems(0.74))
                .text_color(if enabled {
                    theme::success()
                } else {
                    theme::text_faint()
                })
                .child(if enabled { "On" } else { "Off" }),
        )
        .on_click(
            window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                this.toggle_bool_setting(key, cx)
            }),
        )
        .hover(|style| style.bg(theme::bg_hover()))
}

fn icon_button(
    id: &'static str,
    label: &'static str,
    _accessible_name: &'static str,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .size_7()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_size(rems(0.86))
        .text_color(theme::text_muted())
        .child(label)
        .on_click(listener)
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}

fn text_button(
    id: impl Into<ElementId>,
    label: &str,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_size(rems(0.72))
        .text_color(theme::text_muted())
        .child(label.to_string())
        .on_click(listener)
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}

fn menu_action(
    id: &'static str,
    label: &'static str,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .text_size(rems(0.76))
        .text_color(theme::text_muted())
        .child(label)
        .on_click(listener)
        .hover(|style| style.bg(theme::bg_hover()).text_color(theme::text()))
}
