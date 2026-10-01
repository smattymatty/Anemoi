//! Turns Intents into presented Events, and owns the input lock. Exploring plays
//! every waiting Intent's Events at once; an Encounter plays one Unit's Intent per
//! beat, after the last beat's presentation ends. Presenters are game observers.

use std::collections::VecDeque;
use std::marker::PhantomData;

use aeolus::{Intent, Rules, UnitId, World};
use bevy::prelude::*;

use crate::inspect::InspectApp;
use crate::intent::Mode;
use crate::tween::Tween;

/// A game Boreas can drive: its rules and data live in a Bevy resource.
pub trait Game:
    Rules<
        Ext: Send + Sync + 'static,
        Terrain: Send + Sync + 'static,
        Condition: Send + Sync + 'static,
        Intent: Send + Sync + 'static,
        Event: Send + Sync + 'static,
    > + Send
    + Sync
    + 'static
{
}

impl<G> Game for G where
    G: Rules<
            Ext: Send + Sync + 'static,
            Terrain: Send + Sync + 'static,
            Condition: Send + Sync + 'static,
            Intent: Send + Sync + 'static,
            Event: Send + Sync + 'static,
        > + Send
        + Sync
        + 'static
{
}

/// The world, and the Unit that input drives. Read-only outside `pace`, so
/// every change goes through an Intent and the log replays it.
#[derive(Resource)]
pub struct Sim<G: Game> {
    world: World<G>,
    player: UnitId,
}

impl<G: Game> Sim<G> {
    pub fn new(world: World<G>, player: UnitId) -> Self {
        Self { world, player }
    }

    pub fn world(&self) -> &World<G> {
        &self.world
    }

    pub fn player(&self) -> UnitId {
        self.player
    }
}

/// One Unit's Intent. Input writes them; so may the game, for its own Intents.
#[derive(Message, Debug)]
pub struct Act<I: Send + Sync + 'static> {
    pub by: UnitId,
    pub intent: Intent<I>,
}

/// One Event to present, triggered for the game's observers.
#[derive(Event, Clone, Debug, PartialEq)]
pub struct Play<E: Send + Sync + 'static> {
    pub by: UnitId,
    pub event: aeolus::Event<E>,
}

/// A presentation still running. The lock holds while any exists, or any `Tween`.
#[derive(Component)]
pub struct Busy;

/// Applied Intents not yet presented, one beat per Intent.
#[derive(Resource)]
pub struct Pending<E>(pub VecDeque<(UnitId, Vec<aeolus::Event<E>>)>);

impl<E> Default for Pending<E> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

/// Input waits while this holds: Events still queued, or one still playing.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Lock(pub bool);

/// Runs in order each frame: the lock, then input, then play.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TurnSet {
    Lock,
    Input,
    Play,
}

type Playing = Or<(With<Busy>, With<Tween>)>;

/// How many waiting beats to play now.
pub fn beats(mode: Mode, waiting: usize) -> usize {
    match mode {
        Mode::Exploring => waiting,
        Mode::Encounter => waiting.min(1),
    }
}

fn lock<G: Game>(
    pending: Res<Pending<G::Event>>,
    busy: Query<(), Playing>,
    mut lock: ResMut<Lock>,
) {
    lock.set_if_neq(Lock(!pending.0.is_empty() || !busy.is_empty()));
}

fn play<G: Game>(
    mut commands: Commands,
    mut sim: ResMut<Sim<G>>,
    mut acts: ResMut<Messages<Act<G::Intent>>>,
    mut pending: ResMut<Pending<G::Event>>,
    mode: Res<Mode>,
    busy: Query<(), Playing>,
) {
    for act in acts.drain() {
        let events = sim.world.apply(act.by, act.intent);
        pending.0.push_back((act.by, events));
    }
    if !busy.is_empty() {
        return;
    }
    let now = beats(*mode, pending.0.len());
    for (by, events) in pending.0.drain(..now) {
        for event in events {
            commands.trigger(Play { by, event });
        }
    }
}

fn dump<G: Game>(w: &bevy::prelude::World) -> String {
    let waiting = w
        .get_resource::<Pending<G::Event>>()
        .map_or(0, |p| p.0.len());
    let locked = w.get_resource::<Lock>().is_some_and(|l| l.0);
    format!("waiting={waiting} locked={locked}")
}

/// Plays `G`'s Events. Needs a `Sim<G>` resource before the first Intent.
pub struct PacePlugin<G>(PhantomData<fn() -> G>);

