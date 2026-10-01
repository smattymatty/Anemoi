//! Keys and the mouse to Intents for the player's Unit. While Exploring a held
//! key repeats and a click Travels, one Step per unlocked frame; in an Encounter
//! every Step is one press or one click. The game sets the `Mode`.

use std::marker::PhantomData;

use aeolus::{Cell, Dir, Intent, next_step};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inspect::InspectApp;
use crate::pace::{Act, Game, Lock, PacePlugin, Sim, TurnSet};

/// Set by the game. Entering an Encounter stops any Travel.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Exploring,
    Encounter,
}

/// Remappable. The first binding held wins, so two keys never make a diagonal.
#[derive(Resource, Clone, Debug)]
pub struct Bindings {
    pub steps: Vec<(KeyCode, Dir)>,
    pub travel: MouseButton,
    /// World units per cell, to turn a click into a `Cell`.
    pub cell_px: f32,
}

impl Default for Bindings {
    fn default() -> Self {
        use KeyCode::*;
        Self {
            steps: vec![
                (KeyW, Dir::North),
                (ArrowUp, Dir::North),
                (KeyS, Dir::South),
                (ArrowDown, Dir::South),
                (KeyA, Dir::West),
                (ArrowLeft, Dir::West),
                (KeyD, Dir::East),
                (ArrowRight, Dir::East),
            ],
            travel: MouseButton::Left,
            cell_px: 16.0,
        }
    }
}

/// Walk to a cell: a click, or the sandbox's `click_tile`.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TravelTo(pub Cell);

/// Where Travel is heading, if anywhere.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Travel(pub Option<Cell>);

/// This frame's Step for a Unit at `at`. `click` counts only in an Encounter;
/// while Exploring a click has already become `travel`. `route` is the path query.
pub fn choose(
    mode: Mode,
    steps: &[(KeyCode, Dir)],
    keys: &ButtonInput<KeyCode>,
    travel: &mut Option<Cell>,
    click: Option<Cell>,
    at: Cell,
    route: impl Fn(Cell) -> Option<Dir>,
) -> Option<Dir> {
    let down = |k| match mode {
        Mode::Exploring => keys.pressed(k),
        Mode::Encounter => keys.just_pressed(k),
    };
    if let Some(&(_, dir)) = steps.iter().find(|(k, _)| down(*k)) {
        *travel = None;
        return Some(dir);
    }
    if mode == Mode::Encounter {
        return click.and_then(route);
    }
    let to = (*travel)?;
    let dir = route(to);
    // The last step arrives, bumps, or opens and walks in; done either way.
    if dir.is_none_or(|d| at.step(d) == to) {
        *travel = None;
    }
    dir
}

fn read_input<G: Game>(
    (mode, bindings, lock): (Res<Mode>, Res<Bindings>, Res<Lock>),
    keys: Res<ButtonInput<KeyCode>>,
    mut clicks: MessageReader<TravelTo>,
    mut travel: ResMut<Travel>,
    sim: Res<Sim<G>>,
    mut acts: MessageWriter<Act<G::Intent>>,
) {
    let click = clicks.read().last().map(|c| c.0);
    match *mode {
        Mode::Encounter => travel.0 = None,
        Mode::Exploring if click.is_some() => travel.0 = click,
        Mode::Exploring => {}
    }
    let Some(unit) = sim.world().unit(sim.player()) else {
        return;
    };
    if lock.0 {
        return;
    }
    let route = |to| next_step(sim.world(), sim.player(), to);
    let step = choose(
        *mode,
        &bindings.steps,
        &keys,
        &mut travel.0,
        click,
        unit.cell,
        route,
    );
    if let Some(dir) = step {
        let intent = Intent::Step(dir);
        acts.write(Act {
            by: sim.player(),
            intent,
        });
    }
}

