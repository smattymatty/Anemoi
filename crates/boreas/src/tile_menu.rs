//! The Tile Menu: one cell's Offers in a small menu beside it, opened by a click
//! or from the tile cursor. A row runs as an `Act`, a `TravelTo` or a `Looked`;
//! a refused row runs nothing but `Refused`, which the outline flashes and a toast shows.
//! The game names rows and reasons through [`Labels`].

use std::marker::PhantomData;

use aeolus::{Cell, Intent, Kind, UnitId, next_step};
use bevy::ecs::relationship::RelatedSpawner;
use bevy::ecs::spawn::SpawnWith;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::cursor::Cursor;
use crate::inspect::InspectApp;
use crate::intent::{Bindings, ClickMode, IntentPlugin, TravelTo};
use crate::outline::{self, OutlinePlugin, Target, pointed};
use crate::owner::{InputOwner, Owner, TakesInput};
use crate::pace::{Act, Game, Sim, TurnSet};
use crate::toast;
use crate::ui::{self, Focusable, MenuSelection, UiActivated, UiPlugin, UiTheme};

/// What a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row<I> {
    Intent(Intent<I>),
    Travel(Cell),
    Look,
}

/// Why an Offer cannot be taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal<C> {
    /// A shut gate: the closest Route's missing Conditions.
    Gate(Vec<C>),
    NoRoute,
}

/// One row and, when it cannot be taken, why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offered<I, C> {
    pub row: Row<I>,
    pub refusal: Option<Refusal<C>>,
}

/// The game's words for rows and reasons; Boreas has none.
pub trait Labels: Game {
    fn label(world: &aeolus::World<Self>, cell: Cell, row: &Row<Self::Intent>) -> String;
    fn reason(world: &aeolus::World<Self>, refusal: &Refusal<Self::Condition>) -> String;
}

type Rows<G> = Vec<Offered<<G as aeolus::Rules>::Intent, <G as aeolus::Rules>::Condition>>;

fn take<I, C>(row: Row<I>) -> Offered<I, C> {
    Offered { row, refusal: None }
}

/// The rows for `cell`, Look last. Near the Unit: `aeolus::offers`. Distant: Travel
/// when routed, refused by a shut gate (its reason wins) or for no route; a
/// distant wall or solid Unit offers Look only. Off the grid: nothing.
pub fn offered<G: Game>(world: &aeolus::World<G>, unit: UnitId, cell: Cell) -> Rows<G>
where
    G::Condition: Clone,
{
    let (Some(walker), Some(terrain)) = (world.unit(unit), world.grid().get(cell)) else {
        return Vec::new();
    };
    let near = walker.cell == cell || walker.cell.toward(cell).is_some();
    let mut rows: Rows<G> = if near {
        let offers = aeolus::offers(world, unit, cell).into_iter();
        offers.map(|i| take(Row::Intent(i))).collect()
    } else {
        let rules = world.rules();
        let holds = |c: &G::Condition| rules.holds(walker, c);
        let refusal = match rules.kind(terrain) {
            Kind::Gate(gate) if !gate.opens(holds) => {
                let missing = gate.missing(holds).unwrap_or_default();
                Some(Some(Refusal::Gate(missing.into_iter().cloned().collect())))
            }
            _ if !rules.passes(walker, terrain) || world.solid_at(cell).is_some() => None,
            _ => Some(next_step(world, unit, cell).map_or(Some(Refusal::NoRoute), |_| None)),
        };
        let travel = refusal.map(|refusal| Offered {
            row: Row::Travel(cell),
            refusal,
        });
        travel.into_iter().collect()
    };
    rows.push(take(Row::Look));
    rows
}

/// The open menu, on its panel; the panel's rows are its `Focusable` children.
#[derive(Component)]
pub struct TileMenu<I: Send + Sync + 'static, C: Send + Sync + 'static> {
    pub at: Cell,
    pub rows: Vec<Offered<I, C>>,
    pub labels: Vec<String>,
    /// Each refused row's reason, worked out once at open.
    pub reasons: Vec<Option<String>>,
}

/// The keyboard tile cursor: takes the keys while it exists, never steps.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileCursor(pub Cell);

/// Opens the Tile Menu on a cell, closing any other. A click sends it; so may a game.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenTileMenu(pub Cell);

/// Turns the tile cursor on at the player's cell, or off. The game binds the key.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ToggleTileCursor;

/// A Look row ran. The game says what is there; no Intent, no Turn.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Looked {
    pub by: UnitId,
    pub cell: Cell,
}

/// A refused row was taken; nothing ran. Toast and outline answer it; so may the
/// game (a shake, a sound). `reason` is the game's text for `refusal`.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct Refused<C: Send + Sync + 'static> {
    pub by: UnitId,
    pub cell: Cell,
    pub refusal: Refusal<C>,
    pub reason: String,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
enum Choose {
    Run(usize),
    Close,
}

#[derive(Resource, Default)]
struct Looks(u32);

#[derive(Resource, Default)]
struct Refusals(u32);

/// What a click does: §4.4's contract, given the open menu's cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    Open(Cell),
    First,
    Close,
    Nothing,
}

/// Any button opens on a grid cell; the travel button on the open cell runs row
/// 0; a click anywhere else closes it and is used up.
fn on_click(open: Option<Cell>, cell: Option<Cell>, primary: bool, free: bool) -> Click {
    match (open, cell) {
        (Some(at), Some(c)) if at == c && primary => Click::First,
        (Some(at), Some(c)) if at == c => Click::Nothing,
        (Some(_), _) => Click::Close,
        (None, Some(c)) if free => Click::Open(c),
        (None, _) => Click::Nothing,
    }
}

