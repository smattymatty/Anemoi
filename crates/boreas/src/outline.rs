//! Cell outlines in world space: a faint one under the pointer, a stronger one
//! on the Tile Menu's cell. Four thin `Sprite`s each, so they follow the camera.

use std::marker::PhantomData;

use aeolus::Cell;
use bevy::prelude::*;

use crate::cursor::Cursor;
use crate::inspect::InspectApp;
use crate::intent::{Bindings, cell_at};
use crate::pace::{Game, Sim};
use crate::tile_menu::Refused;
use crate::tween::MAX_DT;
use crate::ui::UiTheme;

/// The grid cell under the pointer, or `None` off the grid or the window.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover(pub Option<Cell>);

/// The cell the Tile Menu is open on; whoever opens it sets this.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Target(pub Option<Cell>);

/// Seconds left of the target outline's refused flash; a `Refused` sets it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Flash(pub f32);

/// Seconds the target outline flashes `refused`.
pub const FLASH: f32 = 0.25;

/// Which outline an entity draws.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outline {
    Hover,
    Target,
}

/// One side of an outline; its index orders top, bottom, left, right.
#[derive(Component)]
struct Edge(usize);

/// Above floor and Units, below the UI.
const Z: f32 = 50.0;

impl Outline {
    /// Thickness in screen pixels, so a zoomed camera never thickens it.
    fn width(self) -> f32 {
        match self {
            Outline::Hover => 1.0,
            Outline::Target => 1.0,
        }
    }

    fn color(self, theme: &UiTheme, flash: Flash) -> Color {
        match self {
            Outline::Hover => theme.hover,
            Outline::Target if flash.0 > 0.0 => theme.refused,
            Outline::Target => theme.target,
        }
    }
}

/// Each side's offset from the cell centre and size: inside the cell, so neighbours
/// never overlap, and the sides stop short of the corners, so a faint colour never doubles.
fn edges(px: f32, width: f32) -> [(Vec2, Vec2); 4] {
    let inset = (px - width) / 2.0;
    let (across, down) = (Vec2::new(px, width), Vec2::new(width, px - 2.0 * width));
    [
        (Vec2::new(0.0, inset), across),
        (Vec2::new(0.0, -inset), across),
        (Vec2::new(-inset, 0.0), down),
        (Vec2::new(inset, 0.0), down),
    ]
}

/// World units per screen pixel, from where the camera puts two world points one
/// unit apart; 1 when there is no camera to ask.
fn world_per_pixel(a: Option<Vec2>, b: Option<Vec2>) -> f32 {
    let span = a.zip(b).map_or(1.0, |(a, b)| (b.x - a.x).abs());
    if span > 0.0 { 1.0 / span } else { 1.0 }
}

/// The cell under world point `p`, if the grid holds it.
pub fn on_grid(p: Vec2, px: f32, width: i32, height: i32) -> Option<Cell> {
    let cell = cell_at(p, px);
    let inside = (0..width).contains(&cell.x) && (0..height).contains(&cell.y);
    inside.then_some(cell)
}

fn hover<G: Game>(
    cursor: Res<Cursor>,
    bindings: Res<Bindings>,
    sim: Res<Sim<G>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut hover: ResMut<Hover>,
) {
    let cell = pointed(&cursor, &camera, bindings.cell_px, sim.world().grid());
    hover.set_if_neq(Hover(cell));
}

/// The grid cell under the pointer, through the one 2D camera.
pub fn pointed<T>(
    cursor: &Cursor,
    camera: &Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    px: f32,
    grid: &aeolus::Grid<T>,
) -> Option<Cell> {
    let (cam, at) = camera.single().ok()?;
    let p = cam.viewport_to_world_2d(at, cursor.0?).ok()?;
    on_grid(p, px, grid.width(), grid.height())
}

fn spawn(mut commands: Commands) {
    for outline in [Outline::Hover, Outline::Target] {
        commands
            .spawn((
                outline,
                Transform::from_xyz(0.0, 0.0, Z),
                Visibility::Hidden,
            ))
            .with_children(|sides| {
                for i in 0..4 {
                    sides.spawn((Edge(i), Sprite::default(), Transform::default()));
                }
            });
    }
}

