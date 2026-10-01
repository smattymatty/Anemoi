# Anemoi

A Rust engine for turn-based, top-down games: two crates and a test runner.

- **Aeolus** (`crates/aeolus`): the simulation. Pure Rust, no Bevy, deterministic:
  integers only, its own seeded RNG, ordered maps, no clock. It owns the world as
  plain data: a grid, Units, gates. Intents go in, Events come out. The same
  Intents from the same seed give the same Events on every platform, so a turn log
  replays exactly.
- **Boreas** (`crates/boreas`): the Bevy front end. Menus, turning keys and mouse
  into Intents, and pacing the Events Aeolus returns. It never decides a rule.
- **anemoi-sandbox** (`crates/anemoi-sandbox`): play a Boreas game from a TOML
  script, headless or windowed, and check its declared state. Its own crate, so
  UI-only games never compile it.

Your game supplies its own data on each Unit (`Unit<G>`) and its rules through
the `Rules` trait: what a cell is, what an opened gate becomes, and how its own Intents play
out.

## Example

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

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
