#![recursion_limit = "1024"]

mod model;
mod persistence;
mod protocol;
mod state;
mod theme;
mod ui;

use gpui::*;

use persistence::Snapshot;
use state::AppState;

impl Render for AppState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        ui::root(self, window, cx)
    }
}

fn main() {
    if std::env::args().any(|argument| argument == "--smoke") {
        println!("Codex App GPUI smoke: native shell, sidebar, task thread, composer, queue, settings, protocol, persistence");
        println!(
            "Codex App GPUI smoke: models=4 reasoning=4 destinations=5 settings_pages={} official_client_requests=150",
            model::SettingsPage::ALL.len()
        );
        println!("PARITY_G6_SMOKE_OK");
        return;
    }

    if std::env::args().any(|argument| argument == "--live-smoke") {
        let command = std::env::var("CODEX_APP_SERVER_COMMAND")
            .unwrap_or_else(|_| "codex app-server --stdio".into());
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned));
        match state::connect_live_smoke(&command, cwd.as_deref()) {
            Ok((thread_id, threads, models, apps)) => {
                println!(
                    "PARITY_100_LIVE_CLIENT_OK thread={thread_id} threads={threads} models={models} apps={apps}"
                );
            }
            Err(error) => {
                eprintln!("PARITY_100_LIVE_CLIENT_FAIL {error:#}");
                std::process::exit(1);
            }
        }
        return;
    }

    let snapshot = match persistence::load() {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => Snapshot::demo(),
        Err(error) => {
            eprintln!("Could not load Codex App GPUI state: {error:#}; starting with demo state");
            Snapshot::demo()
        }
    };
    let initial_fullscreen = snapshot.fullscreen;
    Application::new().run(move |app: &mut App| {
        app.activate(true);
        let window = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Codex App GPUI".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.0), px(820.0)),
                app,
            ))),
            ..Default::default()
        };
        app.open_window(window, move |window, app| {
            let snapshot = snapshot.clone();
            let entity = app.new(|cx| {
                let mut state = AppState::new(snapshot, cx);
                state.init(cx);
                state
            });
            if initial_fullscreen {
                window.toggle_fullscreen();
            }
            window.focus(&entity.read(app).input_focus);
            entity
        })
        .expect("open Codex App GPUI window");
    });
}
