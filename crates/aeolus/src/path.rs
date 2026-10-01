//! Travel's pure query: which way to step next on the shortest route.

use std::collections::{BTreeMap, VecDeque};

use crate::grid::{Cell, Dir};
use crate::world::{Rules, UnitId, World};

/// A breadth-first search in `Dir::ALL` order, so ties break the same way
/// everywhere. A cell passes if it is on the grid, holds no other solid Unit
/// and `Rules::passes` it for this Unit. A blocked `to` next to the Unit is
/// still a target: the step becomes a bump.
pub fn next_step<G: Rules>(world: &World<G>, unit: UnitId, to: Cell) -> Option<Dir> {
    let walker = world.unit(unit)?;
    let from = walker.cell;
    if from == to {
        return None;
    }
    let passes = |at: Cell| {
        let free = world.solid_at(at).is_none();
        free && world
            .grid()
            .get(at)
            .is_some_and(|t| world.rules().passes(walker, t))
    };
    let mut came_from = BTreeMap::from([(from, from)]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        for dir in Dir::ALL {
            let next = at.step(dir);
            if came_from.contains_key(&next) || world.grid().get(next).is_none() {
                continue;
            }
            let last_hop = next == to && at == from;
            if !last_hop && !passes(next) {
                continue;
            }
            came_from.insert(next, at);
            if next == to {
                let mut first = next;
                while came_from[&first] != from {
                    first = came_from[&first];
                }
                return from.toward(first);
            }
            queue.push_back(next);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Gate;
    use crate::grid::Cell;
    use crate::world::Unit;
    use crate::world::fixture::{T, world};

    const O: Cell = Cell::new(0, 0);

    fn c(x: i32, y: i32) -> Cell {
        Cell::new(x, y)
    }

    #[test]
    fn a_straight_line_steps_toward_it() {
        let (w, u) = world(4, 1, &[], O);
        assert_eq!(next_step(&w, u, c(3, 0)), Some(Dir::East));
        assert_eq!(next_step(&w, u, O), None, "already there");
    }

    #[test]
    fn it_goes_around_a_wall() {
        // . . .
        // o # x
        let (w, u) = world(3, 2, &[(1, 0, T::Wall)], O);
        assert_eq!(next_step(&w, u, c(2, 0)), Some(Dir::North));
    }

    /// Two equal routes: north is tried first, so it wins.
    #[test]
    fn ties_break_by_dir_order() {
        let (w, u) = world(2, 2, &[], O);
        assert_eq!(next_step(&w, u, c(1, 1)), Some(Dir::North));
        let (w, u) = world(2, 2, &[], c(1, 1));
        assert_eq!(next_step(&w, u, O), Some(Dir::South));
    }

    /// Opposite first steps tie round a wall: north beats south, west beats east.
    #[test]
    fn opposite_routes_break_by_dir_order_too() {
        // . . .
        // o # x
        // . . .
        let (w, u) = world(3, 3, &[(1, 1, T::Wall)], c(0, 1));
        assert_eq!(next_step(&w, u, c(2, 1)), Some(Dir::North));
        let (w, u) = world(3, 3, &[(1, 1, T::Wall)], c(1, 0));
        assert_eq!(next_step(&w, u, c(1, 2)), Some(Dir::West));
    }

    #[test]
    fn a_gate_passes_only_if_it_opens_for_the_unit() {
        // o L x, where the lock needs key 5; the long way is walled off.
        let cells = [(1, 0, T::Lock(Gate::single(5))), (1, 1, T::Wall)];
        let (mut w, u) = world(3, 2, &cells, O);
        assert_eq!(next_step(&w, u, c(2, 0)), None);
        w.unit_mut(u).unwrap().ext.push(5);
        assert_eq!(next_step(&w, u, c(2, 0)), Some(Dir::East));
    }

    #[test]
    fn a_blocked_target_next_to_the_unit_is_a_bump() {
        let (w, u) = world(2, 1, &[(1, 0, T::Wall)], O);
        assert_eq!(next_step(&w, u, c(1, 0)), Some(Dir::East));
    }

    #[test]
    fn a_blocked_target_further_away_is_unreachable() {
        let (w, u) = world(3, 1, &[(2, 0, T::Wall)], O);
        assert_eq!(next_step(&w, u, c(2, 0)), None);
    }

    #[test]
    fn off_grid_targets_are_unreachable() {
        let (w, u) = world(2, 1, &[], O);
        assert_eq!(next_step(&w, u, c(-1, 0)), None);
        assert_eq!(next_step(&w, u, c(5, 0)), None);
    }

    #[test]
    fn other_solid_units_block_the_way_but_not_the_last_hop() {
        let (mut w, u) = world(3, 1, &[], O);
        let other = w.spawn(Unit::new("v", 1, c(1, 0), Vec::new()));
        assert_eq!(next_step(&w, u, c(2, 0)), None);
        assert_eq!(next_step(&w, u, c(1, 0)), Some(Dir::East), "a bump");
        w.unit_mut(other).unwrap().solid = false;
        assert_eq!(next_step(&w, u, c(2, 0)), Some(Dir::East));
    }
}
