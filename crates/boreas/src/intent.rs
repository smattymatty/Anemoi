//! Keys and the mouse to Intents for the player's Unit. While Exploring a held
//! key repeats and a click Travels, a Step per `Cadence`, and a bump ends the Run;
//! in an Encounter every Step is one press or one click. The game sets the `Mode`.

use std::marker::PhantomData;

use aeolus::{Cell, Dir, Intent, next_step};
use bevy::prelude::*;

use crate::cursor::CursorPlugin;
use crate::inspect::InspectApp;
use crate::outline::Pointer;
use crate::owner::{self, InputBase, InputOwner, Owner};
use crate::pace::{Act, Cadence, Game, Lock, PacePlugin, Play, Sim, TurnSet};

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

/// Who a map click goes to: `click_cell`'s Travel, or the Tile Menu.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClickMode {
    #[default]
    Travel,
    TileMenu,
}

impl ClickMode {
    /// The `clicks=` token the `intent` dump carries.
    pub fn token(self) -> &'static str {
        match self {
            ClickMode::Travel => "travel",
            ClickMode::TileMenu => "menu",
        }
    }
}

/// Where Travel is heading, if anywhere.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Travel(pub Option<Cell>);

/// The held Run: the key behind the last Step, the keys a bump muted until each
/// is released, and the seconds until a repeat may step.
#[derive(Resource, Default, Debug)]
struct Run {
    key: Option<KeyCode>,
    muted: Vec<KeyCode>,
    wait: f32,
}

impl Run {
    /// Unmutes released keys and counts down, keeping at most this frame's overshoot.
    fn tick(&mut self, keys: &ButtonInput<KeyCode>, dt: f32) {
        self.muted.retain(|&k| keys.pressed(k));
        self.wait = (self.wait - dt).max(-dt);
    }

    /// The first unmuted binding down, and whether it was just pressed. An
    /// Encounter needs a fresh press.
    fn pick(
        &self,
        mode: Mode,
        steps: &[(KeyCode, Dir)],
        keys: &ButtonInput<KeyCode>,
    ) -> Option<(KeyCode, Dir, bool)> {
        let down = |k| match mode {
            Mode::Exploring => keys.pressed(k),
            Mode::Encounter => keys.just_pressed(k),
        };
        let (k, d) = steps
            .iter()
            .copied()
            .find(|&(k, _)| !self.muted.contains(&k) && down(k))?;
        Some((k, d, keys.just_pressed(k)))
    }

    /// A fresh press or click steps at once; a repeat waits out the cadence.
    fn ready(&self, fresh: bool) -> bool {
        fresh || self.wait <= 0.0
    }

    /// A repeat keeps its overshoot, so the steps keep the cadence.
    fn stepped(&mut self, key: Option<KeyCode>, fresh: bool, cadence: f32) {
        let carry = if fresh { 0.0 } else { self.wait };
        self.wait = cadence + carry;
        self.key = key;
    }

    /// Mutes every step key now held, so a handback starts still.
    fn mute_held(&mut self, steps: &[(KeyCode, Dir)], keys: &ButtonInput<KeyCode>) {
        for &(k, _) in steps {
            if keys.pressed(k) && !self.muted.contains(&k) {
                self.muted.push(k);
            }
        }
    }

    /// A bump mutes the key behind the last Step until it is released.
    fn end(&mut self) {
        if let Some(k) = self.key.filter(|k| !self.muted.contains(k)) {
            self.muted.push(k);
        }
    }
}

