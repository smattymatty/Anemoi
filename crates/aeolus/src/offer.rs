//! A cell's Offers: what a Unit can ask to do there, as Intents.

use crate::grid::Cell;
use crate::world::{Intent, Rules, UnitId, World};

/// Next to the Unit: `Step` first, whatever is there (the Bump decides), then
/// the game's own from `Rules::offers`. On its own cell, the game's only.
/// Distant or off-grid cells, or an unknown Unit, offer nothing.
pub fn offers<G: Rules>(world: &World<G>, unit: UnitId, cell: Cell) -> Vec<Intent<G::Intent>> {
    let Some(at) = world.unit(unit).map(|u| u.cell) else {
        return Vec::new();
    };
    let step = at.toward(cell);
    if world.grid().get(cell).is_none() || (step.is_none() && cell != at) {
        return Vec::new();
    }
    let own = G::offers(world, unit, cell).into_iter().map(Intent::Game);
    step.map(Intent::Step).into_iter().chain(own).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Gate;
    use crate::grid::Dir;
    use crate::world::fixture::{Act, T, world};

    const O: Cell = Cell::new(1, 1);
    const E: Cell = Cell::new(2, 1);

    /// The fixture's hook: `Wait` on the Unit's own cell, two grants elsewhere.
    fn after_step(dir: Dir) -> Vec<Intent<Act>> {
        vec![
            Intent::Step(dir),
            Intent::Game(Act::Grant(1)),
            Intent::Game(Act::Grant(2)),
        ]
    }

    #[test]
    fn a_neighbour_offers_its_step_first_whatever_is_there() {
        for t in [T::Open, T::Wall, T::Lock(Gate::single(5))] {
            let (w, u) = world(3, 3, &[(2, 1, t.clone())], O);
            assert_eq!(offers(&w, u, E), after_step(Dir::East), "{t:?}");
        }
        let (w, u) = world(3, 3, &[], O);
        assert_eq!(offers(&w, u, Cell::new(1, 0)), after_step(Dir::South));
    }

    #[test]
    fn the_own_cell_offers_only_the_games() {
        let (w, u) = world(3, 3, &[], O);
        assert_eq!(offers(&w, u, O), [Intent::Game(Act::Wait)]);
    }

    #[test]
    fn distant_and_off_grid_cells_offer_nothing() {
        let (w, u) = world(3, 3, &[], O);
        for far in [Cell::new(2, 2), Cell::new(1, 3)] {
            assert_eq!(offers(&w, u, far), [], "{far:?}");
        }
        let (w, u) = world(1, 1, &[], Cell::new(0, 0));
        assert_eq!(offers(&w, u, Cell::new(1, 0)), [], "off the grid");
        assert_eq!(offers(&w, UnitId(9), Cell::new(0, 0)), [], "unknown Unit");
    }
}