/// A click on the map, as a cell. UI buttons keep their clicks.
fn click_cell(
    mouse: Res<ButtonInput<MouseButton>>,
    bindings: Res<Bindings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    ui: Query<&Interaction, With<Button>>,
    mut out: MessageWriter<TravelTo>,
) {
    if !mouse.just_pressed(bindings.travel) || ui.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let (Ok(window), Ok((cam, at))) = (windows.single(), camera.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if let Ok(p) = cam.viewport_to_world_2d(at, cursor) {
        out.write(TravelTo(cell_at(p, bindings.cell_px)));
    }
}

/// The cell under a world point, for cells `px` wide with (0, 0) at the origin.
pub fn cell_at(p: Vec2, px: f32) -> Cell {
    Cell::new((p.x / px).floor() as i32, (p.y / px).floor() as i32)
}

fn dump(w: &World) -> String {
    let mode = w.get_resource::<Mode>().copied().unwrap_or_default();
    let travel = w.get_resource::<Travel>().and_then(|t| t.0);
    let travel = travel.map_or("none".into(), |c| format!("({},{})", c.x, c.y));
    format!("mode={mode:?} travel={travel}")
}

/// Input for `G`'s player Unit; adds `PacePlugin<G>` if it is not in yet.
pub struct IntentPlugin<G>(PhantomData<fn() -> G>);

impl<G> Default for IntentPlugin<G> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<G: Game> Plugin for IntentPlugin<G> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<PacePlugin<G>>() {
            app.add_plugins(PacePlugin::<G>::default());
        }
        app.init_resource::<Bindings>()
            .init_resource::<Travel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<TravelTo>()
            .add_systems(
                Update,
                (
                    click_cell,
                    read_input::<G>.run_if(resource_exists::<Sim<G>>),
                )
                    .chain()
                    .in_set(TurnSet::Input),
            )
            .inspect("intent", dump);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pace::tests::{Walls, app};

    fn input_app() -> App {
        let mut app = app();
        app.add_plugins(IntentPlugin::<Walls>::default());
        app
    }

    fn cell(app: &App) -> Cell {
        let sim = app.world().resource::<Sim<Walls>>();
        sim.world().unit(sim.player()).unwrap().cell
    }

    fn keys(app: &mut App) -> Mut<'_, ButtonInput<KeyCode>> {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
    }

    #[test]
    fn a_held_key_repeats_while_exploring() {
        let mut app = input_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 2));
    }

    #[test]
    fn an_encounter_takes_one_step_per_press() {
        let mut app = input_app();
        app.insert_resource(Mode::Encounter);
        keys(&mut app).press(KeyCode::ArrowRight);
        app.update();
        keys(&mut app).clear();
        app.update();
        assert_eq!(cell(&app), Cell::new(1, 0));
    }

    #[test]
    fn bindings_remap_and_the_first_held_wins() {
        let mut app = input_app();
        app.insert_resource(Bindings {
            steps: vec![(KeyCode::KeyK, Dir::East), (KeyCode::KeyW, Dir::North)],
            ..default()
        });
        keys(&mut app).press(KeyCode::KeyW);
        keys(&mut app).press(KeyCode::KeyK);
        app.update();
        assert_eq!(cell(&app), Cell::new(1, 0));
    }

    #[test]
    fn travel_walks_one_step_per_frame_and_stops_on_arrival() {
        let mut app = input_app();
        app.world_mut().write_message(TravelTo(Cell::new(2, 0)));
        app.update();
        assert_eq!(cell(&app), Cell::new(1, 0));
        app.update();
        assert_eq!(cell(&app), Cell::new(2, 0));
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
    }

    #[test]
    fn a_key_cancels_travel() {
        let mut app = input_app();
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
        assert_eq!(cell(&app), Cell::new(1, 1));
    }

    #[test]
    fn an_encounter_click_is_one_step_and_ends_travel() {
        let mut app = input_app();
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        app.insert_resource(Mode::Encounter);
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "travel stops");
        app.world_mut().write_message(TravelTo(Cell::new(3, 1)));
        app.update();
        app.update();
        assert_eq!(cell(&app), Cell::new(1, 1), "one step per click");
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
    }

    /// No route: the Travel ends rather than retrying every frame.
    #[test]
    fn an_unreachable_travel_ends() {
        let keys = ButtonInput::<KeyCode>::default();
        let mut travel = Some(Cell::new(9, 9));
        let at = Cell::new(0, 0);
        let dir = choose(Mode::Exploring, &[], &keys, &mut travel, None, at, |_| None);
        assert_eq!((dir, travel), (None, None));
    }

    /// Sandbox scripts `expect` these whole tokens (`scripts/walk.toml`).
    #[test]
    fn the_dumps_carry_the_tokens_scripts_expect() {
        let mut app = input_app();
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        let got = crate::inspect::snapshot(app.world());
        let line = |name| got.iter().find(|(n, _)| *n == name).unwrap().1.clone();
        assert_eq!(line("intent"), "mode=Exploring travel=(0,3)");
        assert_eq!(line("pace"), "waiting=0 locked=false");
        app.insert_resource(Mode::Encounter);
        app.update();
        let got = crate::inspect::snapshot(app.world());
        let intent = got.iter().find(|(n, _)| *n == "intent").unwrap();
        assert_eq!(intent.1, "mode=Encounter travel=none");
    }

    #[test]
    fn a_point_floors_to_its_cell() {
        assert_eq!(cell_at(Vec2::new(-0.5, 17.0), 16.0), Cell::new(-1, 1));
        assert_eq!(cell_at(Vec2::new(31.9, 0.0), 16.0), Cell::new(1, 0));
    }

    #[test]
    fn the_lock_holds_input() {
        let mut app = input_app();
        app.world_mut().spawn(crate::pace::Busy);
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 0));
    }
}