/// This frame's Step for a Unit at `at`. `click` counts only in an Encounter;
/// while Exploring a click has already become `travel`. `route` is the path query.
pub fn choose(
    mode: Mode,
    key: Option<Dir>,
    travel: &mut Option<Cell>,
    click: Option<Cell>,
    at: Cell,
    route: impl Fn(Cell) -> Option<Dir>,
) -> Option<Dir> {
    if let Some(dir) = key {
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
    (mode, bindings, lock, cadence): (Res<Mode>, Res<Bindings>, Res<Lock>, Res<Cadence>),
    (keys, time): (Res<ButtonInput<KeyCode>>, Res<Time>),
    mut clicks: MessageReader<TravelTo>,
    (mut travel, mut run): (ResMut<Travel>, ResMut<Run>),
    (sim, owner): (Res<Sim<G>>, InputOwner),
    mut acts: MessageWriter<Act<G::Intent>>,
) {
    let click = clicks.read().last().map(|c| c.0);
    if owner.get() != Owner::Gameplay {
        travel.0 = None;
        run.mute_held(&bindings.steps, &keys);
        return;
    }
    match *mode {
        Mode::Encounter => travel.0 = None,
        Mode::Exploring if click.is_some() => travel.0 = click,
        Mode::Exploring => {}
    }
    let Some(unit) = sim.world().unit(sim.player()) else {
        return;
    };
    run.tick(&keys, time.delta_secs());
    if lock.0 {
        return;
    }
    let key = run.pick(*mode, &bindings.steps, &keys);
    let fresh = key.is_some_and(|(_, _, f)| f) || click.is_some();
    if !run.ready(fresh) {
        return;
    }
    let route = |to| next_step(sim.world(), sim.player(), to);
    let step = choose(
        *mode,
        key.map(|(_, d, _)| d),
        &mut travel.0,
        click,
        unit.cell,
        route,
    );
    if let Some(dir) = step {
        run.stepped(key.map(|(k, _, _)| k), fresh, cadence.0.as_secs_f32());
        let intent = Intent::Step(dir);
        acts.write(Act {
            by: sim.player(),
            intent,
        });
    }
}

/// A bump or refusal of the player's own key Step ends the Run while Exploring.
fn end_run<G: Game>(
    play: On<Play<G::Event>>,
    mode: Res<Mode>,
    sim: Res<Sim<G>>,
    mut run: ResMut<Run>,
) {
    let stopped = matches!(
        play.event,
        aeolus::Event::Bumped { .. } | aeolus::Event::Refused { .. }
    );
    if *mode == Mode::Exploring && play.by == sim.player() && stopped {
        run.end();
    }
}

/// A click on the map, as a lit grid cell. UI buttons keep their clicks.
fn click_cell<G: Game>(
    mouse: Res<ButtonInput<MouseButton>>,
    bindings: Res<Bindings>,
    pointer: Pointer<G>,
    ui: Query<&Interaction, With<Button>>,
    mut out: MessageWriter<TravelTo>,
) {
    if !mouse.just_pressed(bindings.travel) || ui.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    if let Some(cell) = pointer.cell() {
        out.write(TravelTo(cell));
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
    let ended = w.get_resource::<Run>().is_some_and(|r| !r.muted.is_empty());
    let run = if ended { "ended" } else { "live" };
    let owner = owner::of(w).token();
    let clicks = w.get_resource::<ClickMode>().copied().unwrap_or_default();
    let clicks = clicks.token();
    format!("owner={owner} mode={mode:?} clicks={clicks} travel={travel} run={run}")
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
        if !app.is_plugin_added::<CursorPlugin>() {
            app.add_plugins(CursorPlugin);
        }
        app.init_resource::<Bindings>()
            .init_resource::<InputBase>()
            .init_resource::<Travel>()
            .init_resource::<ClickMode>()
            .init_resource::<Run>()
            .init_resource::<Time>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<TravelTo>()
            .add_systems(
                Update,
                (
                    click_cell::<G>.run_if(resource_equals(ClickMode::Travel)),
                    read_input::<G>,
                )
                    .chain()
                    .run_if(resource_exists::<Sim<G>>)
                    .in_set(TurnSet::Input),
            )
            .add_observer(end_run::<G>)
            .inspect("intent", dump);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pace::tests::{Seen, Walls, app};
    use crate::pace::{Cadence, Play};
    use aeolus::UnitId;
    use std::time::Duration;

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

    /// Repeats wait 100 ms; every frame lasts 30 ms.
    fn paced_app() -> App {
        let mut app = input_app();
        app.insert_resource(Cadence(Duration::from_millis(100)));
        let dt = Duration::from_millis(30);
        app.world_mut().resource_mut::<Time>().advance_by(dt);
        app
    }

    fn bumps(app: &App) -> usize {
        let seen = &app.world().resource::<Seen>().0;
        let bump = |p: &&Play<u8>| matches!(p.event, aeolus::Event::Bumped { .. });
        seen.iter().filter(bump).count()
    }

    fn line(app: &App, name: &str) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == name).unwrap().1
    }

    /// The player's cell after `frames` more updates.
    fn after(app: &mut App, frames: usize) -> Cell {
        for _ in 0..frames {
            app.update();
        }
        cell(app)
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
    fn a_held_key_repeats_no_faster_than_the_cadence() {
        let mut app = paced_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        assert_eq!(cell(&app), Cell::new(0, 1), "the first press steps at once");
        assert_eq!(after(&mut app, 3), Cell::new(0, 1), "90 ms: waits");
        assert_eq!(after(&mut app, 1), Cell::new(0, 2), "120 ms: repeats");
        assert_eq!(after(&mut app, 2), Cell::new(0, 2));
        assert_eq!(
            after(&mut app, 1),
            Cell::new(0, 3),
            "the 20 ms carries over"
        );
    }

    #[test]
    fn travel_steps_no_faster_than_the_cadence() {
        let mut app = paced_app();
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "the click steps at once");
        assert_eq!(after(&mut app, 3), Cell::new(0, 1));
        assert_eq!(after(&mut app, 1), Cell::new(0, 2));
        app.world_mut().write_message(TravelTo(Cell::new(1, 2)));
        app.update();
        assert_eq!(cell(&app), Cell::new(1, 2), "a new click steps at once");
    }

    /// The Run ends with one bump; that key is muted until released.
    #[test]
    fn a_held_key_bumps_once_and_another_key_steps_at_once() {
        let mut app = paced_app();
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        keys(&mut app).clear();
        for _ in 0..30 {
            if bumps(&app) > 0 {
                break;
            }
            app.update();
        }
        assert_eq!((cell(&app), bumps(&app)), (Cell::new(3, 0), 1));
        assert!(line(&app, "intent").ends_with(" run=ended"));
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(cell(&app), Cell::new(3, 1), "a new key skips the cadence");
        keys(&mut app).release(KeyCode::KeyW);
        keys(&mut app).clear();
        assert_eq!(
            after(&mut app, 10),
            Cell::new(3, 1),
            "the bumped key stays muted"
        );
        assert_eq!(bumps(&app), 1);
        keys(&mut app).release(KeyCode::KeyD);
        app.update();
        assert!(line(&app, "intent").ends_with(" run=live"));
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        assert_eq!(cell(&app), Cell::new(4, 1), "released, it steps again");
    }

    /// Each bumped key stays muted while held, so two held keys never take turns.
    #[test]
    fn two_held_keys_each_bump_once() {
        let mut app = paced_app();
        let mut grid = aeolus::Grid::new(5, 5, false);
        grid.set(Cell::new(3, 2), true);
        grid.set(Cell::new(4, 1), true);
        let mut world = aeolus::World::new(Walls, grid, 1);
        let player = world.spawn(aeolus::Unit::new("a", 1, Cell::new(0, 0), ()));
        world.spawn(aeolus::Unit::new("b", 1, Cell::new(4, 0), ()));
        app.insert_resource(Sim::new(world, player));
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        assert_eq!(after(&mut app, 30), Cell::new(3, 0));
        keys(&mut app).press(KeyCode::KeyW);
        // D, unmuted by W's bump, would bump (4,1) and the two would alternate.
        assert_eq!(after(&mut app, 60), Cell::new(3, 1));
        assert_eq!(bumps(&app), 2);
        assert!(line(&app, "intent").ends_with(" run=ended"));
    }

    /// A release while the lock holds still unmutes the bumped key.
    #[test]
    fn a_release_under_the_lock_unmutes_the_key() {
        let mut app = input_app();
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        keys(&mut app).clear();
        after(&mut app, 5);
        assert_eq!((cell(&app), bumps(&app)), (Cell::new(3, 0), 1));
        let busy = app.world_mut().spawn(crate::pace::Busy).id();
        keys(&mut app).release(KeyCode::KeyD);
        app.update();
        keys(&mut app).press(KeyCode::KeyD);
        app.update();
        keys(&mut app).clear();
        app.world_mut().despawn(busy);
        app.update();
        assert_eq!(bumps(&app), 2, "the re-pressed key runs again");
    }

    /// Time under the lock counts toward the cadence.
    #[test]
    fn the_cadence_counts_down_under_the_lock() {
        let mut app = paced_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        let busy = app.world_mut().spawn(crate::pace::Busy).id();
        assert_eq!(after(&mut app, 4), Cell::new(0, 1), "locked");
        app.world_mut().despawn(busy);
        assert_eq!(after(&mut app, 1), Cell::new(0, 2), "repeats on unlock");
    }

    /// A repeat carries at most one frame's overshoot, however long it waited.
    #[test]
    fn a_run_carries_at_most_one_frame() {
        let (mut run, keys) = (Run::default(), ButtonInput::<KeyCode>::default());
        for _ in 0..10 {
            run.tick(&keys, 0.03);
        }
        assert!(run.ready(false));
        run.stepped(Some(KeyCode::KeyW), false, 0.1);
        assert!((run.wait - 0.07).abs() < 1e-6, "wait={}", run.wait);
        run.stepped(Some(KeyCode::KeyW), true, 0.1);
        assert!(!run.ready(false) && run.ready(true));
    }

    #[test]
    fn a_refusal_ends_the_run_and_another_units_bump_does_not() {
        let mut app = input_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        let (unit, at) = (UnitId(1), Cell::new(4, 0));
        app.world_mut().trigger(Play {
            by: unit,
            event: aeolus::Event::<u8>::Bumped { unit, at },
        });
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 2), "still running");
        let (unit, at) = (UnitId(0), Cell::new(0, 3));
        app.world_mut().trigger(Play {
            by: unit,
            event: aeolus::Event::<u8>::Refused { unit, at },
        });
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 2), "ended");
    }

    /// A key held from Exploring does not keep stepping once an Encounter starts.
    #[test]
    fn an_encounter_needs_a_fresh_press() {
        let mut app = input_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        app.insert_resource(Mode::Encounter);
        assert_eq!(after(&mut app, 2), Cell::new(0, 1));
        keys(&mut app).release(KeyCode::KeyW);
        keys(&mut app).press(KeyCode::KeyW);
        assert_eq!(after(&mut app, 1), Cell::new(0, 2));
        let (unit, at) = (UnitId(0), Cell::new(0, 3));
        app.world_mut().trigger(Play {
            by: unit,
            event: aeolus::Event::<u8>::Refused { unit, at },
        });
        assert!(line(&app, "intent").ends_with(" run=live"), "no Run to end");
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
        let mut travel = Some(Cell::new(9, 9));
        let at = Cell::new(0, 0);
        let dir = choose(Mode::Exploring, None, &mut travel, None, at, |_| None);
        assert_eq!((dir, travel), (None, None));
    }

    /// Sandbox scripts `expect` these whole tokens (`scripts/walk.toml`).
    #[test]
    fn the_dumps_carry_the_tokens_scripts_expect() {
        let mut app = input_app();
        app.insert_resource(Cadence(Duration::from_millis(150)));
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        let got = crate::inspect::snapshot(app.world());
        let line = |name| got.iter().find(|(n, _)| *n == name).unwrap().1.clone();
        let want = "owner=gameplay mode=Exploring clicks=travel travel=(0,3) run=live";
        assert_eq!(line("intent"), want);
        assert_eq!(line("pace"), "waiting=0 locked=false cadence=150");
        assert_eq!(line("sim"), "turn=1 player=(0,1)");
        app.insert_resource(Mode::Encounter);
        app.update();
        let got = crate::inspect::snapshot(app.world());
        let intent = got.iter().find(|(n, _)| *n == "intent").unwrap();
        let want = "owner=gameplay mode=Encounter clicks=travel travel=none run=live";
        assert_eq!(intent.1, want);
        app.insert_resource(ClickMode::TileMenu);
        app.update();
        let got = crate::inspect::snapshot(app.world());
        let intent = got.iter().find(|(n, _)| *n == "intent").unwrap();
        assert!(intent.1.contains(" clicks=menu "), "{}", intent.1);
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
