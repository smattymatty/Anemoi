//! The world: a grid, Units and the Turn count. Intents go in, Events come out.

use std::hash::{Hash, Hasher};

use crate::gate::Gate;
use crate::grid::{Cell, Dir, Grid};
use crate::hash::StableHasher;
use crate::rng::Pcg32;

/// The base every game builds on; `ext` is the game's own data.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unit<G> {
    pub name: String,
    pub health: i32,
    pub cell: Cell,
    /// A solid Unit blocks its cell: stepping there bumps it.
    pub solid: bool,
    pub ext: G,
}

impl<G> Unit<G> {
    /// A solid Unit.
    pub fn new(name: impl Into<String>, health: i32, cell: Cell, ext: G) -> Self {
        Self {
            name: name.into(),
            health,
            cell,
            solid: true,
            ext,
        }
    }
}

/// A Unit's place in its world, in spawn order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent<I> {
    Step(Dir),
    /// A game's own Intent, handled by `Rules::act`, so replay logs it too.
    Game(I),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Event<E> {
    Moved { unit: UnitId, from: Cell, to: Cell },
    Bumped { unit: UnitId, at: Cell },
    Opened { unit: UnitId, at: Cell },
    Refused { unit: UnitId, at: Cell },
    Game(E),
}

/// What a cell's terrain is to a stepping Unit.
pub enum Kind<'a, C> {
    Passable,
    Blocking,
    Gate(&'a Gate<C>),
}

