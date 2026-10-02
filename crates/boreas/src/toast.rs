//! A floating toast: short text over a cell, in world space, that rises and fades.
//! One at a time; a new one replaces the old.

use aeolus::Cell;
use bevy::prelude::*;

use crate::intent::Bindings;
use crate::tile_menu::Refused;
use crate::tween::MAX_DT;
use crate::ui::UiTheme;

/// Seconds a toast lives, and how far it rises in that time (world px).
pub const LIFE: f32 = 0.9;
pub const RISE: f32 = 12.0;
/// Above the outlines.
const Z: f32 = 60.0;
const SIZE: f32 = 12.0;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Toast {
    from: Vec2,
    color: Color,
    age: f32,
}

/// A toast just above `cell`'s top edge, so it clears a menu beside the cell.
pub fn toast(text: impl Into<String>, cell: Cell, px: f32, color: Color) -> impl Bundle {
    let from = Vec2::new(
        (cell.x as f32 + 0.5) * px,
        (cell.y + 1) as f32 * px + SIZE / 2.0,
    );
    (
        Toast {
            from,
            color,
            age: 0.0,
        },
        Text2d::new(text),
        TextFont::from_font_size(SIZE),
        TextColor(color),
        Transform::from_translation(from.extend(Z)),
    )
}

/// A refusal replaces any toast with its reason over its cell, in `refused`.
pub fn on_refused<C: Send + Sync + 'static>(
    mut refused: MessageReader<Refused<C>>,
    (bindings, theme): (Res<Bindings>, Res<UiTheme>),
    toasts: Query<Entity, With<Toast>>,
    mut commands: Commands,
) {
    let Some(r) = refused.read().last() else {
        return;
    };
    toasts.iter().for_each(|e| commands.entity(e).despawn());
    let (text, px) = (r.reason.clone(), bindings.cell_px);
    commands.spawn(toast(text, r.cell, px, theme.refused));
}

/// The showing toast's text, for dumps.
pub fn shown(w: &World) -> Option<String> {
    let mut q = w.try_query_filtered::<&Text2d, With<Toast>>()?;
    q.iter(w).next().map(|t| t.0.clone())
}

/// Rises linearly; holds its colour, then fades (alpha `1 - k²`); goes at `LIFE`.
pub fn rise(
    time: Res<Time>,
    mut commands: Commands,
    mut toasts: Query<(Entity, &mut Toast, &mut Transform, &mut TextColor)>,
) {
    for (e, mut toast, mut at, mut color) in &mut toasts {
        toast.age += time.delta_secs().min(MAX_DT);
        let k = toast.age / LIFE;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        at.translation = (toast.from + Vec2::Y * RISE * k).extend(Z);
        color.0 = toast.color.with_alpha(toast.color.alpha() * (1.0 - k * k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    fn frame(world: &mut World, secs: f32) {
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(secs));
        world.insert_resource(time);
        world.run_system_once(rise).unwrap();
    }

    #[test]
    fn a_toast_starts_above_its_cell_rises_fades_and_goes() {
        let mut world = World::new();
        let red = Color::srgb(1.0, 0.0, 0.0);
        let e = world.spawn(toast("no", Cell::new(2, 3), 16.0, red)).id();
        let at = |w: &World| w.get::<Transform>(e).unwrap().translation;
        assert_eq!(
            at(&world),
            Vec3::new(40.0, 70.0, Z),
            "just over the top edge"
        );
        (0..5).for_each(|_| frame(&mut world, LIFE / 10.0));
        assert!((at(&world) - Vec3::new(40.0, 76.0, Z)).length() < 1e-4);
        let alpha = world.get::<TextColor>(e).unwrap().0.alpha();
        assert!((alpha - 0.75).abs() < 1e-4, "alpha {alpha}");
        (0..4).for_each(|_| frame(&mut world, LIFE / 10.0));
        assert!(world.get_entity(e).is_ok(), "still there just before LIFE");
        frame(&mut world, LIFE / 5.0);
        assert!(world.get_entity(e).is_err(), "gone at LIFE");
    }

    /// A hitch (one long frame) ages a toast by at most `MAX_DT`, so it still shows.
    #[test]
    fn a_hitch_does_not_swallow_the_toast() {
        let mut world = World::new();
        let e = world
            .spawn(toast("no", Cell::new(0, 0), 16.0, Color::WHITE))
            .id();
        frame(&mut world, 5.0);
        assert!(
            world.get_entity(e).is_ok(),
            "still showing after a 5 s frame"
        );
    }
}
