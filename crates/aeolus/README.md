# Aeolus

The deterministic, Bevy-free simulation core of Anemoi. Integers only, its own
seeded RNG (`Pcg32`), ordered maps, no clock: the same Intents from the same seed
give the same Events everywhere.

- `World<G>` holds a `Grid` of terrain, `Unit`s and the Turn count.
  `world.apply(unit, intent)` takes an `Intent` (a `Step(Dir)` or the game's own)
  and returns `Event`s: moved, bumped, opened, refused, or the game's own.
- A step that moves costs a Turn. A bump costs none, unless the bump opens the
  cell and passes into it: then one Intent gives opened then moved, for one Turn.
- Your game implements `Rules`: what terrain is (passable, blocking, a `Gate`),
  which conditions a Unit holds, what an opened gate becomes, what bumping a
  blocking cell does (`Refuse`, `Stay`, `Open`), and how its own Intents play out.
  The engine decides gates itself, so `next_step` and `apply` always agree.
- Only setup and `Rules::act` may use `grid_mut`, `unit_mut` or `rng`: a change
  made anywhere else is not in the Intent log and breaks replay.
- A `Gate` is a list of routes, each a list of conditions; any full route opens
  it, and `missing` names what the closest route still lacks.
- `next_step(&world, unit, to)` is a deterministic BFS for travel. A blocked
  target next to the Unit is still a target: the step becomes a bump.
- `offers(&world, unit, cell)` lists a cell's Offers as Intents: next to the
  Unit, `Step` first, then `Rules::offers` (the game's own, default none); on its
  own cell, the game's only; distant cells, nothing.
- `world.state_hash()` folds grid, Units, Turn and RNG into one `u64` through
  `StableHasher` (FNV-1a, fixed-width little-endian), so native and wasm agree.
  `tests/replay.rs` pins it; `tests/fitness.rs` bans floats, hash maps, clocks
  and thread RNGs from `src/`.
- `palette` parses any GIMP `.gpl` text into a `Palette` (`at`, `contains`,
  `nearest`, integer squared distance). The engine ships no palette.

```rust
use aeolus::{Cell, Cost, Dir, Event, Grid, Intent, Kind, Rules, Unit, UnitId, World};

#[derive(Clone, PartialEq)]
enum Tile { Open, Wall }

struct Game;

impl Rules for Game {
    type Ext = ();
    type Terrain = Tile;
    type Condition = ();
    type Intent = ();
    type Event = ();

    fn kind<'a>(&'a self, t: &'a Tile) -> Kind<'a, ()> {
        if *t == Tile::Wall { Kind::Blocking } else { Kind::Passable }
    }
    fn holds(&self, _: &Unit<()>, _: &()) -> bool { false }
    fn opened(&self, _: &Tile) -> Tile { Tile::Open }
    fn act(_: &mut World<Self>, _: UnitId, _: ()) -> (Vec<()>, Cost) { (vec![], Cost::Free) }
}

let mut grid = Grid::new(3, 1, Tile::Open);
grid.set(Cell::new(2, 0), Tile::Wall);
let mut world = World::new(Game, grid, 42);
let hero = world.spawn(Unit::new("hero", 10, Cell::new(0, 0), ()));
assert!(matches!(world.apply(hero, Intent::Step(Dir::East))[..], [Event::Moved { .. }]));
assert!(matches!(world.apply(hero, Intent::Step(Dir::East))[..], [Event::Bumped { .. }]));
assert_eq!(world.turn(), 1);
```

Bevy conversion, input and pacing belong in Boreas.
