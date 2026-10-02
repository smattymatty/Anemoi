# Anemoi

Two Rust libraries for turn-based, top-down 2D games in Bevy, roguelikes first.

- **Aeolus** is the simulation: the grid, the Units, the rules, every Turn. It doesn't need Bevy.
- **Boreas** connects Aeolus to Bevy and handles the presentation: input, animation, menus.

Because Aeolus doesn't need Bevy, the whole game can be simulated headless, on a server or in CI. **anemoi-sandbox** is the test framework: a TOML file scripts any action, frame by frame, and checks the state the game declares. Today a script starts from the game's startup. With Turn Logs it will start from any recorded Turn.

LDtk maps are planned for Boreas. Today each game loads its own.

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
