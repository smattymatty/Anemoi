//! Presentation-only tweens of an entity's local `Transform` xy.
//! One `Tween` per entity, so inserting a new one replaces the old (per channel).

use crate::inspect::{self, InspectApp};
use bevy::prelude::*;

/// Frame dt clamp, so a hitch doesn't skip a tween.
pub const MAX_DT: f32 = 0.1;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: Vec2,
    to: Vec2,
    secs: f32,
    ease: EaseFunction,
    elapsed: f32,
    then: Option<(Vec2, f32, EaseFunction)>,
}

impl Tween {
    pub fn new(from: Vec2, to: Vec2, secs: f32, ease: EaseFunction) -> Self {
        Self {
            from,
            to,
            secs,
            ease,
            elapsed: 0.0,
            then: None,
        }
    }

    /// A second leg, from where this one ends.
    pub fn then(self, to: Vec2, secs: f32, ease: EaseFunction) -> Self {
        Self {
            then: Some((to, secs, ease)),
            ..self
        }
    }
}

pub fn run_tweens(
    time: Res<Time>,
    mut commands: Commands,
    mut tweens: Query<(Entity, &mut Tween, &mut Transform)>,
) {
    let dt = time.delta_secs().min(MAX_DT);
    for (entity, mut tween, mut tf) in &mut tweens {
        tween.elapsed += dt;
        let k = (tween.elapsed / tween.secs).min(1.0);
        let at = EasingCurve::new(tween.from, tween.to, tween.ease).sample_clamped(k);
        tf.translation = at.extend(tf.translation.z);
        if k < 1.0 {
            continue;
        }
        match tween.then {
            Some((to, secs, ease)) => *tween = Tween::new(tween.to, to, secs, ease),
            None => {
                commands.entity(entity).remove::<Tween>();
            }
        }
    }
}

pub struct TweenPlugin;

impl Plugin for TweenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, run_tweens).inspect("tweens", |w| {
            format!("active={}", inspect::count::<With<Tween>>(w))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    /// One frame of `secs` over a world holding one tweened entity.
    fn frame(world: &mut World, secs: f32) {
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(secs));
        world.insert_resource(time);
        world.run_system_once(run_tweens).unwrap();
    }

    fn x(world: &World, e: Entity) -> f32 {
        world.get::<Transform>(e).unwrap().translation.x
    }

    #[test]
    fn a_hitch_frame_counts_as_a_tenth_of_a_second() {
        let mut world = World::new();
        let tween = Tween::new(Vec2::ZERO, Vec2::X * 10.0, 0.4, EaseFunction::Linear);
        let e = world.spawn((tween, Transform::default())).id();
        frame(&mut world, 1.0);
        assert!((x(&world, e) - 2.5).abs() < 1e-4, "x = {}", x(&world, e));
        assert!(world.get::<Tween>(e).is_some(), "not skipped to the end");
    }

    #[test]
    fn second_leg_runs_from_the_first_ends_then_the_tween_goes() {
        let mut world = World::new();
        let tween = Tween::new(Vec2::ZERO, Vec2::X * 4.0, 0.05, EaseFunction::Linear).then(
            Vec2::ZERO,
            0.1,
            EaseFunction::Linear,
        );
        let e = world
            .spawn((tween, Transform::from_xyz(0.0, 0.0, 7.0)))
            .id();
        frame(&mut world, 0.06);
        assert_eq!(x(&world, e), 4.0, "first leg lands");
        frame(&mut world, 0.05);
        assert!((x(&world, e) - 2.0).abs() < 1e-4, "x = {}", x(&world, e));
        frame(&mut world, 0.06);
        assert_eq!(
            world.get::<Transform>(e).unwrap().translation,
            Vec3::Z * 7.0
        );
        assert!(world.get::<Tween>(e).is_none(), "done, input unlocks");
    }
}
