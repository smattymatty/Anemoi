//! The darkness: `aeolus::light` from one eye cell, worked out each frame once
//! the Turn has played, drawn as one dark sprite per grid cell.

use std::marker::PhantomData;

use aeolus::{Cell, Light, light};
use bevy::prelude::*;

use crate::inspect::InspectApp;
use crate::intent::Bindings;
use crate::outline::OutlineSystems;
use crate::pace::{Game, Sim, TurnSet};
use crate::ui::UiTheme;

/// The darkness's z: above terrain and Units, below the outlines (50). A game
/// draws its Player above this to keep it out of the dark.
pub const DARK_Z: f32 = 40.0;

/// Where light comes from, when the game says (say, where the Player is drawn
/// mid-slide); without it, the player Unit's cell.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eye(pub Cell);

/// This frame's light out to `radius`. `all` is the dev switch: nothing dark.
#[derive(Resource, Clone, Debug)]
pub struct Vision {
    pub radius: u8,
    pub all: bool,
    eye: Option<Cell>,
    light: Option<Light>,
}

impl Vision {
    pub fn new(radius: u8) -> Self {
        Self {
            radius,
            all: false,
            eye: None,
            light: None,
        }
    }

    /// The cell this frame's light came from; `None` with no Eye and no player.
    pub fn eye(&self) -> Option<Cell> {
        self.eye
    }

    /// The light level at `at`: `radius + 1` at the eye, 0 dark or off the grid.
    pub fn level(&self, at: Cell) -> u8 {
        self.light.as_ref().map_or(0, |l| l.level(at))
    }

    /// Whether `at` is seen: lit, or `all` is on.
    pub fn lit(&self, at: Cell) -> bool {
        self.all || self.level(at) > 0
    }

    /// The darkness over `at`, 0 clear to 255 black.
    pub fn alpha(&self, at: Cell) -> u8 {
        if self.all {
            0
        } else {
            alpha(self.radius, self.level(at))
        }
    }
}

/// Whether `at` answers the pointer and the cursor: lit, or the game has no vision.
pub(crate) fn seen(vision: Option<&Vision>, at: Cell) -> bool {
    vision.is_none_or(|v| v.lit(at))
}

/// The darkness at `level`, `(radius + 1 - level) * 255 / (radius + 1)`: the
/// reference's float curve, exact in integers.
pub fn alpha(radius: u8, level: u8) -> u8 {
    let full = u32::from(radius) + 1;
    (full.saturating_sub(u32::from(level)) * 255 / full) as u8
}

/// One cell's darkness.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shade(pub Cell);

/// The grid size the shades were spawned for.
#[derive(Resource, Default)]
struct Spread(Option<(i32, i32)>);

fn see<G: Game>(sim: Res<Sim<G>>, eye: Option<Res<Eye>>, mut vision: ResMut<Vision>) {
    let world = sim.world();
    let at = eye
        .map(|e| e.0)
        .or_else(|| world.unit(sim.player()).map(|u| u.cell));
    let radius = vision.radius;
    vision.eye = at;
    vision.light = at.map(|at| light(world, at, radius));
}

/// Paints every shade from `Vision`; respawns them all when the grid's size changes.
fn shade<G: Game>(
    (sim, vision, theme, bindings): (Res<Sim<G>>, Res<Vision>, Res<UiTheme>, Res<Bindings>),
    mut spread: ResMut<Spread>,
    mut shades: Query<(Entity, &Shade, &mut Sprite, &mut Transform, &mut Visibility)>,
    mut commands: Commands,
) {
    let px = bindings.cell_px;
    let paint = |at: Cell| {
        let a = vision.alpha(at);
        let sprite = Sprite {
            color: theme.dark.with_alpha(f32::from(a) / 255.0),
            custom_size: Some(Vec2::splat(px)),
            ..default()
        };
        let centre = (Vec2::new(at.x as f32, at.y as f32) + 0.5) * px;
        let shown = if a == 0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        (
            sprite,
            Transform::from_translation(centre.extend(DARK_Z)),
            shown,
        )
    };
    let grid = sim.world().grid();
    let size = (grid.width(), grid.height());
    if spread.0 == Some(size) {
        for (_, cell, mut sprite, mut at, mut shown) in &mut shades {
            (*sprite, *at, *shown) = paint(cell.0);
        }
        return;
    }
    shades.iter().for_each(|s| commands.entity(s.0).despawn());
    for y in 0..size.1 {
        for x in 0..size.0 {
            commands.spawn((Shade(Cell::new(x, y)), paint(Cell::new(x, y))));
        }
    }
    spread.0 = Some(size);
}

