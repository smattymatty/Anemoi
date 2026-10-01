//! The harness's one window into a game: each plugin declares a dump of its
//! own state, and the sandbox reads only these. It never queries internals.

use bevy::prelude::*;

/// One named, read-only view of the world, as a short human-readable line.
pub type Dump = fn(&World) -> String;

#[derive(Resource, Default, Clone)]
struct Dumps(Vec<(&'static str, Dump)>);

pub trait InspectApp {
    /// Declare what `name` shows the harness.
    fn inspect(&mut self, name: &'static str, dump: Dump) -> &mut Self;
}

impl InspectApp for App {
    fn inspect(&mut self, name: &'static str, dump: Dump) -> &mut Self {
        self.init_resource::<Dumps>();
        self.world_mut()
            .resource_mut::<Dumps>()
            .0
            .push((name, dump));
        self
    }
}

/// Entities matching `F`; zero when its components were never spawned.
pub fn count<F: bevy::ecs::query::QueryFilter>(world: &World) -> usize {
    world
        .try_query_filtered::<(), F>()
        .map_or(0, |mut q| q.iter(world).count())
}

/// Every declared dump, in plugin order.
pub fn snapshot(world: &World) -> Vec<(&'static str, String)> {
    let dumps = world.get_resource::<Dumps>().cloned().unwrap_or_default();
    dumps
        .0
        .into_iter()
        .map(|(name, dump)| (name, dump(world)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reads_every_declared_dump_in_order() {
        #[derive(Resource)]
        struct Hp(u32);
        let mut app = App::new();
        app.insert_resource(Hp(7))
            .inspect("a", |_| "one".into())
            .inspect("b", |w| format!("hp={}", w.resource::<Hp>().0));
        let got = snapshot(app.world());
        assert_eq!(got, [("a", "one".into()), ("b", "hp=7".into())]);
    }
}