/// Toggles the tile cursor, moves it a cell per press, opens its cell's menu.
fn steer<G: Game>(
    mut toggles: MessageReader<ToggleTileCursor>,
    (keys, bindings, sim): (Res<ButtonInput<KeyCode>>, Res<Bindings>, Res<Sim<G>>),
    menus: Query<(), With<TileMenu<G::Intent, G::Condition>>>,
    mut cursors: Query<(Entity, &mut TileCursor)>,
    mut open: MessageWriter<OpenTileMenu>,
    mut commands: Commands,
) {
    let cursor = cursors.single_mut().ok();
    if toggles.read().count() % 2 == 1 {
        match (cursor, sim.world().unit(sim.player())) {
            (Some((e, _)), _) => commands.entity(e).despawn(),
            (None, Some(u)) => {
                commands.spawn((TileCursor(u.cell), TakesInput));
            }
            (None, None) => {}
        }
        return;
    }
    let Some((e, mut cursor)) = cursor.filter(|_| menus.is_empty()) else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        commands.entity(e).despawn();
        return;
    }
    let pressed = bindings.steps.iter().find(|(k, _)| keys.just_pressed(*k));
    let next = pressed.map(|&(_, dir)| cursor.0.step(dir));
    if let Some(next) = next.filter(|&c| sim.world().grid().get(c).is_some()) {
        cursor.0 = next;
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::Space]) {
        open.write(OpenTileMenu(cursor.0));
    }
}

/// Map clicks, while this plugin has them (`ClickMode::TileMenu`). A click on any
/// button or on the menu is the UI's.
fn click<G: Game>(
    (mouse, owner): (
        Res<ButtonInput<MouseButton>>,
        InputOwner<Without<TileCursor>>,
    ),
    (bindings, cursor, sim): (Res<Bindings>, Res<Cursor>, Res<Sim<G>>),
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    menus: Query<&TileMenu<G::Intent, G::Condition>>,
    over: Query<&Interaction, Or<(With<Button>, With<TileMenu<G::Intent, G::Condition>>)>>,
    tile_cursors: Query<Entity, With<TileCursor>>,
    (mut choose, mut open, mut commands): (
        MessageWriter<Choose>,
        MessageWriter<OpenTileMenu>,
        Commands,
    ),
) {
    let Some(&button) = mouse.get_just_pressed().next() else {
        return;
    };
    if over.iter().any(|i| *i != Interaction::None) {
        return;
    }
    let cell = pointed(&cursor, &camera, bindings.cell_px, sim.world().grid());
    // Free: play has the input, or only the tile cursor holds it.
    let free = owner.get() == Owner::Gameplay;
    let at = menus.single().ok().map(|m| m.at);
    match on_click(at, cell, button == bindings.travel, free) {
        Click::Open(c) => {
            tile_cursors
                .iter()
                .for_each(|e| commands.entity(e).despawn());
            open.write(OpenTileMenu(c));
        }
        Click::First => {
            choose.write(Choose::Run(0));
        }
        Click::Close => {
            choose.write(Choose::Close);
        }
        Click::Nothing => {}
    }
}

/// Escape, and rows activated by Enter/Space or a pointer press (`ui.rs`).
fn pick<G: Game>(
    keys: Res<ButtonInput<KeyCode>>,
    mut activated: MessageReader<UiActivated>,
    menus: Query<&Children, With<TileMenu<G::Intent, G::Condition>>>,
    rows: Query<&Focusable>,
    mut choose: MessageWriter<Choose>,
) {
    let Ok(children) = menus.single() else {
        activated.clear();
        return;
    };
    for a in activated.read().filter(|a| children.contains(&a.entity)) {
        if let Ok(row) = rows.get(a.entity) {
            choose.write(Choose::Run(row.0));
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        choose.write(Choose::Close);
    }
}

/// What a choice leaves: the menu open, closed, or closed with the tile cursor ended.
enum After {
    Stay,
    Close,
    End,
}

/// Where a row's run goes.
#[derive(SystemParam)]
struct Outbox<'w, G: Game> {
    acts: MessageWriter<'w, Act<<G as aeolus::Rules>::Intent>>,
    travel: MessageWriter<'w, TravelTo>,
    looked: MessageWriter<'w, Looked>,
    refused: MessageWriter<'w, Refused<<G as aeolus::Rules>::Condition>>,
    looks: ResMut<'w, Looks>,
}

impl<G: Game> Outbox<'_, G>
where
    G::Condition: Clone,
{
    /// Sends row `i`'s Act, TravelTo, Looked or Refused. A refused row keeps the
    /// menu; Look keeps the tile cursor; any other row ends it.
    fn run(&mut self, by: UnitId, menu: &mut TileMenu<G::Intent, G::Condition>, i: usize) -> After {
        let (cell, Some(offer)) = (menu.at, menu.rows.get(i)) else {
            return After::Stay;
        };
        if let Some(refusal) = offer.refusal.clone() {
            let reason = menu.reasons[i].clone().unwrap_or_default();
            self.refused.write(Refused {
                by,
                cell,
                refusal,
                reason,
            });
            return After::Stay;
        }
        // The menu is despawned after this: move the row out of it.
        match menu.rows.swap_remove(i).row {
            Row::Intent(intent) => {
                self.acts.write(Act { by, intent });
            }
            Row::Travel(to) => {
                self.travel.write(TravelTo(to));
            }
            Row::Look => {
                self.looked.write(Looked { by, cell });
                self.looks.0 += 1;
                return After::Close;
            }
        }
        After::End
    }
}