/// Moves each outline onto its cell, hides it when there is none, and keeps its
/// sides sized to `cell_px` and coloured from the theme.
fn place(
    hover: Res<Hover>,
    (target, flash): (Res<Target>, Res<Flash>),
    bindings: Res<Bindings>,
    theme: Res<UiTheme>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut outlines: Query<(&Outline, &mut Transform, &mut Visibility, &Children)>,
    mut sides: Query<(&Edge, &mut Sprite, &mut Transform), Without<Outline>>,
) {
    let px = bindings.cell_px;
    let to_screen = |p: Vec3| {
        camera
            .single()
            .ok()
            .and_then(|(c, eye)| c.world_to_viewport(eye, p).ok())
    };
    let per_pixel = world_per_pixel(to_screen(Vec3::ZERO), to_screen(Vec3::X));
    for (outline, mut at, mut shown, children) in &mut outlines {
        let cell = match outline {
            Outline::Hover => hover.0,
            Outline::Target => target.0,
        };
        *shown = if cell.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(c) = cell {
            let centre = (Vec2::new(c.x as f32, c.y as f32) + 0.5) * px;
            at.translation = centre.extend(Z);
        }
        let geometry = edges(px, outline.width() * per_pixel);
        for child in children {
            if let Ok((edge, mut sprite, mut side)) = sides.get_mut(*child) {
                let (offset, size) = geometry[edge.0];
                sprite.color = outline.color(&theme, *flash);
                sprite.custom_size = Some(size);
                side.translation = offset.extend(0.0);
            }
        }
    }
}

/// Any refusal flashes the target outline.
pub fn flash_on_refused<C: Send + Sync + 'static>(
    mut refused: MessageReader<Refused<C>>,
    mut flash: ResMut<Flash>,
) {
    if refused.read().count() > 0 {
        flash.0 = FLASH;
    }
}

fn fade(time: Res<Time>, mut flash: ResMut<Flash>) {
    if flash.0 > 0.0 {
        flash.0 = (flash.0 - time.delta_secs().min(MAX_DT)).max(0.0);
    }
}

fn token(cell: Option<Cell>) -> String {
    cell.map_or("none".into(), |c| format!("({},{})", c.x, c.y))
}

fn dump(w: &World) -> String {
    let hover = w.get_resource::<Hover>().and_then(|h| h.0);
    let target = w.get_resource::<Target>().and_then(|t| t.0);
    format!("hover={} target={}", token(hover), token(target))
}

/// Hover and target outlines for `G`'s grid. Dump `outline`: `hover=(x,y)|none target=(x,y)|none`.
pub struct OutlinePlugin<G>(PhantomData<fn() -> G>);