impl<G> Default for PacePlugin<G> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<G: Game> Plugin for PacePlugin<G> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mode>()
            .init_resource::<Lock>()
            .init_resource::<Pending<G::Event>>()
            .add_message::<Act<G::Intent>>()
            .configure_sets(
                Update,
                (TurnSet::Lock, TurnSet::Input, TurnSet::Play).chain(),
            )
            .add_systems(Update, lock::<G>.in_set(TurnSet::Lock))
            .add_systems(
                Update,
                play::<G>
                    .in_set(TurnSet::Play)
                    .run_if(resource_exists::<Sim<G>>),
            )
            .inspect("pace", dump::<G>);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use aeolus::{Cell, Cost, Dir, Grid, Kind, Unit};

    /// Open ground, walls where the terrain is `true`.
    pub struct Walls;

    impl Rules for Walls {
        type Ext = ();
        type Terrain = bool;
        type Condition = ();
        type Intent = ();
        type Event = u8;

        fn kind<'a>(&'a self, wall: &'a bool) -> Kind<'a, ()> {
            if *wall {
                Kind::Blocking
            } else {
                Kind::Passable
            }
        }
        fn holds(&self, _: &Unit<()>, _: &()) -> bool {
            false
        }
        fn opened(&self, _: &bool) -> bool {
            false
        }
        fn act(_: &mut World<Self>, _: UnitId, _: ()) -> (Vec<u8>, Cost) {
            (vec![7, 8], Cost::Free)
        }
    }

    #[derive(Resource, Default)]
    pub struct Seen(pub Vec<Play<u8>>);

    /// A 5x5 open world with Units at (0,0) and (4,0); every Play is recorded.
    pub fn app() -> App {
        let mut world = World::new(Walls, Grid::new(5, 5, false), 1);
        let player = world.spawn(Unit::new("a", 1, Cell::new(0, 0), ()));
        world.spawn(Unit::new("b", 1, Cell::new(4, 0), ()));
        let mut app = App::new();
        app.add_plugins(PacePlugin::<Walls>::default())
            .insert_resource(Sim::new(world, player))
            .init_resource::<Seen>()
            .add_observer(|p: On<Play<u8>>, mut seen: ResMut<Seen>| seen.0.push(p.clone()));
        app
    }

    fn act(app: &mut App, by: u32) {
        let intent = Intent::Step(Dir::North);
        app.world_mut().write_message(Act::<()> {
            by: UnitId(by),
            intent,
        });
    }

    fn seen(app: &App) -> Vec<UnitId> {
        app.world()
            .resource::<Seen>()
            .0
            .iter()
            .map(|p| p.by)
            .collect()
    }

    #[test]
    fn beats_are_all_while_exploring_and_one_in_an_encounter() {
        assert_eq!(beats(Mode::Exploring, 3), 3);
        assert_eq!(beats(Mode::Encounter, 3), 1);
        assert_eq!(beats(Mode::Encounter, 0), 0);
    }

    #[test]
    fn exploring_plays_every_intent_in_one_frame() {
        let mut app = app();
        act(&mut app, 0);
        act(&mut app, 1);
        app.update();
        assert_eq!(seen(&app), [UnitId(0), UnitId(1)]);
        assert_eq!(app.world().resource::<Sim<Walls>>().world().turn(), 2);
        app.update();
        assert_eq!(*app.world().resource::<Lock>(), Lock(false));
    }

    /// Presenters get each Event whole, in the order Aeolus returned it.
    #[test]
    fn a_beat_plays_its_events_in_order() {
        let mut app = app();
        let intent = Intent::Game(());
        app.world_mut().write_message(Act::<()> {
            by: UnitId(0),
            intent,
        });
        act(&mut app, 0);
        app.update();
        let by = UnitId(0);
        let play = |event| Play { by, event };
        let (from, to) = (Cell::new(0, 0), Cell::new(0, 1));
        let want = [
            play(aeolus::Event::Game(7)),
            play(aeolus::Event::Game(8)),
            play(aeolus::Event::Moved { unit: by, from, to }),
        ];
        assert_eq!(app.world().resource::<Seen>().0, want);
    }

    #[test]
    fn an_encounter_waits_for_each_presentation_and_holds_the_lock() {
        let mut app = app();
        app.insert_resource(Mode::Encounter);
        act(&mut app, 0);
        act(&mut app, 1);
        app.update();
        assert_eq!(seen(&app), [UnitId(0)], "one Unit per beat");
        let busy = app.world_mut().spawn(Busy).id();
        app.update();
        assert_eq!(seen(&app), [UnitId(0)], "waits while a presentation runs");
        assert_eq!(*app.world().resource::<Lock>(), Lock(true));
        app.world_mut().despawn(busy);
        app.update();
        assert_eq!(seen(&app), [UnitId(0), UnitId(1)]);
        app.update();
        assert_eq!(*app.world().resource::<Lock>(), Lock(false));
    }

    #[test]
    fn queued_events_hold_the_lock() {
        let mut app = app();
        app.insert_resource(Mode::Encounter);
        act(&mut app, 0);
        act(&mut app, 1);
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<Lock>(), Lock(true));
    }

    #[test]
    fn a_running_tween_holds_the_lock() {
        let mut app = app();
        let tween = Tween::new(Vec2::ZERO, Vec2::X, 1.0, EaseFunction::Linear);
        app.world_mut().spawn(tween);
        app.update();
        assert_eq!(*app.world().resource::<Lock>(), Lock(true));
    }
}