/// Runs or closes the open menu on the newest choice, then opens the newest request.
fn apply<G: Labels>(
    mut commands: Commands,
    (mut choices, mut opens): (MessageReader<Choose>, MessageReader<OpenTileMenu>),
    (sim, theme): (Res<Sim<G>>, Res<UiTheme>),
    mut menus: Query<(Entity, &mut TileMenu<G::Intent, G::Condition>)>,
    tile_cursors: Query<Entity, With<TileCursor>>,
    mut out: Outbox<G>,
) where
    G::Condition: Clone,
{
    let by = sim.player();
    let mut menu = menus.single_mut().ok();
    let choice = choices.read().last().copied();
    if let (Some(choice), Some((panel, open))) = (choice, menu.as_mut()) {
        let after = match choice {
            Choose::Run(i) => out.run(by, open, i),
            Choose::Close => After::Close,
        };
        if let After::End = after {
            tile_cursors
                .iter()
                .for_each(|e| commands.entity(e).despawn());
        }
        if !matches!(after, After::Stay) {
            commands.entity(*panel).despawn();
            menu = None;
        }
    }
    let Some(&OpenTileMenu(at)) = opens.read().last() else {
        return;
    };
    let world = sim.world();
    let rows = offered(world, by, at);
    if rows.is_empty() {
        return;
    }
    if let Some((panel, _)) = menu {
        commands.entity(panel).despawn();
    }
    let labels: Vec<String> = rows.iter().map(|o| G::label(world, at, &o.row)).collect();
    let reasons: Vec<Option<String>> = rows
        .iter()
        .map(|o| o.refusal.as_ref().map(|r| G::reason(world, r)))
        .collect();
    let panel = panel(&labels, &reasons, &theme);
    commands.spawn(panel).insert(TileMenu {
        at,
        rows,
        labels,
        reasons,
    });
}

/// The menu's panel: one compact row per label, muted with its reason when refused.
fn panel(labels: &[String], reasons: &[Option<String>], theme: &UiTheme) -> impl Bundle {
    let rows: Vec<_> = labels
        .iter()
        .cloned()
        .zip(reasons.iter().cloned())
        .collect();
    let t = *theme;
    (
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            padding: UiRect::all(px(3)),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(theme.panel),
        BorderColor::all(theme.accent),
        Interaction::None,
        GlobalZIndex(10),
        MenuSelection {
            selected: 0,
            count: rows.len(),
        },
        TakesInput,
        Children::spawn(SpawnWith(move |p: &mut RelatedSpawner<ChildOf>| {
            for (i, (label, reason)) in rows.into_iter().enumerate() {
                match reason {
                    Some(r) => p.spawn(ui::refused_row(label, r, i, &t)),
                    None => p.spawn(ui::row(label, i, &t)),
                };
            }
        })),
    )
}

/// Counts refusals for the `refusal` dump.
fn tally<C: Send + Sync + 'static>(
    mut refused: MessageReader<Refused<C>>,
    mut n: ResMut<Refusals>,
) {
    n.0 += refused.read().count() as u32;
}

/// The outline target: the open menu's cell, else the tile cursor's.
fn aim<G: Game>(
    menus: Query<&TileMenu<G::Intent, G::Condition>>,
    cursors: Query<&TileCursor>,
    mut target: ResMut<Target>,
) {
    let menu = menus.single().ok().map(|m| m.at);
    let cell = menu.or_else(|| cursors.single().ok().map(|c| c.0));
    target.set_if_neq(Target(cell));
}

const GAP: f32 = 4.0;

/// The panel's top-left: right of the cell, else left of it, clamped inside the view.
fn beside(cell: Rect, size: Vec2, view: Vec2) -> Vec2 {
    let x = if cell.max.x + GAP + size.x <= view.x {
        cell.max.x + GAP
    } else {
        cell.min.x - GAP - size.x
    };
    let room = (view - size).max(Vec2::ZERO);
    Vec2::new(x, cell.min.y).clamp(Vec2::ZERO, room)
}