impl<G> Default for OutlinePlugin<G> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<G: Game> Plugin for OutlinePlugin<G> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hover>()
            .init_resource::<Target>()
            .init_resource::<Flash>()
            .init_resource::<Time>()
            .init_resource::<Cursor>()
            .init_resource::<Bindings>()
            .init_resource::<UiTheme>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (hover::<G>.run_if(resource_exists::<Sim<G>>), fade, place).chain(),
            )
            .inspect("outline", dump);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pace::tests::Walls;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(OutlinePlugin::<Walls>::default());
        app.update();
        app
    }

    fn outline(app: &mut App, which: Outline) -> (Vec3, Visibility, Vec<(Vec3, Vec2, Color)>) {
        let w = app.world_mut();
        let mut q = w.query::<(&Outline, &Transform, &Visibility, &Children)>();
        let (_, at, shown, children) = q.iter(w).find(|(o, ..)| **o == which).unwrap();
        let children: Vec<Entity> = children.iter().collect();
        let (at, shown) = (at.translation, *shown);
        let mut sides: Vec<_> = children
            .iter()
            .map(|c| {
                let e = w.entity(*c);
                let (sprite, side) = (e.get::<Sprite>().unwrap(), e.get::<Transform>().unwrap());
                (side.translation, sprite.custom_size.unwrap(), sprite.color)
            })
            .collect();
        sides.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
        (at, shown, sides)
    }

    fn line(app: &App) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == "outline").unwrap().1
    }

    #[test]
    fn a_line_is_one_screen_pixel_whatever_the_zoom() {
        let at = |x| Some(Vec2::new(x, 0.0));
        assert_eq!(world_per_pixel(at(10.0), at(13.0)), 1.0 / 3.0, "zoom 3");
        assert_eq!(
            world_per_pixel(at(13.0), at(10.0)),
            1.0 / 3.0,
            "either way round"
        );
        assert_eq!(world_per_pixel(None, at(1.0)), 1.0, "no camera");
        assert_eq!(
            world_per_pixel(at(5.0), at(5.0)),
            1.0,
            "a degenerate camera"
        );
    }

    #[test]
    fn a_point_is_hovered_only_on_the_grid() {
        let at = |x, y| on_grid(Vec2::new(x, y), 16.0, 8, 8);
        assert_eq!(at(0.0, 0.0), Some(Cell::new(0, 0)));
        assert_eq!(at(127.9, 127.9), Some(Cell::new(7, 7)));
        for (x, y) in [(128.0, 5.0), (5.0, 128.0), (-0.1, 5.0), (5.0, -0.1)] {
            assert_eq!(at(x, y), None, "({x},{y}) is past an edge");
        }
    }

    /// Four sides hug the inside of a 16 px cell: none crosses into a neighbour or
    /// overlaps another at a corner.
    #[test]
    fn each_outline_sits_inside_its_cell_in_its_theme_colour() {
        let mut app = app();
        app.insert_resource(Hover(Some(Cell::new(2, 3))));
        app.insert_resource(Target(Some(Cell::new(-1, 0))));
        app.update();
        let theme = UiTheme::default();
        let (at, shown, sides) = outline(&mut app, Outline::Hover);
        assert_eq!(
            (at, shown),
            (Vec3::new(40.0, 56.0, Z), Visibility::Inherited)
        );
        let (h, w) = (Vec2::new(16.0, 1.0), Vec2::new(1.0, 14.0));
        let want = [
            (Vec3::new(-7.5, 0.0, 0.0), w, theme.hover),
            (Vec3::new(0.0, -7.5, 0.0), h, theme.hover),
            (Vec3::new(0.0, 7.5, 0.0), h, theme.hover),
            (Vec3::new(7.5, 0.0, 0.0), w, theme.hover),
        ];
        assert_eq!(sides, want);
        let (at, _, sides) = outline(&mut app, Outline::Target);
        assert_eq!(at, Vec3::new(-8.0, 8.0, Z));
        let side = |x| (Vec3::new(x, 0.0, 0.0), Vec2::new(1.0, 14.0), theme.target);
        assert_eq!((sides[0], sides[3]), (side(-7.5), side(7.5)));
        assert_eq!(line(&app), "hover=(2,3) target=(-1,0)");
    }

    /// A refused flash turns the target red, then fades back after its beat.
    #[test]
    fn a_flash_reddens_the_target_for_its_beat() {
        let mut app = app();
        app.insert_resource(Target(Some(Cell::new(1, 1))));
        app.insert_resource(Flash(0.25));
        let tick = |app: &mut App, secs: f32| {
            let mut time = Time::<()>::default();
            time.advance_by(std::time::Duration::from_secs_f32(secs));
            app.insert_resource(time);
            app.update();
        };
        let theme = UiTheme::default();
        tick(&mut app, 0.1);
        tick(&mut app, 0.1);
        assert_eq!(outline(&mut app, Outline::Target).2[0].2, theme.refused);
        assert_eq!(outline(&mut app, Outline::Hover).2[0].2, theme.hover);
        tick(&mut app, 0.06);
        assert_eq!(app.world().resource::<Flash>().0, 0.0);
        assert_eq!(outline(&mut app, Outline::Target).2[0].2, theme.target);
    }

    #[test]
    fn no_cell_hides_the_outline() {
        let mut app = app();
        assert_eq!(outline(&mut app, Outline::Hover).1, Visibility::Hidden);
        assert_eq!(outline(&mut app, Outline::Target).1, Visibility::Hidden);
        assert_eq!(line(&app), "hover=none target=none");
        app.insert_resource(Target(Some(Cell::new(1, 1))));
        app.update();
        app.insert_resource(Target(None));
        app.update();
        assert_eq!(
            outline(&mut app, Outline::Target).1,
            Visibility::Hidden,
            "closes"
        );
    }

    /// With a world but no camera, nothing is hovered, and nothing panics.
    #[test]
    fn hover_needs_a_camera() {
        let mut app = crate::pace::tests::app();
        app.add_plugins(OutlinePlugin::<Walls>::default());
        app.insert_resource(Hover(Some(Cell::new(1, 1))));
        app.insert_resource(Cursor(Some(Vec2::new(5.0, 5.0))));
        app.update();
        assert_eq!(app.world().resource::<Hover>().0, None);
    }

    /// A hitch (one long frame) right after a refusal still shows the flash.
    #[test]
    fn a_hitch_does_not_swallow_the_flash() {
        let mut app = app();
        app.insert_resource(Target(Some(Cell::new(1, 1))));
        app.insert_resource(Flash(FLASH));
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(5));
        app.insert_resource(time);
        app.update();
        assert!((app.world().resource::<Flash>().0 - (FLASH - MAX_DT)).abs() < 1e-6);
        let theme = UiTheme::default();
        assert_eq!(outline(&mut app, Outline::Target).2[0].2, theme.refused);
    }
}
