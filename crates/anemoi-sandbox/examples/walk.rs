//! The smallest game the runner drives: one Unit on open ground, no art, and a
//! wall line at x=6, with a shut gate at (6,4), that cuts off the last column.
//! A click Travels; `1` hands clicks to the Tile Menu and back; `L` toggles the
//! tile cursor; `V` turns the darkness (radius 4) on, as everything starts lit.
//!   cargo run -p anemoi-sandbox --example walk -- scripts/walk.toml

use aeolus::{Cell, Cost, Gate, Grid, Kind, Rules, Unit, UnitId, World};
use bevy::prelude::*;
use boreas::intent::{ClickMode, IntentPlugin};
use boreas::outline::OutlinePlugin;
use boreas::pace::Sim;
use boreas::tile_menu::{Labels, Refusal, Row, TileMenuPlugin, ToggleTileCursor};
use boreas::vision::{Vision, VisionPlugin};

struct Ground;

#[derive(Clone)]
enum Terrain {
    Open,
    Wall,
    Gate(Gate<Key>),
}

/// The Condition the gate wants; the walker never holds it.
#[derive(Clone)]
struct Key;

impl Rules for Ground {
    type Ext = ();
    type Terrain = Terrain;
    type Condition = Key;
    type Intent = ();
    type Event = ();

    fn kind<'a>(&'a self, terrain: &'a Terrain) -> Kind<'a, Key> {
        match terrain {
            Terrain::Open => Kind::Passable,
            Terrain::Wall => Kind::Blocking,
            Terrain::Gate(gate) => Kind::Gate(gate),
        }
    }
    fn holds(&self, _: &Unit<()>, _: &Key) -> bool {
        false
    }
    fn opened(&self, _: &Terrain) -> Terrain {
        Terrain::Open
    }
    fn act(_: &mut World<Self>, _: UnitId, _: ()) -> (Vec<()>, Cost) {
        (vec![], Cost::Free)
    }
}

impl Labels for Ground {
    fn label(_: &World<Self>, _: Cell, row: &Row<()>) -> String {
        match row {
            Row::Intent(aeolus::Intent::Step(_)) | Row::Travel(_) => "Move",
            Row::Intent(aeolus::Intent::Game(())) => "Wait",
            Row::Look => "Look",
        }
        .into()
    }
    fn reason(_: &World<Self>, refusal: &Refusal<Key>) -> String {
        match refusal {
            Refusal::Gate(_) => "locked".into(),
            Refusal::NoRoute => "no route".into(),
        }
    }
}

/// `1`: clicks open the Tile Menu, or Travel again. `L`: the tile cursor. `V`: the dev light.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    (mut clicks, mut vision): (ResMut<ClickMode>, ResMut<Vision>),
    mut toggle: MessageWriter<ToggleTileCursor>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        *clicks = match *clicks {
            ClickMode::Travel => ClickMode::TileMenu,
            ClickMode::TileMenu => ClickMode::Travel,
        };
    }
    if keys.just_pressed(KeyCode::KeyL) {
        toggle.write(ToggleTileCursor);
    }
    if keys.just_pressed(KeyCode::KeyV) {
        vision.all = !vision.all;
    }
}

struct Walk;

impl Plugin for Walk {
    fn build(&self, app: &mut App) {
        let mut grid = Grid::new(8, 8, Terrain::Open);
        for y in 0..8 {
            grid.set(Cell::new(6, y), Terrain::Wall);
        }
        grid.set(Cell::new(6, 4), Terrain::Gate(Gate::single(Key)));
        let mut world = World::new(Ground, grid, 7);
        let player = world.spawn(Unit::new("walker", 1, Cell::new(0, 0), ()));
        app.add_plugins((
            IntentPlugin::<Ground>::default(),
            OutlinePlugin::<Ground>::default(),
            // Click-to-Travel by default, so `pointer.toml` keeps its meaning.
            TileMenuPlugin::<Ground>::new(ClickMode::Travel),
            // All lit, so the older scripts and goldens keep their frames.
            VisionPlugin::<Ground>::new(4).all(true),
        ))
        .insert_resource(Sim::new(world, player))
        .add_systems(Startup, |mut c: Commands| {
            c.spawn(Camera2d);
        })
        .add_systems(Update, keys);
    }
}

impl anemoi_sandbox::Config for Walk {
    fn game() -> impl Plugin {
        Walk
    }
}

fn main() {
    anemoi_sandbox::run::<Walk>();
}
