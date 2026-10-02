//! Who a press belongs to. The game sets the base; a menu marked `TakesInput`
//! overrides it, so one key never both steps and moves a menu's focus.

use bevy::ecs::query::QueryFilter;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Set by the game: play has the keys, or nobody does (its screen is not play).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputBase {
    #[default]
    Gameplay,
    Nobody,
}

/// Marks a `MenuSelection` that takes the keys while it exists.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct TakesInput;

/// The resolved owner of this frame's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Gameplay,
    Menu,
    Nobody,
}

impl Owner {
    /// The `owner=` token dumps carry.
    pub fn token(self) -> &'static str {
        match self {
            Owner::Gameplay => "gameplay",
            Owner::Menu => "menu",
            Owner::Nobody => "none",
        }
    }
}

fn resolve(base: InputBase, marked: bool) -> Owner {
    match (marked, base) {
        (true, _) => Owner::Menu,
        (false, InputBase::Gameplay) => Owner::Gameplay,
        (false, InputBase::Nobody) => Owner::Nobody,
    }
}

/// Reads the owner in a system; no ordering, no frame of lag. `F` narrows which
/// marks count: the Tile Menu ignores its own tile cursor's.
#[derive(SystemParam)]
pub struct InputOwner<'w, 's, F: QueryFilter + 'static = ()> {
    base: Option<Res<'w, InputBase>>,
    marked: Query<'w, 's, (), (With<TakesInput>, F)>,
}

impl<F: QueryFilter + 'static> InputOwner<'_, '_, F> {
    pub fn get(&self) -> Owner {
        let base = self.base.as_deref().copied().unwrap_or_default();
        resolve(base, !self.marked.is_empty())
    }
}

