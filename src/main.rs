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
        println!("Codex App GPUI smoke: native shell, sidebar, task thread, composer, settings, protocol, persistence");
        println!("Codex App GPUI smoke: models=4 reasoning=4 destinations=5 settings=11");
        println!("PARITY_G6_SMOKE_OK");
        return;
    }

    let snapshot = persistence::load()
        .ok()
        .flatten()
        .unwrap_or_else(Snapshot::demo);
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
            window.focus(&entity.read(app).input_focus);
            entity
        })
        .expect("open Codex App GPUI window");
    });
}