fn dump(w: &World) -> String {
    let Some(v) = w.get_resource::<Vision>() else {
        return "none".into();
    };
    let eye = v
        .eye
        .map_or("none".into(), |c| format!("({},{})", c.x, c.y));
    let lit = v.light.as_ref().map_or(0, Light::lit);
    format!("eye={eye} radius={} lit={lit} all={}", v.radius, v.all)
}

/// Light and darkness for `G`'s player, out to `radius` (a game value). Without
/// this plugin every cell is seen. Dump `vision`: `eye=(x,y)|none radius=N lit=K all=bool`.
pub struct VisionPlugin<G> {
    radius: u8,
    all: bool,
    game: PhantomData<fn() -> G>,
}

impl<G> VisionPlugin<G> {
    pub fn new(radius: u8) -> Self {
        Self {
            radius,
            all: false,
            game: PhantomData,
        }
    }

    /// Starts with the dev switch on or off; the game toggles `Vision::all` after.
    pub fn all(self, all: bool) -> Self {
        Self { all, ..self }
    }
}

impl<G: Game> Plugin for VisionPlugin<G> {
    fn build(&self, app: &mut App) {
        let vision = Vision {
            all: self.all,
            ..Vision::new(self.radius)
        };
        app.init_resource::<Bindings>()
            .init_resource::<UiTheme>()
            .init_resource::<Spread>()
            .insert_resource(vision)
            .add_systems(
                Update,
                (see::<G>, shade::<G>)
                    .chain()
                    .after(TurnSet::Play)
                    .before(OutlineSystems)
                    .run_if(resource_exists::<Sim<G>>),
            )
            .inspect("vision", dump);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pace::tests::Walls;
    use crate::pace::{Act, PacePlugin};
    use aeolus::{Dir, Grid, Intent, Unit, UnitId};

    /// A 6x1 corridor, a wall at (3,0), the player at (0,0).
    fn corridor() -> Sim<Walls> {
        let mut grid = Grid::new(6, 1, false);
        grid.set(Cell::new(3, 0), true);
        let mut world = aeolus::World::new(Walls, grid, 1);
        let player = world.spawn(Unit::new("a", 1, Cell::new(0, 0), ()));
        Sim::new(world, player)
    }

    fn app(radius: u8) -> App {
        let mut app = App::new();
        app.add_plugins((
            PacePlugin::<Walls>::default(),
            VisionPlugin::<Walls>::new(radius),
        ))
        .insert_resource(corridor());
        app.update();
        app
    }

    /// Each shade's cell, alpha (0..=255), visibility and z, by cell.
    fn shades(app: &mut App) -> Vec<(Cell, u8, Visibility, f32)> {
        let w = app.world_mut();
        let mut q = w.query::<(&Shade, &Sprite, &Transform, &Visibility)>();
        let mut got: Vec<_> = q
            .iter(w)
            .map(|(s, sprite, at, shown)| {
                let a = (sprite.color.alpha() * 255.0).round() as u8;
                (s.0, a, *shown, at.translation.z)
            })
            .collect();
        got.sort_by_key(|g| (g.0.y, g.0.x));
        got
    }

    fn alphas(app: &mut App) -> Vec<u8> {
        shades(app).into_iter().map(|s| s.1).collect()
    }

    fn line(app: &App) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == "vision").unwrap().1
    }

    #[test]
    fn ten_levels_give_the_reference_alphas_at_radius_8() {
        let got: Vec<u8> = (0..=9).rev().map(|level| alpha(8, level)).collect();
        assert_eq!(got, [0, 28, 56, 85, 113, 141, 170, 198, 226, 255]);
    }

    #[test]
    fn alpha_holds_at_the_ends_of_a_u8() {
        assert_eq!(
            (alpha(0, 1), alpha(0, 0)),
            (0, 255),
            "radius 0: the eye only"
        );
        assert_eq!(alpha(255, 255), 0, "light's eye at radius 255 (capped 254)");
        assert_eq!((alpha(255, 1), alpha(255, 0)), (254, 255));
        assert_eq!(alpha(4, 9), 0, "a level past the eye's is clear");
    }

    /// Falls off from the eye, the wall takes its neighbour's level, the cell
    /// behind it is in shadow and the last is past the edge.
    #[test]
    fn darkness_falls_off_from_the_player_and_shadows_the_wall() {
        let mut app = app(4);
        assert_eq!(alphas(&mut app), [0, 51, 102, 102, 255, 255]);
        let shown: Vec<_> = shades(&mut app).into_iter().map(|s| s.2).collect();
        let (hid, on) = (Visibility::Hidden, Visibility::Inherited);
        assert_eq!(
            shown,
            [hid, on, on, on, on, on],
            "a clear cell draws nothing"
        );
        assert_eq!(line(&app), "eye=(0,0) radius=4 lit=4 all=false");
        let v = app.world().resource::<Vision>();
        assert_eq!(
            (v.level(Cell::new(3, 0)), v.lit(Cell::new(4, 0))),
            (3, false)
        );
        assert_eq!(v.level(Cell::new(-1, 0)), 0, "off the grid");
    }

    #[test]
    fn an_eye_resource_moves_the_light() {
        let mut app = app(4);
        app.insert_resource(Eye(Cell::new(5, 0)));
        app.update();
        assert_eq!(alphas(&mut app), [255, 255, 255, 51, 51, 0]);
        assert_eq!(line(&app), "eye=(5,0) radius=4 lit=3 all=false");
    }

    #[test]
    fn the_dev_switch_lights_everything() {
        let mut app = app(4);
        app.world_mut().resource_mut::<Vision>().all = true;
        app.update();
        assert_eq!(alphas(&mut app), [0; 6]);
        assert!(shades(&mut app).iter().all(|s| s.2 == Visibility::Hidden));
        let v = app.world().resource::<Vision>();
        assert!((0..6).all(|x| v.lit(Cell::new(x, 0))));
        assert_eq!(line(&app), "eye=(0,0) radius=4 lit=4 all=true");
        let mut on = App::new();
        on.add_plugins(VisionPlugin::<Walls>::new(3).all(true));
        let v = on.world().resource::<Vision>();
        assert!(v.all && v.radius == 3, "a game may start with it on");
    }

    /// The light follows a Step in the frame it lands, so nothing reads a stale map.
    #[test]
    fn light_follows_the_step_in_the_same_frame() {
        let mut app = app(4);
        let intent = Intent::Step(Dir::East);
        app.world_mut().write_message(Act::<()> {
            by: UnitId(0),
            intent,
        });
        app.update();
        assert_eq!(line(&app), "eye=(1,0) radius=4 lit=4 all=false");
        assert_eq!(alphas(&mut app), [51, 0, 51, 51, 255, 255]);
    }

    #[test]
    fn shades_sit_below_the_outlines_in_the_dark_role() {
        const { assert!(DARK_Z < crate::outline::Z) };
        let mut app = app(4);
        let dark = Color::srgb_u8(10, 20, 30);
        app.insert_resource(UiTheme { dark, ..default() });
        app.update();
        assert!(shades(&mut app).iter().all(|s| s.3 == DARK_Z));
        let w = app.world_mut();
        let mut q = w.query::<(&Shade, &Sprite, &Transform)>();
        let (_, sprite, at) = q.iter(w).find(|(s, ..)| s.0 == Cell::new(2, 0)).unwrap();
        assert_eq!(sprite.color, dark.with_alpha(102.0 / 255.0));
        assert_eq!(sprite.custom_size, Some(Vec2::splat(16.0)));
        assert_eq!(at.translation.truncate(), Vec2::new(40.0, 8.0));
    }

    #[test]
    fn a_new_grid_size_respawns_one_shade_per_cell() {
        let mut app = app(4);
        let ids = |app: &mut App| {
            let w = app.world_mut();
            let mut q = w.query_filtered::<Entity, With<Shade>>();
            q.iter(w).collect::<Vec<_>>()
        };
        let first = ids(&mut app);
        app.update();
        assert_eq!(ids(&mut app), first, "the same grid keeps its shades");
        let mut world = aeolus::World::new(Walls, Grid::new(2, 3, false), 1);
        let player = world.spawn(Unit::new("a", 1, Cell::new(1, 2), ()));
        app.insert_resource(Sim::new(world, player));
        app.update();
        let cells: Vec<_> = shades(&mut app).into_iter().map(|s| s.0).collect();
        let want: Vec<_> = (0..3)
            .flat_map(|y| (0..2).map(move |x| Cell::new(x, y)))
            .collect();
        assert_eq!(cells, want);
    }

    /// No Eye and no player Unit: no light at all, never last frame's map.
    #[test]
    fn without_an_eye_or_a_player_unit_everything_is_dark() {
        let mut app = app(4);
        assert_eq!(line(&app), "eye=(0,0) radius=4 lit=4 all=false");
        let mut world = aeolus::World::new(Walls, Grid::new(6, 1, false), 1);
        world.spawn(Unit::new("a", 1, Cell::new(0, 0), ()));
        app.insert_resource(Sim::new(world, UnitId(9)));
        app.update();
        assert_eq!(line(&app), "eye=none radius=4 lit=0 all=false");
        assert_eq!(alphas(&mut app), [255; 6]);
        let v = app.world().resource::<Vision>();
        assert_eq!((v.eye(), v.lit(Cell::new(0, 0))), (None, false));
    }
}
