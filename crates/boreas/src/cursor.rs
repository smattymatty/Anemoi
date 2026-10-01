//! The pointer, read in one place. A real run feeds `Cursor` from the primary
//! window; with no window nothing writes it, so a headless sandbox can.

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inspect::InspectApp;

/// The pointer's screen position in logical px, or `None` off the window.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Cursor(pub Option<Vec2>);

/// Copies the primary window's cursor; public so a driver can order after it.
pub fn feed(windows: Query<&Window, With<PrimaryWindow>>, mut cursor: ResMut<Cursor>) {
    if let Ok(window) = windows.single() {
        cursor.0 = window.cursor_position();
    }
}

fn dump(w: &World) -> String {
    match w.get_resource::<Cursor>().and_then(|c| c.0) {
        Some(p) => format!("at=({:.0},{:.0})", p.x, p.y),
        None => "at=none".into(),
    }
}

/// `Cursor`, fed each `PreUpdate`; `IntentPlugin` adds it.
pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cursor>()
            .add_systems(PreUpdate, feed.after(InputSystems))
            .inspect("cursor", dump);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(CursorPlugin);
        app
    }

    fn line(app: &App) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == "cursor").unwrap().1
    }

    /// Headless: a driver's position survives, since no window overwrites it.
    #[test]
    fn with_no_window_a_set_cursor_survives() {
        let mut app = app();
        app.update();
        assert_eq!(line(&app), "at=none");
        app.insert_resource(Cursor(Some(Vec2::new(648.4, 351.6))));
        app.update();
        assert_eq!(line(&app), "at=(648,352)");
    }

    #[test]
    fn the_primary_window_feeds_the_cursor() {
        let mut app = app();
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(10.0, 20.0)));
        let id = app.world_mut().spawn((window, PrimaryWindow)).id();
        app.update();
        assert_eq!(
            app.world().resource::<Cursor>().0,
            Some(Vec2::new(10.0, 20.0))
        );
        let mut w = app.world_mut().get_mut::<Window>(id).unwrap();
        w.set_cursor_position(None);
        app.update();
        assert_eq!(
            *app.world().resource::<Cursor>(),
            Cursor(None),
            "off the window"
        );
    }
}
