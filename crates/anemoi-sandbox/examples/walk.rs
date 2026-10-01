//! The smallest game the runner drives: one Unit on open ground, no art.
//!   cargo run -p anemoi-sandbox --example walk -- scripts/walk.toml

use aeolus::{Cell, Cost, Grid, Kind, Rules, Unit, UnitId, World};
use bevy::prelude::*;
use boreas::intent::IntentPlugin;
use boreas::pace::Sim;

struct Ground;

impl Rules for Ground {
    type Ext = ();
    type Terrain = ();
    type Condition = ();
    type Intent = ();
    type Event = ();

    fn kind<'a>(&'a self, _: &'a ()) -> Kind<'a, ()> {
        Kind::Passable
    }
    fn holds(&self, _: &Unit<()>, _: &()) -> bool {
        false
    }
    fn opened(&self, _: &()) {}
    fn act(_: &mut World<Self>, _: UnitId, _: ()) -> (Vec<()>, Cost) {
        (vec![], Cost::Free)
    }
}

struct Walk;

impl Plugin for Walk {
    fn build(&self, app: &mut App) {
        let mut world = World::new(Ground, Grid::new(8, 8, ()), 7);
        let player = world.spawn(Unit::new("walker", 1, Cell::new(0, 0), ()));
        app.add_plugins(IntentPlugin::<Ground>::default())
            .insert_resource(Sim::new(world, player))
            .add_systems(Startup, |mut c: Commands| {
                c.spawn(Camera2d);
            });
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
