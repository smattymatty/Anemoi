//! Replay fitness: the same scripted Intents from the same seed give the same
//! Events and state hash, twice and on every platform. The wasm32 CI job runs
//! this file under `wasm-bindgen-test` against the same `GOLDEN`.

use std::hash::{Hash, Hasher};

use aeolus::{
    Cell, Cost, Dir, Event, Gate, Grid, Intent, Kind, Rules, StableHasher, Unit, UnitId, World,
};

/// The native run's hash of its Events and final state. A deliberate rule
/// change updates it in the same diff.
const GOLDEN: u64 = 0x3ae1_1cb6_ad97_a1b7;
const SEED: u64 = 2026;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Tile {
    Open,
    Wall,
    Lock(Gate<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Act {
    /// Draw a token from the world RNG, for a Turn.
    Draw,
}

struct Tokens;

impl Rules for Tokens {
    type Ext = Vec<u8>;
    type Terrain = Tile;
    type Condition = u8;
    type Intent = Act;
    type Event = (UnitId, u8);

    fn kind<'a>(&'a self, t: &'a Tile) -> Kind<'a, u8> {
        match t {
            Tile::Open => Kind::Passable,
            Tile::Wall => Kind::Blocking,
            Tile::Lock(gate) => Kind::Gate(gate),
        }
    }

    fn holds(&self, unit: &Unit<Vec<u8>>, token: &u8) -> bool {
        unit.ext.contains(token)
    }

    fn opened(&self, _: &Tile) -> Tile {
        Tile::Open
    }

    fn act(world: &mut World<Self>, by: UnitId, _: Act) -> (Vec<(UnitId, u8)>, Cost) {
        let token = world.rng().below(4) as u8;
        world.unit_mut(by).unwrap().ext.push(token);
        (vec![(by, token)], Cost::Turn)
    }
}

/// One line of a script: an Intent, or a Travel leg resolved by `next_step`.
enum Line {
    Do(u32, Intent<Act>),
    Travel(u32, Cell, usize),
}

fn c(x: i32, y: i32) -> Cell {
    Cell::new(x, y)
}

/// 6x4, y up. `#` walls, `L` a lock any token opens, `K` one needing 0 and 3.
///
///   . . . # . .
///   . # . L . .
///   . # . # K .
///   a . . # . b
fn world() -> World<Tokens> {
    let mut grid = Grid::new(6, 4, Tile::Open);
    for at in [c(3, 0), c(1, 1), c(3, 1), c(1, 2), c(3, 3)] {
        grid.set(at, Tile::Wall);
    }
    let any = Gate::new((0..4).map(|t| vec![t]).collect());
    grid.set(c(3, 2), Tile::Lock(any));
    grid.set(c(4, 1), Tile::Lock(Gate::new(vec![vec![0, 3]])));
    let mut w = World::new(Tokens, grid, SEED);
    w.spawn(Unit::new("a", 5, c(0, 0), Vec::new()));
    w.spawn(Unit::new("b", 5, c(5, 0), Vec::new()));
    w
}

fn script() -> Vec<Line> {
    use Dir::*;
    use Line::*;
    let step = |u, d| Do(u, Intent::Step(d));
    vec![
        // Two equal routes each way round the left loop: Dir order picks one.
        Travel(0, c(2, 3), 8),
        Travel(0, c(0, 0), 8),
        step(0, West),
        step(0, East),
        step(0, East),
        step(0, East),
        Travel(0, c(2, 2), 8),
        step(0, East),
        Do(0, Intent::Game(Act::Draw)),
        step(0, East),
        Travel(1, c(4, 2), 8),
        Travel(0, c(4, 2), 8),
        step(1, West),
        Do(1, Intent::Game(Act::Draw)),
        Do(1, Intent::Game(Act::Draw)),
        Do(1, Intent::Game(Act::Draw)),
        Travel(1, c(0, 3), 12),
    ]
}

fn run() -> (Vec<Event<(UnitId, u8)>>, u64) {
    let mut w = world();
    let mut events = Vec::new();
    for line in script() {
        match line {
            Line::Do(u, intent) => events.extend(w.apply(UnitId(u), intent)),
            Line::Travel(u, to, legs) => {
                for _ in 0..legs {
                    let Some(dir) = aeolus::next_step(&w, UnitId(u), to) else {
                        break;
                    };
                    events.extend(w.apply(UnitId(u), Intent::Step(dir)));
                }
            }
        }
    }
    (events, w.state_hash())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_script_and_seed_replay_identically() {
    let (events, hash) = run();
    assert_eq!(run(), (events.clone(), hash));
    let has = |f: fn(&Event<(UnitId, u8)>) -> bool| events.iter().any(f);
    assert!(has(|e| matches!(e, Event::Moved { .. })));
    assert!(has(|e| matches!(e, Event::Bumped { .. })));
    assert!(has(|e| matches!(e, Event::Opened { .. })));
    assert!(has(|e| matches!(e, Event::Refused { .. })));
    assert!(has(|e| matches!(e, Event::Game(_))));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn events_and_state_hash_to_the_golden_value() {
    let mut h = StableHasher::default();
    run().hash(&mut h);
    let hash = h.finish();
    assert_eq!(hash, GOLDEN, "now {hash:#018x}");
}
