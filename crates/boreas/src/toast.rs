//! A floating toast: short text over a cell that rises and fades. It sits in the
//! world but keeps one screen size at any zoom, inside the view. One at a time.

use aeolus::Cell;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;

use crate::intent::Bindings;
use crate::tile_menu::Refused;
use crate::tween::MAX_DT;
use crate::ui::{UiMetrics, UiTheme};

/// Seconds a toast lives, and how far it rises in that time (world px).
pub const LIFE: f32 = 0.9;
pub const RISE: f32 = 12.0;
/// Above the outlines.
const Z: f32 = 60.0;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Toast {
    from: Vec2,
    color: Color,
    age: f32,
}

/// A toast standing on `cell`'s top edge, so it clears a menu beside the cell.
/// `size` is screen px.
pub fn toast(text: impl Into<String>, cell: Cell, px: f32, size: f32, color: Color) -> impl Bundle {
    let from = Vec2::new((cell.x as f32 + 0.5) * px, (cell.y + 1) as f32 * px);
    (
        Toast {
            from,
            color,
            age: 0.0,
        },
        Text2d::new(text),
        TextFont::from_font_size(size),
        TextColor(color),
        Anchor::BOTTOM_CENTER,
        Transform::from_translation(from.extend(Z)),
    )
}

/// A refusal replaces any toast with its reason over its cell, in `refused`.
pub fn on_refused<C: Send + Sync + 'static>(
    mut refused: MessageReader<Refused<C>>,
    (bindings, theme, metrics): (Res<Bindings>, Res<UiTheme>, Res<UiMetrics>),
    toasts: Query<Entity, With<Toast>>,
    mut commands: Commands,
) {
    let Some(r) = refused.read().last() else {
        return;
    };
    toasts.iter().for_each(|e| commands.entity(e).despawn());
    let (text, px) = (r.reason.clone(), bindings.cell_px);
    commands.spawn(toast(text, r.cell, px, metrics.row_text, theme.refused));
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

/// Scales each toast by the camera's world px per screen px, so zoom never grows
/// it, then keeps it inside the view. After `rise`, before transforms propagate.
pub fn fit(
    cameras: Query<(&Camera, &Projection, &GlobalTransform)>,
    mut toasts: Query<(&mut Transform, &TextLayoutInfo), With<Toast>>,
) {
    let Some((per_px, view)) = cameras.iter().find_map(|(camera, projection, eye)| {
        let Projection::Orthographic(o) = projection else {
            return None;
        };
        let screen = camera
            .logical_viewport_size()
            .filter(|_| camera.is_active)?;
        let at = eye.translation().truncate();
        Some((
            o.area.height() / screen.y,
            Rect::from_corners(at + o.area.min, at + o.area.max),
        ))
    }) else {
        return;
    };
    for (mut at, layout) in &mut toasts {
        let to = inside(at.translation.truncate(), layout.size * per_px, view);
        at.translation = to.extend(at.translation.z);
        at.scale = Vec3::new(per_px, per_px, 1.0);
    }
}

/// A bottom-centred box of `size` at `at`, moved the least to lie inside `view`.
fn inside(at: Vec2, size: Vec2, view: Rect) -> Vec2 {
    let x =
        at.x.min(view.max.x - size.x / 2.0)
            .max(view.min.x + size.x / 2.0);
    let y = at.y.min(view.max.y - size.y).max(view.min.y);
    Vec2::new(x, y)
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
        let e = world
            .spawn(toast("no", Cell::new(2, 3), 16.0, 13.0, red))
            .id();
        let at = |w: &World| w.get::<Transform>(e).unwrap().translation;
        assert_eq!(at(&world), Vec3::new(40.0, 64.0, Z), "on the top edge");
        (0..5).for_each(|_| frame(&mut world, LIFE / 10.0));
        assert!((at(&world) - Vec3::new(40.0, 70.0, Z)).length() < 1e-4);
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
            .spawn(toast("no", Cell::new(0, 0), 16.0, 13.0, Color::WHITE))
            .id();
        frame(&mut world, 5.0);
        assert!(
            world.get_entity(e).is_ok(),
            "still showing after a 5 s frame"
        );
    }

    /// Inside the view, it stays put; past an edge, it moves back just inside.
    #[test]
    fn a_toast_is_kept_inside_the_view() {
        let view = Rect::new(0.0, 0.0, 100.0, 50.0);
        let size = Vec2::new(40.0, 10.0);
        assert_eq!(
            inside(Vec2::new(50.0, 20.0), size, view),
            Vec2::new(50.0, 20.0)
        );
        assert_eq!(
            inside(Vec2::new(50.0, 45.0), size, view),
            Vec2::new(50.0, 40.0),
            "top"
        );
        assert_eq!(
            inside(Vec2::new(5.0, 20.0), size, view),
            Vec2::new(20.0, 20.0),
            "left"
        );
        assert_eq!(
            inside(Vec2::new(95.0, 20.0), size, view),
            Vec2::new(80.0, 20.0),
            "right"
        );
        assert_eq!(
            inside(Vec2::new(50.0, -5.0), size, view),
            Vec2::new(50.0, 0.0),
            "bottom"
        );
        let wide = Vec2::new(140.0, 10.0);
        assert_eq!(
            inside(Vec2::new(50.0, 20.0), wide, view).x,
            70.0,
            "too wide: left edge wins"
        );
    }

    /// Zoomed to 0.5 world px per screen px, the toast scales by 0.5: the same
    /// text size on screen, standing on its cell's edge.
    #[test]
    fn a_toast_keeps_its_screen_size_at_any_zoom() {
        let mut world = World::new();
        let mut cam = Camera::default();
        cam.computed.target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: UVec2::new(160, 80),
            scale_factor: 1.0,
        });
        let mut o = OrthographicProjection::default_2d();
        o.area = Rect::new(-40.0, -20.0, 40.0, 20.0);
        world.spawn((cam, Projection::Orthographic(o), GlobalTransform::default()));
        let e = world
            .spawn(toast("no", Cell::new(0, 0), 16.0, 13.0, Color::WHITE))
            .id();

        world.run_system_once(fit).unwrap();

        assert_eq!(
            world.get::<Transform>(e).unwrap().scale,
            Vec3::new(0.5, 0.5, 1.0)
        );
        assert_eq!(
            world.get::<TextFont>(e).unwrap(),
            &TextFont::from_font_size(13.0)
        );
        assert_eq!(world.get::<Anchor>(e).unwrap(), &Anchor::BOTTOM_CENTER);
    }
}