/// What bumping a cell does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bump<T> {
    /// Stay put, and say no (a shut gate).
    Refuse,
    /// Stay put; nothing gives.
    Stay,
    /// The cell becomes `T`, and the same step passes into it.
    Open(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost {
    Free,
    Turn,
}

/// The game's hooks. Aeolus calls them; it never knows what the terrain,
/// conditions or game Intents mean.
pub trait Rules: Sized {
    type Ext;
    type Terrain;
    type Condition;
    type Intent;
    type Event;

    fn kind<'a>(&'a self, terrain: &'a Self::Terrain) -> Kind<'a, Self::Condition>;

    fn holds(&self, unit: &Unit<Self::Ext>, condition: &Self::Condition) -> bool;

    /// What a gate's terrain becomes once it opens.
    fn opened(&self, gate: &Self::Terrain) -> Self::Terrain;

    /// Called only for a `Blocking` cell; the engine decides gates itself.
    fn bump(
        &self,
        _unit: &Unit<Self::Ext>,
        _at: Cell,
        _terrain: &Self::Terrain,
    ) -> Bump<Self::Terrain> {
        Bump::Stay
    }

    /// The game's own Offers for `cell`, after the Step. Asked only for the
    /// Unit's own cell and its neighbours; see `offer::offers`.
    fn offers(_world: &World<Self>, _by: UnitId, _cell: Cell) -> Vec<Self::Intent> {
        Vec::new()
    }

    /// May change the world: the one place outside setup that should.
    fn act(world: &mut World<Self>, by: UnitId, intent: Self::Intent) -> (Vec<Self::Event>, Cost);

    /// Whether `unit` may walk onto `terrain`: passable, or a gate open for it.
    fn passes(&self, unit: &Unit<Self::Ext>, terrain: &Self::Terrain) -> bool {
        match self.kind(terrain) {
            Kind::Passable => true,
            Kind::Blocking => false,
            Kind::Gate(gate) => gate.opens(|c| self.holds(unit, c)),
        }
    }
}

/// Mutators (`grid_mut`, `unit_mut`, `rng`) are for setup and `Rules::act`
/// only; anything else breaks replay.
pub struct World<G: Rules> {
    rules: G,
    grid: Grid<G::Terrain>,
    units: Vec<Unit<G::Ext>>,
    turn: u64,
    rng: Pcg32,
}

impl<G: Rules> World<G> {
    pub fn new(rules: G, grid: Grid<G::Terrain>, seed: u64) -> Self {
        Self {
            rules,
            grid,
            units: Vec::new(),
            turn: 0,
            rng: Pcg32::new(seed, 0),
        }
    }

    pub fn spawn(&mut self, unit: Unit<G::Ext>) -> UnitId {
        self.units.push(unit);
        UnitId(self.units.len() as u32 - 1)
    }

    pub fn rules(&self) -> &G {
        &self.rules
    }

    pub fn grid(&self) -> &Grid<G::Terrain> {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid<G::Terrain> {
        &mut self.grid
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit<G::Ext>> {
        self.units.get(id.0 as usize)
    }

    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit<G::Ext>> {
        self.units.get_mut(id.0 as usize)
    }

    /// Every Unit with its id, in spawn order.
    pub fn units(&self) -> impl Iterator<Item = (UnitId, &Unit<G::Ext>)> {
        self.units
            .iter()
            .enumerate()
            .map(|(i, u)| (UnitId(i as u32), u))
    }

    /// The solid Unit on `at`, if any.
    pub fn solid_at(&self, at: Cell) -> Option<UnitId> {
        self.units()
            .find(|(_, u)| u.solid && u.cell == at)
            .map(|(id, _)| id)
    }

    pub fn turn(&self) -> u64 {
        self.turn
    }

    pub fn rng(&mut self) -> &mut Pcg32 {
        &mut self.rng
    }

    /// Grid, Units, Turn and RNG in one number that native and wasm agree on.
    pub fn state_hash(&self) -> u64
    where
        G::Terrain: Hash,
        G::Ext: Hash,
    {
        let mut h = StableHasher::default();
        (&self.grid, &self.units, self.turn, &self.rng).hash(&mut h);
        h.finish()
    }

    pub fn apply(&mut self, by: UnitId, intent: Intent<G::Intent>) -> Vec<Event<G::Event>> {
        match intent {
            Intent::Step(dir) => self.step(by, dir),
            Intent::Game(intent) => {
                let (events, cost) = G::act(self, by, intent);
                self.spend(cost);
                events.into_iter().map(Event::Game).collect()
            }
        }
    }

    fn spend(&mut self, cost: Cost) {
        if cost == Cost::Turn {
            self.turn += 1;
        }
    }

    /// A move costs a Turn; a bump costs none unless it opens and passes.
    fn step(&mut self, by: UnitId, dir: Dir) -> Vec<Event<G::Event>> {
        let Some(unit) = self.unit(by) else {
            return Vec::new();
        };
        let from = unit.cell;
        let to = from.step(dir);
        let Some(terrain) = self.grid.get(to) else {
            return Vec::new();
        };
        if self.solid_at(to).is_some() {
            return vec![Event::Bumped { unit: by, at: to }];
        }
        let opened = match self.rules.kind(terrain) {
            Kind::Passable => None,
            Kind::Gate(gate) if gate.opens(|c| self.rules.holds(unit, c)) => {
                Some(self.rules.opened(terrain))
            }
            Kind::Gate(_) => return vec![Event::Refused { unit: by, at: to }],
            Kind::Blocking => match self.rules.bump(unit, to, terrain) {
                Bump::Refuse => return vec![Event::Refused { unit: by, at: to }],
                Bump::Stay => return vec![Event::Bumped { unit: by, at: to }],
                Bump::Open(t) => Some(t),
            },
        };
        let mut events = Vec::new();
        if let Some(t) = opened {
            self.grid.set(to, t);
            events.push(Event::Opened { unit: by, at: to });
        }
        self.units[by.0 as usize].cell = to;
        self.spend(Cost::Turn);
        events.push(Event::Moved { unit: by, from, to });
        events
    }
}

/// A small game for the engine's own tests: walls, gates of numbered keys.
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum T {
        Open,
        Wall,
        Lock(Gate<u8>),
        /// Blocking; its bump hook refuses.
        Shut,
        /// Blocking; its bump hook opens it into `Open`.
        Hinge,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Act {
        Grant(u8),
        Wait,
    }

    pub struct Keys;

    impl Rules for Keys {
        type Ext = Vec<u8>;
        type Terrain = T;
        type Condition = u8;
        type Intent = Act;
        type Event = u8;

        fn kind<'a>(&'a self, terrain: &'a T) -> Kind<'a, u8> {
            match terrain {
                T::Open => Kind::Passable,
                T::Wall | T::Shut | T::Hinge => Kind::Blocking,
                T::Lock(gate) => Kind::Gate(gate),
            }
        }

        fn holds(&self, unit: &Unit<Vec<u8>>, key: &u8) -> bool {
            unit.ext.contains(key)
        }

        fn opened(&self, _: &T) -> T {
            T::Open
        }

        fn bump(&self, _: &Unit<Vec<u8>>, _: Cell, terrain: &T) -> Bump<T> {
            match terrain {
                T::Shut => Bump::Refuse,
                T::Hinge => Bump::Open(T::Open),
                _ => Bump::Stay,
            }
        }

        fn act(world: &mut World<Self>, by: UnitId, intent: Act) -> (Vec<u8>, Cost) {
            match intent {
                Act::Grant(key) => {
                    world.unit_mut(by).unwrap().ext.push(key);
                    (vec![key], Cost::Free)
                }
                Act::Wait => (Vec::new(), Cost::Turn),
            }
        }

        fn offers(world: &World<Self>, by: UnitId, cell: Cell) -> Vec<Act> {
            match world.unit(by) {
                Some(u) if u.cell == cell => vec![Act::Wait],
                _ => vec![Act::Grant(1), Act::Grant(2)],
            }
        }
    }

    /// A `w`×`h` open grid with `cells` set, and one keyless Unit at `at`.
    pub fn world(w: u16, h: u16, cells: &[(i32, i32, T)], at: Cell) -> (World<Keys>, UnitId) {
        let mut grid = Grid::new(w, h, T::Open);
        for (x, y, t) in cells {
            grid.set(Cell::new(*x, *y), t.clone());
        }
        let mut world = World::new(Keys, grid, 1);
        let id = world.spawn(Unit::new("u", 3, at, Vec::new()));
        (world, id)
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{Act, T, world};
    use super::*;

    const O: Cell = Cell::new(0, 0);
    const E: Cell = Cell::new(1, 0);

    fn step(w: &mut World<fixture::Keys>, id: UnitId) -> Vec<Event<u8>> {
        w.apply(id, Intent::Step(Dir::East))
    }

    #[test]
    fn a_step_onto_passable_moves_for_a_turn() {
        let (mut w, u) = world(2, 1, &[], O);
        assert_eq!(
            step(&mut w, u),
            [Event::Moved {
                unit: u,
                from: O,
                to: E
            }]
        );
        assert_eq!(w.unit(u).unwrap().cell, E);
        assert_eq!(w.turn(), 1);
    }

    #[test]
    fn a_bump_that_stays_costs_nothing() {
        let (mut w, u) = world(2, 1, &[(1, 0, T::Wall)], O);
        assert_eq!(step(&mut w, u), [Event::Bumped { unit: u, at: E }]);
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (O, 0));
    }

    #[test]
    fn a_refused_bump_costs_nothing_and_leaves_the_gate() {
        let (mut w, u) = world(2, 1, &[(1, 0, T::Lock(Gate::single(5)))], O);
        assert_eq!(step(&mut w, u), [Event::Refused { unit: u, at: E }]);
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (O, 0));
        assert_eq!(w.grid().get(E), Some(&T::Lock(Gate::single(5))));
    }

    /// One Intent: opened then moved, for one Turn.
    #[test]
    fn an_open_bump_opens_then_passes_for_one_turn() {
        let (mut w, u) = world(2, 1, &[(1, 0, T::Lock(Gate::single(5)))], O);
        w.unit_mut(u).unwrap().ext.push(5);
        let want = [
            Event::Opened { unit: u, at: E },
            Event::Moved {
                unit: u,
                from: O,
                to: E,
            },
        ];
        assert_eq!(step(&mut w, u), want);
        assert_eq!(w.grid().get(E), Some(&T::Open));
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (E, 1));
    }

    /// The Blocking hook, not just gates: refuse is free, open passes for one Turn.
    #[test]
    fn a_blocking_bump_hook_refuses_or_opens_and_passes() {
        let (mut w, u) = world(2, 1, &[(1, 0, T::Shut)], O);
        assert_eq!(step(&mut w, u), [Event::Refused { unit: u, at: E }]);
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (O, 0));
        let (mut w, u) = world(2, 1, &[(1, 0, T::Hinge)], O);
        let moved = Event::Moved {
            unit: u,
            from: O,
            to: E,
        };
        assert_eq!(step(&mut w, u), [Event::Opened { unit: u, at: E }, moved]);
        assert_eq!(w.grid().get(E), Some(&T::Open), "the hook's terrain lands");
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (E, 1));
    }

    /// Travel routes through any gate `passes` opens, and `step` walks it.
    #[test]
    fn travel_and_step_agree_on_gates() {
        let lock = T::Lock(Gate::single(5));
        let (mut w, u) = world(3, 1, &[(1, 0, lock)], O);
        w.unit_mut(u).unwrap().ext.push(5);
        let to = Cell::new(2, 0);
        for _ in 0..4 {
            let Some(dir) = crate::path::next_step(&w, u, to) else {
                break;
            };
            w.apply(u, Intent::Step(dir));
        }
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (to, 2));
    }

    #[test]
    fn a_step_off_the_grid_does_nothing() {
        let (mut w, u) = world(1, 1, &[], O);
        assert_eq!(step(&mut w, u), []);
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (O, 0));
    }

    #[test]
    fn a_solid_unit_is_bumped_and_a_non_solid_one_is_walked_onto() {
        let (mut w, u) = world(2, 1, &[], O);
        let other = w.spawn(Unit::new("v", 1, E, Vec::new()));
        assert_eq!(step(&mut w, u), [Event::Bumped { unit: u, at: E }]);
        assert_eq!((w.unit(u).unwrap().cell, w.turn()), (O, 0));
        w.unit_mut(other).unwrap().solid = false;
        assert_eq!(
            step(&mut w, u),
            [Event::Moved {
                unit: u,
                from: O,
                to: E
            }]
        );
    }

    #[test]
    fn a_game_intent_runs_its_hook_and_pays_its_cost() {
        let (mut w, u) = world(1, 1, &[], O);
        assert_eq!(w.apply(u, Intent::Game(Act::Grant(9))), [Event::Game(9)]);
        assert_eq!((w.unit(u).unwrap().ext.clone(), w.turn()), (vec![9], 0));
        assert_eq!(w.apply(u, Intent::Game(Act::Wait)), []);
        assert_eq!(w.turn(), 1);
    }

    #[test]
    fn an_unknown_unit_does_nothing() {
        let (mut w, _) = world(2, 1, &[], O);
        assert_eq!(step(&mut w, UnitId(9)), []);
        assert_eq!(w.turn(), 0);
    }

    #[test]
    fn the_seed_drives_the_world_rng() {
        let mut w = World::new(fixture::Keys, Grid::new(1, 1, T::Open), 3);
        let mut want = Pcg32::new(3, 0);
        assert_eq!(w.rng().next_u32(), want.next_u32());
    }
}