fn place<G: Game>(
    bindings: Res<Bindings>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut panels: Query<(&TileMenu<G::Intent, G::Condition>, &ComputedNode, &mut Node)>,
) {
    let Ok((cam, eye)) = camera.single() else {
        return;
    };
    let Some(view) = cam.logical_viewport_size() else {
        return;
    };
    for (menu, computed, mut node) in &mut panels {
        let corner = |dx: i32, dy: i32| {
            let c = Vec2::new((menu.at.x + dx) as f32, (menu.at.y + dy) as f32);
            cam.world_to_viewport(eye, (c * bindings.cell_px).extend(0.0))
        };
        let (Ok(top_left), Ok(bottom_right)) = (corner(0, 1), corner(1, 0)) else {
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        let at = beside(Rect::from_corners(top_left, bottom_right), size, view);
        if (node.left, node.top) != (px(at.x), px(at.y)) {
            (node.left, node.top) = (px(at.x), px(at.y));
        }
    }
}

fn token(cell: Option<Cell>) -> String {
    cell.map_or("none".into(), |c| format!("({},{})", c.x, c.y))
}

fn dump_refusal(w: &World) -> String {
    let refusals = w.get_resource::<Refusals>().map_or(0, |r| r.0);
    let text = toast::shown(w).map_or("none".into(), |t| format!("\"{t}\""));
    format!("toast={text} refusals={refusals}")
}

fn dump<G: Game>(w: &World) -> String {
    let looks = w.get_resource::<Looks>().map_or(0, |l| l.0);
    let cursor = w
        .try_query::<&TileCursor>()
        .and_then(|mut q| q.iter(w).next().map(|c| c.0));
    let cursor = token(cursor);
    let menu = w
        .try_query::<(&TileMenu<G::Intent, G::Condition>, &MenuSelection)>()
        .and_then(|mut q| q.iter(w).next());
    let Some((menu, focus)) = menu else {
        return format!(
            "open=false at=none rows=none focus=none refused=0 cursor={cursor} looks={looks}"
        );
    };
    let rows: Vec<String> = menu.labels.iter().map(|l| l.replace(' ', "_")).collect();
    let refused = menu.rows.iter().filter(|o| o.refusal.is_some()).count();
    format!(
        "open=true at={} rows={} focus={} refused={refused} cursor={cursor} looks={looks}",
        token(Some(menu.at)),
        rows.join(","),
        focus.selected
    )
}

/// The Tile Menu and the tile cursor for `G`'s player. Sets `clicks` as the
/// `ClickMode` (default `TileMenu`, map clicks open the menu); the game may change it.
/// Dump `tile_menu`: `open= at= rows=A,B focus=N refused=N cursor= looks=N`;
/// dump `refusal`: `toast="<text>"|none refusals=N`.
pub struct TileMenuPlugin<G> {
    pub clicks: ClickMode,
    game: PhantomData<fn() -> G>,
}

impl<G> TileMenuPlugin<G> {
    pub fn new(clicks: ClickMode) -> Self {
        Self {
            clicks,
            game: PhantomData,
        }
    }
}

impl<G> Default for TileMenuPlugin<G> {
    fn default() -> Self {
        Self::new(ClickMode::TileMenu)
    }
}

impl<G: Labels> Plugin for TileMenuPlugin<G>
where
    G::Condition: Clone,
{
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<IntentPlugin<G>>() {
            app.add_plugins(IntentPlugin::<G>::default());
        }
        if !app.is_plugin_added::<OutlinePlugin<G>>() {
            app.add_plugins(OutlinePlugin::<G>::default());
        }
        if !app.is_plugin_added::<UiPlugin>() {
            app.add_plugins(UiPlugin);
        }
        app.init_resource::<Looks>()
            .init_resource::<Refusals>()
            .insert_resource(self.clicks)
            .add_message::<OpenTileMenu>()
            .add_message::<ToggleTileCursor>()
            .add_message::<Looked>()
            .add_message::<Refused<G::Condition>>()
            .add_message::<Choose>()
            .add_systems(
                Update,
                (
                    steer::<G>,
                    click::<G>.run_if(resource_equals(ClickMode::TileMenu)),
                    pick::<G>,
                    apply::<G>,
                    (
                        toast::on_refused::<G::Condition>,
                        outline::flash_on_refused::<G::Condition>,
                        tally::<G::Condition>,
                    ),
                    aim::<G>,
                    place::<G>,
                )
                    .chain()
                    .run_if(resource_exists::<Sim<G>>)
                    .after(ui::navigate_and_activate)
                    .before(TurnSet::Input),
            )
            .add_systems(Update, toast::rise)
            .inspect("tile_menu", dump::<G>)
            .inspect("refusal", dump_refusal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::Travel;
    use crate::outline::{FLASH, Flash};
    use crate::pace::tests::Walls;
    use crate::toast::Toast;
    use aeolus::{Cost, Dir, Gate, Grid, Rules, Unit};

    /// Walls, gates of numbered keys, and Wait on the Unit's own cell.
    struct Locks;

    #[derive(Clone)]
    enum T {
        Open,
        Wall,
        Lock(Gate<u8>),
    }

    impl Rules for Locks {
        type Ext = Vec<u8>;
        type Terrain = T;
        type Condition = u8;
        type Intent = ();
        type Event = ();

        fn kind<'a>(&'a self, t: &'a T) -> Kind<'a, u8> {
            match t {
                T::Open => Kind::Passable,
                T::Wall => Kind::Blocking,
                T::Lock(gate) => Kind::Gate(gate),
            }
        }
        fn holds(&self, unit: &Unit<Vec<u8>>, key: &u8) -> bool {
            unit.ext.contains(key)
        }
        fn opened(&self, _: &T) -> T {
            T::Open
        }
        fn offers(world: &aeolus::World<Self>, by: UnitId, cell: Cell) -> Vec<()> {
            let own = world.unit(by).is_some_and(|u| u.cell == cell);
            if own { vec![()] } else { Vec::new() }
        }
        fn act(_: &mut aeolus::World<Self>, _: UnitId, _: ()) -> (Vec<()>, Cost) {
            (Vec::new(), Cost::Turn)
        }
    }

    /// A 5x1 corridor `o . . . .` with `cells` set; the Unit at (0,0).
    fn corridor(cells: &[(i32, T)], keys: &[u8]) -> (aeolus::World<Locks>, UnitId) {
        let mut grid = Grid::new(5, 1, T::Open);
        for (x, t) in cells {
            grid.set(Cell::new(*x, 0), t.clone());
        }
        let mut world = aeolus::World::new(Locks, grid, 1);
        let unit = world.spawn(Unit::new("u", 1, Cell::new(0, 0), keys.to_vec()));
        (world, unit)
    }

    fn rows(cells: &[(i32, T)], keys: &[u8], x: i32) -> Vec<Offered<(), u8>> {
        let (world, unit) = corridor(cells, keys);
        offered(&world, unit, Cell::new(x, 0))
    }

    fn ok(row: Row<()>) -> Offered<(), u8> {
        take(row)
    }

    fn no(refusal: Refusal<u8>) -> Offered<(), u8> {
        Offered {
            row: Row::Travel(Cell::new(4, 0)),
            refusal: Some(refusal),
        }
    }

    #[test]
    fn near_cells_offer_aeolus_rows_then_look() {
        let step = ok(Row::Intent(Intent::Step(Dir::East)));
        assert_eq!(rows(&[], &[], 1), [step.clone(), ok(Row::Look)]);
        assert_eq!(
            rows(&[(1, T::Wall)], &[], 1),
            [step, ok(Row::Look)],
            "a wall too"
        );
        let wait = ok(Row::Intent(Intent::Game(())));
        assert_eq!(rows(&[], &[], 0), [wait, ok(Row::Look)], "own cell");
    }

    #[test]
    fn a_distant_open_cell_offers_travel_or_refuses_no_route() {
        let travel = ok(Row::Travel(Cell::new(4, 0)));
        assert_eq!(rows(&[], &[], 4), [travel, ok(Row::Look)]);
        let walled = rows(&[(2, T::Wall)], &[], 4);
        assert_eq!(walled, [no(Refusal::NoRoute), ok(Row::Look)]);
    }

    /// §4.9: the gate's reason wins, even walled off; an opening gate is Travel.
    #[test]
    fn a_distant_shut_gate_refuses_with_its_missing_conditions() {
        let gate = || T::Lock(Gate::new(vec![vec![5, 6], vec![7]]));
        let shut = rows(&[(4, gate())], &[6], 4);
        assert_eq!(shut, [no(Refusal::Gate(vec![5])), ok(Row::Look)]);
        let walled = rows(&[(2, T::Wall), (4, gate())], &[], 4);
        assert_eq!(walled, [no(Refusal::Gate(vec![7])), ok(Row::Look)]);
        let opens = rows(&[(4, gate())], &[7], 4);
        assert_eq!(opens, [ok(Row::Travel(Cell::new(4, 0))), ok(Row::Look)]);
        let never = rows(&[(4, T::Lock(Gate::new(vec![])))], &[], 4);
        assert_eq!(never, [no(Refusal::Gate(vec![])), ok(Row::Look)]);
    }

    #[test]
    fn a_distant_wall_or_unit_offers_look_only_and_off_grid_nothing() {
        assert_eq!(rows(&[(3, T::Wall)], &[], 3), [ok(Row::Look)]);
        let (mut world, unit) = corridor(&[], &[]);
        world.spawn(Unit::new("v", 1, Cell::new(3, 0), Vec::new()));
        assert_eq!(offered(&world, unit, Cell::new(3, 0)), [ok(Row::Look)]);
        assert_eq!(offered(&world, unit, Cell::new(5, 0)), []);
        assert_eq!(offered(&world, UnitId(7), Cell::new(1, 0)), []);
    }

    #[test]
    fn the_click_contract() {
        let (a, b) = (Some(Cell::new(1, 1)), Some(Cell::new(2, 1)));
        assert_eq!(on_click(None, a, false, true), Click::Open(Cell::new(1, 1)));
        assert_eq!(on_click(None, a, true, false), Click::Nothing, "not play's");
        assert_eq!(on_click(None, None, true, true), Click::Nothing, "off grid");
        assert_eq!(on_click(a, a, true, true), Click::First);
        assert_eq!(on_click(a, a, false, true), Click::Nothing, "right on it");
        assert_eq!(on_click(a, b, true, true), Click::Close);
        assert_eq!(on_click(a, None, false, true), Click::Close, "off grid");
    }

    #[test]
    fn the_panel_sits_beside_the_cell_inside_the_view() {
        let view = Vec2::new(100.0, 50.0);
        let cell = |x, y| Rect::new(x, y, x + 10.0, y + 10.0);
        let size = Vec2::new(30.0, 20.0);
        assert_eq!(beside(cell(10.0, 5.0), size, view), Vec2::new(24.0, 5.0));
        assert_eq!(
            beside(cell(80.0, 5.0), size, view),
            Vec2::new(46.0, 5.0),
            "flips left"
        );
        assert_eq!(
            beside(cell(10.0, 45.0), size, view),
            Vec2::new(24.0, 30.0),
            "clamped up"
        );
        assert_eq!(beside(cell(-5.0, -8.0), size, view), Vec2::new(9.0, 0.0));
        let huge = Vec2::new(200.0, 80.0);
        assert_eq!(
            beside(cell(10.0, 5.0), huge, view),
            Vec2::ZERO,
            "never off the top-left"
        );
    }

    impl Labels for Walls {
        fn label(_: &aeolus::World<Self>, _: Cell, row: &Row<()>) -> String {
            match row {
                Row::Intent(Intent::Step(_)) => "Step",
                Row::Intent(Intent::Game(_)) => "Own",
                Row::Travel(_) => "Go there",
                Row::Look => "Look",
            }
            .into()
        }
        fn reason(_: &aeolus::World<Self>, refusal: &Refusal<()>) -> String {
            match refusal {
                Refusal::Gate(_) => "shut".into(),
                Refusal::NoRoute => "no route".into(),
            }
        }
    }

    #[derive(Resource, Default)]
    struct Seen(Vec<Looked>);

    /// Walls' 5x5 world, player at (0,0), with the Tile Menu; Looks recorded.
    fn app() -> App {
        let mut app = crate::pace::tests::app();
        app.add_plugins(TileMenuPlugin::<Walls>::default())
            .init_resource::<Seen>()
            .add_systems(Last, |mut m: MessageReader<Looked>, mut s: ResMut<Seen>| {
                s.0.extend(m.read().copied());
            });
        app.update();
        app
    }

    fn open(app: &mut App, x: i32, y: i32) {
        app.world_mut().write_message(OpenTileMenu(Cell::new(x, y)));
        app.update();
    }

    fn tap(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn line(app: &App, name: &str) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == name).unwrap().1
    }

    fn sim(app: &App) -> (Cell, u64) {
        let sim = app.world().resource::<Sim<Walls>>();
        (
            sim.world().unit(sim.player()).unwrap().cell,
            sim.world().turn(),
        )
    }

    fn rows_of(app: &mut App) -> Vec<Entity> {
        rows_of_in(app.world_mut())
    }

    #[test]
    fn opening_takes_the_keys_and_costs_no_turn() {
        let mut app = app();
        open(&mut app, 1, 0);
        let want = "open=true at=(1,0) rows=Step,Look focus=0 refused=0 cursor=none looks=0";
        assert_eq!(line(&app, "tile_menu"), want);
        assert_eq!(line(&app, "outline"), "hover=none target=(1,0)");
        assert!(line(&app, "intent").starts_with("owner=menu "));
        tap(&mut app, KeyCode::KeyD);
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "D moves nothing");
        assert_eq!(rows_of(&mut app).len(), 2);
    }

    #[test]
    fn enter_runs_row_zero_as_an_act_and_closes() {
        let mut app = app();
        open(&mut app, 1, 0);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(sim(&app), (Cell::new(1, 0), 1));
        assert!(line(&app, "tile_menu").starts_with("open=false at=none rows=none "));
        assert_eq!(line(&app, "outline"), "hover=none target=none");
        assert!(line(&app, "intent").starts_with("owner=gameplay "));
    }

    #[test]
    fn look_sends_a_message_and_no_intent() {
        let mut app = app();
        open(&mut app, 1, 0);
        tap(&mut app, KeyCode::KeyS);
        assert!(line(&app, "tile_menu").contains(" focus=1 "));
        tap(&mut app, KeyCode::Enter);
        assert_eq!(sim(&app), (Cell::new(0, 0), 0));
        let by = UnitId(0);
        let cell = Cell::new(1, 0);
        assert_eq!(app.world().resource::<Seen>().0, [Looked { by, cell }]);
        assert!(line(&app, "tile_menu").starts_with("open=false "));
        assert!(line(&app, "tile_menu").ends_with(" looks=1"));
    }

    #[test]
    fn escape_closes_with_no_turn() {
        let mut app = app();
        open(&mut app, 1, 0);
        tap(&mut app, KeyCode::Escape);
        assert!(line(&app, "tile_menu").starts_with("open=false "));
        assert_eq!(sim(&app), (Cell::new(0, 0), 0));
    }

    /// Headless, the pointer never reaches `Interaction`: insert it, as bevy_ui
    /// would. The same press is a map click off the grid, which must not close it.
    #[test]
    fn a_pointer_press_on_a_row_runs_it() {
        let mut app = app();
        open(&mut app, 0, 1);
        let row = rows_of(&mut app)[0];
        app.world_mut().entity_mut(row).insert(Interaction::Pressed);
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.press(MouseButton::Left);
        app.update();
        assert_eq!(sim(&app), (Cell::new(0, 1), 1));
        assert!(line(&app, "tile_menu").starts_with("open=false "));
    }

    /// A Travel row: the menu closes before input reads the `TravelTo`, so it walks at once.
    #[test]
    fn a_distant_cell_travels_the_same_frame() {
        let mut app = app();
        open(&mut app, 0, 3);
        assert!(line(&app, "tile_menu").contains(" rows=Go_there,Look "));
        tap(&mut app, KeyCode::Enter);
        assert_eq!(
            *app.world().resource::<Travel>(),
            Travel(Some(Cell::new(0, 3)))
        );
        assert_eq!(sim(&app), (Cell::new(0, 1), 1));
    }

    /// (4,4) walled off by (3,4) and (4,3).
    fn walled(app: &mut App) {
        let mut grid = Grid::new(5, 5, false);
        grid.set(Cell::new(3, 4), true);
        grid.set(Cell::new(4, 3), true);
        let mut world = aeolus::World::new(Walls, grid, 1);
        let player = world.spawn(Unit::new("a", 1, Cell::new(0, 0), ()));
        app.insert_resource(Sim::new(world, player));
    }

    #[test]
    fn a_refused_row_runs_nothing_and_stays_open() {
        let mut app = app();
        walled(&mut app);
        open(&mut app, 4, 4);
        let want = "open=true at=(4,4) rows=Go_there,Look focus=0 refused=1 ";
        assert!(
            line(&app, "tile_menu").starts_with(want),
            "{}",
            line(&app, "tile_menu")
        );
        let muted = app.world().resource::<UiTheme>().muted;
        let texts: Vec<Color> = {
            let w = app.world_mut();
            let row = rows_of_in(w)[0];
            let kids: Vec<Entity> = w.entity(row).get::<Children>().unwrap().iter().collect();
            kids.iter()
                .map(|k| w.entity(*k).get::<TextColor>().unwrap().0)
                .collect()
        };
        assert_eq!(texts, [muted, muted], "label and reason, muted");
        tap(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
        assert_eq!(sim(&app), (Cell::new(0, 0), 0));
        assert!(line(&app, "tile_menu").starts_with("open=true "));
    }

    /// §4.10: one toast with the reason over the cell, a red flash, one `Refused`
    /// per take; a second take replaces the toast.
    #[test]
    fn taking_a_refused_row_toasts_flashes_and_tells_the_game() {
        let mut app = app();
        app.init_resource::<Messages<Refused<()>>>();
        walled(&mut app);
        assert_eq!(line(&app, "refusal"), "toast=none refusals=0");
        open(&mut app, 4, 4);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(line(&app, "refusal"), "toast=\"no route\" refusals=1");
        assert_eq!(app.world().resource::<Flash>().0, FLASH);
        let sent: Vec<_> = {
            let msgs = app.world().resource::<Messages<Refused<()>>>();
            msgs.get_cursor().read(msgs).cloned().collect()
        };
        let (by, cell) = (UnitId(0), Cell::new(4, 4));
        let (refusal, reason) = (Refusal::NoRoute, "no route".into());
        assert_eq!(
            sent,
            [Refused {
                by,
                cell,
                refusal,
                reason
            }]
        );
        let toasts = |app: &mut App| {
            let w = app.world_mut();
            let mut q = w.query_filtered::<(&Transform, &TextColor), With<Toast>>();
            q.iter(w)
                .map(|(t, c)| (t.translation.xy(), c.0))
                .collect::<Vec<_>>()
        };
        let red = app.world().resource::<UiTheme>().refused;
        assert_eq!(toasts(&mut app), [(Vec2::new(72.0, 86.0), red)]);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(line(&app, "refusal"), "toast=\"no route\" refusals=2");
        assert_eq!(toasts(&mut app).len(), 1, "one at a time");
        assert_eq!(sim(&app), (Cell::new(0, 0), 0));
    }

    /// A 2D camera with no renderer: viewport (x, y) shows world (x, 80 - y).
    fn camera(app: &mut App) {
        let mut cam = Camera::default();
        cam.computed.clip_from_view = Mat4::orthographic_rh(-40.0, 40.0, -40.0, 40.0, -1.0, 1.0);
        cam.computed.target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: UVec2::splat(80),
            scale_factor: 1.0,
        });
        let eye = GlobalTransform::from_xyz(40.0, 40.0, 0.0);
        app.world_mut().spawn((cam, Camera2d, eye));
    }

    /// A Left click on the centre of cell (x, y) through the camera.
    fn click_cell(app: &mut App, x: i32, y: i32) {
        press_cell(app, x, y, MouseButton::Left);
    }

    /// A click of `button` on the centre of cell (x, y) through the camera.
    fn press_cell(app: &mut App, x: i32, y: i32, button: MouseButton) {
        let at = Vec2::new(x as f32 * 16.0 + 8.0, 80.0 - (y as f32 * 16.0 + 8.0));
        app.insert_resource(Cursor(Some(at)));
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.press(button);
        app.update();
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.release(button);
        mouse.clear();
    }

    /// Another menu taking the input blocks a map click; the tile cursor alone does not.
    #[test]
    fn a_map_click_opens_only_when_play_or_the_tile_cursor_has_the_input() {
        let mut app = app();
        camera(&mut app);
        let empty = MenuSelection {
            selected: 0,
            count: 0,
        };
        let other = app.world_mut().spawn((empty, TakesInput)).id();
        click_cell(&mut app, 1, 0);
        assert!(
            line(&app, "tile_menu").starts_with("open=false "),
            "blocked"
        );
        app.world_mut().entity_mut(other).despawn();
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        click_cell(&mut app, 1, 0);
        let want = "open=true at=(1,0) rows=Step,Look focus=0 refused=0 cursor=none ";
        assert!(
            line(&app, "tile_menu").starts_with(want),
            "{}",
            line(&app, "tile_menu")
        );
        let msgs = app.world().resource::<Messages<TravelTo>>();
        assert_eq!(msgs.get_cursor().read(msgs).count(), 0, "click_cell is off");
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "no Travel");
    }

    /// Enter and Escape in one frame: the last choice, Escape, wins.
    #[test]
    fn escape_beats_enter_in_the_same_frame() {
        let mut app = app();
        open(&mut app, 1, 0);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::Enter);
        keys.press(KeyCode::Escape);
        app.update();
        assert!(line(&app, "tile_menu").starts_with("open=false "));
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "no Step");
    }

    fn rows_of_in(w: &mut World) -> Vec<Entity> {
        let mut q = w.query_filtered::<&Children, With<TileMenu<(), ()>>>();
        q.single(w).unwrap().iter().collect()
    }

    #[test]
    fn opening_another_cell_replaces_the_menu() {
        let mut app = app();
        open(&mut app, 1, 0);
        open(&mut app, 0, 1);
        let menus = app
            .world_mut()
            .query::<&TileMenu<(), ()>>()
            .iter(app.world())
            .count();
        assert_eq!(menus, 1);
        assert!(line(&app, "tile_menu").contains(" at=(0,1) "));
        open(&mut app, 9, 9);
        assert!(
            line(&app, "tile_menu").contains(" at=(0,1) "),
            "off grid: kept"
        );
    }

    #[test]
    fn the_tile_cursor_moves_on_the_grid_without_stepping() {
        let mut app = app();
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        assert!(line(&app, "tile_menu").contains(" cursor=(0,0) "));
        assert!(line(&app, "intent").starts_with("owner=menu "));
        for key in [
            KeyCode::KeyD,
            KeyCode::ArrowUp,
            KeyCode::KeyW,
            KeyCode::KeyA,
            KeyCode::KeyA,
        ] {
            tap(&mut app, key);
        }
        assert!(
            line(&app, "tile_menu").contains(" cursor=(0,2) "),
            "A stops at the edge"
        );
        assert_eq!(line(&app, "outline"), "hover=none target=(0,2)");
        tap(&mut app, KeyCode::KeyS);
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "never steps");
        tap(&mut app, KeyCode::Space);
        assert!(line(&app, "tile_menu").starts_with("open=true at=(0,1) "));
        tap(&mut app, KeyCode::KeyD);
        assert!(
            line(&app, "tile_menu").contains(" cursor=(0,1) "),
            "frozen under its menu"
        );
        tap(&mut app, KeyCode::Escape);
        assert!(line(&app, "tile_menu").starts_with("open=false "));
        assert!(
            line(&app, "tile_menu").contains(" cursor=(0,1) "),
            "Escape closes the menu first"
        );
        tap(&mut app, KeyCode::Escape);
        assert!(line(&app, "tile_menu").contains(" cursor=none "));
        assert!(line(&app, "intent").starts_with("owner=gameplay "));
    }

    #[test]
    fn a_row_from_the_cursor_ends_it_and_a_look_keeps_it() {
        let mut app = app();
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        tap(&mut app, KeyCode::KeyW);
        tap(&mut app, KeyCode::Enter);
        tap(&mut app, KeyCode::KeyS);
        tap(&mut app, KeyCode::Enter);
        assert!(line(&app, "tile_menu").contains(" cursor=(0,1) looks=1"));
        tap(&mut app, KeyCode::Enter);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(sim(&app), (Cell::new(0, 1), 1), "row 0 Steps");
        assert!(line(&app, "tile_menu").contains(" cursor=none "));
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        assert!(
            line(&app, "tile_menu").contains(" cursor=none "),
            "toggles off"
        );
    }

    /// §4.4: Right opens, but only the travel button's second click runs row 0.
    #[test]
    fn a_second_right_click_on_the_open_cell_runs_nothing() {
        let mut app = app();
        camera(&mut app);
        press_cell(&mut app, 1, 0, MouseButton::Right);
        assert!(line(&app, "tile_menu").starts_with("open=true at=(1,0) "));
        press_cell(&mut app, 1, 0, MouseButton::Right);
        assert!(line(&app, "tile_menu").starts_with("open=true at=(1,0) "));
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "no Step");
        click_cell(&mut app, 1, 0);
        assert_eq!(sim(&app), (Cell::new(1, 0), 1), "Left runs row 0");
    }

    /// A press over the open panel (between rows, or on its border) is the UI's:
    /// it neither closes the menu nor counts as a click on the cell beneath.
    #[test]
    fn a_press_on_the_panel_is_not_a_map_click() {
        let mut app = app();
        camera(&mut app);
        click_cell(&mut app, 1, 0);
        let panel = {
            let w = app.world_mut();
            let mut q = w.query_filtered::<Entity, With<TileMenu<(), ()>>>();
            q.single(w).unwrap()
        };
        app.world_mut()
            .entity_mut(panel)
            .insert(Interaction::Hovered);
        click_cell(&mut app, 3, 3);
        assert!(line(&app, "tile_menu").starts_with("open=true at=(1,0) "));
        click_cell(&mut app, 1, 0);
        assert!(line(&app, "tile_menu").starts_with("open=true at=(1,0) "));
        assert_eq!(sim(&app), (Cell::new(0, 0), 0), "nor runs row 0");
    }

    /// `ClickMode::Travel`: the plugin is in, but a map click is `click_cell`'s Travel.
    #[test]
    fn in_travel_mode_a_map_click_travels_and_opens_nothing() {
        let mut app = app();
        camera(&mut app);
        app.insert_resource(ClickMode::Travel);
        click_cell(&mut app, 0, 3);
        assert!(line(&app, "tile_menu").starts_with("open=false "));
        assert_eq!(
            *app.world().resource::<Travel>(),
            Travel(Some(Cell::new(0, 3)))
        );
    }

    /// Two toggles in one frame cancel: the cursor stays off.
    #[test]
    fn two_toggles_in_one_frame_cancel() {
        let mut app = app();
        app.world_mut().write_message(ToggleTileCursor);
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        assert!(line(&app, "tile_menu").contains(" cursor=none "));
        assert!(line(&app, "intent").starts_with("owner=gameplay "));
    }

    /// A game opening a menu away from the tile cursor: the target is the menu's cell.
    #[test]
    fn the_target_outline_follows_the_menu_over_the_tile_cursor() {
        let mut app = app();
        app.world_mut().write_message(ToggleTileCursor);
        app.update();
        open(&mut app, 2, 2);
        assert!(line(&app, "tile_menu").contains(" cursor=(0,0) "));
        assert_eq!(line(&app, "outline"), "hover=none target=(2,2)");
    }

    /// The panel's node is placed beside its cell through the camera.
    #[test]
    fn the_open_panel_is_placed_beside_its_cell() {
        let mut app = app();
        camera(&mut app);
        open(&mut app, 1, 0);
        app.update();
        let w = app.world_mut();
        let mut q = w.query_filtered::<&Node, With<TileMenu<(), ()>>>();
        let node = q.single(w).unwrap();
        assert_eq!((node.left, node.top), (px(36.0), px(64.0)));
    }
}