/// The owner, for dumps.
pub fn of(w: &World) -> Owner {
    let base = w.get_resource::<InputBase>().copied().unwrap_or_default();
    resolve(base, crate::inspect::count::<With<TakesInput>>(w) > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{IntentPlugin, Travel, TravelTo};
    use crate::pace::Sim;
    use crate::pace::tests::{Walls, app};
    use crate::ui::{Focusable, MenuSelection, StyledButton, UiPlugin};
    use aeolus::Cell;

    fn both_app() -> App {
        let mut app = app();
        app.add_plugins((IntentPlugin::<Walls>::default(), UiPlugin));
        app
    }

    /// A two-row menu, marked or not; returns its panel.
    fn menu(app: &mut App, marked: bool) -> Entity {
        let panel = app
            .world_mut()
            .spawn(MenuSelection {
                selected: 0,
                count: 2,
            })
            .id();
        if marked {
            app.world_mut().entity_mut(panel).insert(TakesInput);
        }
        for i in 0..2 {
            let row = (StyledButton, Focusable(i), Interaction::None);
            let row = (row, BackgroundColor::DEFAULT, BorderColor::DEFAULT);
            let id = app.world_mut().spawn(row).id();
            app.world_mut().entity_mut(panel).add_child(id);
        }
        panel
    }

    fn cell(app: &App) -> Cell {
        let sim = app.world().resource::<Sim<Walls>>();
        sim.world().unit(sim.player()).unwrap().cell
    }

    fn selected(app: &App, panel: Entity) -> usize {
        app.world().get::<MenuSelection>(panel).unwrap().selected
    }

    fn keys(app: &mut App) -> Mut<'_, ButtonInput<KeyCode>> {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
    }

    fn intent_line(app: &App) -> String {
        let got = crate::inspect::snapshot(app.world());
        got.into_iter().find(|(n, _)| *n == "intent").unwrap().1
    }

    #[test]
    fn a_marked_menu_takes_the_key_and_the_turn_waits() {
        let mut app = both_app();
        let panel = menu(&mut app, true);
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(selected(&app, panel), 1, "the menu's focus moves");
        assert_eq!(cell(&app), Cell::new(0, 0), "no Step");
        assert!(intent_line(&app).starts_with("owner=menu "));
    }

    #[test]
    fn an_unmarked_menu_leaves_the_key_to_play() {
        let mut app = both_app();
        let panel = menu(&mut app, false);
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "steps");
        assert_eq!(selected(&app, panel), 0, "focus stays");
        assert!(intent_line(&app).starts_with("owner=gameplay "));
    }

    #[test]
    fn base_nobody_takes_no_step_from_a_key_or_a_click() {
        let mut app = both_app();
        app.insert_resource(InputBase::Nobody);
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        app.world_mut().write_message(TravelTo(Cell::new(2, 0)));
        app.update();
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 0));
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
        assert!(intent_line(&app).starts_with("owner=none "));
    }

    /// An open menu zeroes the movement keys: a handback starts still.
    #[test]
    fn a_closed_menu_hands_back_a_still_player() {
        let mut app = both_app();
        app.world_mut().write_message(TravelTo(Cell::new(0, 3)));
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "travelling");
        let panel = menu(&mut app, true);
        app.update();
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        app.world_mut().entity_mut(panel).despawn();
        app.update();
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "the held W waits");
        keys(&mut app).release(KeyCode::KeyW);
        app.update();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 2), "re-pressed, it steps");
    }

    /// A W held into a menu (not pressed under it) also waits after it closes.
    #[test]
    fn a_step_key_held_into_a_menu_waits_after_it_closes() {
        let mut app = both_app();
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        keys(&mut app).clear();
        assert_eq!(cell(&app), Cell::new(0, 1), "walking");
        let panel = menu(&mut app, true);
        app.update();
        app.world_mut().entity_mut(panel).despawn();
        app.update();
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 1), "still until re-pressed");
    }

    /// A click under a menu is spent, not queued for the handback. The click
    /// lands mid-frame, as `click_cell`'s does, so it outlives one update.
    #[test]
    fn a_click_under_a_menu_is_not_replayed_after_it_closes() {
        let mut app = both_app();
        let panel = menu(&mut app, true);
        let click_once = |mut done: Local<bool>, mut out: MessageWriter<TravelTo>| {
            if !std::mem::replace(&mut *done, true) {
                out.write(TravelTo(Cell::new(0, 3)));
            }
        };
        app.add_systems(PreUpdate, click_once);
        app.update();
        app.world_mut().entity_mut(panel).despawn();
        app.update();
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 0), "no Step");
        assert_eq!(*app.world().resource::<Travel>(), Travel(None));
    }

    /// A marked menu with no rows still owns the keys, and W does not panic it.
    #[test]
    fn an_empty_marked_menu_owns_the_keys() {
        let mut app = both_app();
        let empty = MenuSelection {
            selected: 0,
            count: 0,
        };
        app.world_mut().spawn((empty, TakesInput));
        keys(&mut app).press(KeyCode::KeyW);
        app.update();
        assert_eq!(cell(&app), Cell::new(0, 0));
        assert!(intent_line(&app).starts_with("owner=menu "));
    }

    #[derive(Component)]
    struct Ignored;

    /// A filter drops its own marks and keeps every other.
    #[test]
    fn a_filtered_owner_ignores_only_the_filtered_marks() {
        use bevy::ecs::system::RunSystemOnce;
        let owner = |w: &mut World| {
            w.run_system_once(|o: InputOwner<Without<Ignored>>| o.get())
                .unwrap()
        };
        let mut world = World::new();
        world.spawn((TakesInput, Ignored));
        assert_eq!(owner(&mut world), Owner::Gameplay);
        world.insert_resource(InputBase::Nobody);
        assert_eq!(owner(&mut world), Owner::Nobody, "the base still counts");
        world.spawn(TakesInput);
        assert_eq!(owner(&mut world), Owner::Menu);
    }

    #[test]
    fn a_marked_menu_overrides_either_base() {
        assert_eq!(resolve(InputBase::Gameplay, false), Owner::Gameplay);
        assert_eq!(resolve(InputBase::Nobody, false), Owner::Nobody);
        assert_eq!(resolve(InputBase::Gameplay, true), Owner::Menu);
        assert_eq!(resolve(InputBase::Nobody, true), Owner::Menu);
    }
}
