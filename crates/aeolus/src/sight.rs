//! Sight's pure query: whether one cell can be seen from another.

use crate::grid::Cell;
use crate::world::{Kind, Rules, World};

/// Whether `to` is seen from `from`: no cell strictly between them blocks.
/// A cell blocks if it is off the grid, its terrain is `Blocking` or a `Gate`,
/// or a Unit on it `Rules::blocks_sight`. `to` itself never blocks, so a wall
/// is seen. The walk is Bresenham's: a diagonal step skips both corners.
pub fn line_of_sight<G: Rules>(world: &World<G>, from: Cell, to: Cell) -> bool {
    !between(from, to).any(|at| blocks(world, at))
}

fn blocks<G: Rules>(world: &World<G>, at: Cell) -> bool {
    !open(world, at)
        || world
            .units()
            .any(|(_, u)| u.cell == at && world.rules().blocks_sight(u))
}

/// Whether `at` is on the grid and its terrain is `Passable`.
pub(crate) fn open<G: Rules>(world: &World<G>, at: Cell) -> bool {
    let terrain = world.grid().get(at);
    terrain.is_some_and(|t| matches!(world.rules().kind(t), Kind::Passable))
}

/// The cells strictly between `from` and `to`, in Bresenham's step order.
fn between(from: Cell, to: Cell) -> impl Iterator<Item = Cell> {
    let (dx, dy) = ((to.x - from.x).abs(), (to.y - from.y).abs());
    let sx = if from.x < to.x { 1 } else { -1 };
    let sy = if from.y < to.y { 1 } else { -1 };
    let mut err = dx - dy;
    let mut at = from;
    std::iter::from_fn(move || {
        if at == to {
            return None;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            at.x += sx;
        }
        if e2 < dx {
            err += dx;
            at.y += sy;
        }
        (at != to).then_some(at)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Gate;
    use crate::world::Unit;
    use crate::world::fixture::{T, world};

    fn c(x: i32, y: i32) -> Cell {
        Cell::new(x, y)
    }

    fn cells(pairs: &[(i32, i32)]) -> Vec<Cell> {
        pairs.iter().map(|&(x, y)| c(x, y)).collect()
    }

    /// Goldens from the reference loop, compiled and run as written.
    #[test]
    fn the_walk_visits_the_reference_cells_in_order() {
        let cases: [((i32, i32), (i32, i32), &[(i32, i32)]); 10] = [
            ((0, 0), (5, 2), &[(1, 0), (2, 1), (3, 1), (4, 2)]),
            ((5, 2), (0, 0), &[(4, 2), (3, 1), (2, 1), (1, 0)]),
            ((1, 1), (3, 6), &[(1, 2), (2, 3), (2, 4), (3, 5)]),
            ((4, 4), (0, 1), &[(3, 3), (2, 3), (1, 2)]),
            ((0, 0), (2, 2), &[(1, 1)]),
            ((0, 0), (1, 1), &[]),
            ((3, 3), (3, 3), &[]),
            ((0, 0), (4, 0), &[(1, 0), (2, 0), (3, 0)]),
            (
                (0, 0),
                (7, 8),
                &[(1, 1), (2, 2), (3, 3), (3, 4), (4, 5), (5, 6), (6, 7)],
            ),
            (
                (0, 0),
                (8, 7),
                &[(1, 1), (2, 2), (3, 3), (4, 3), (5, 4), (6, 5), (7, 6)],
            ),
        ];
        for ((fx, fy), (tx, ty), want) in cases {
            let got: Vec<_> = between(c(fx, fy), c(tx, ty)).collect();
            assert_eq!(got, cells(want), "({fx},{fy}) to ({tx},{ty})");
        }
    }

    #[test]
    fn an_open_line_is_clear() {
        let (w, _) = world(6, 3, &[], c(0, 0));
        assert!(line_of_sight(&w, c(0, 0), c(5, 2)));
    }

    #[test]
    fn a_wall_between_blocks() {
        let (w, _) = world(6, 3, &[(2, 1, T::Wall)], c(0, 0));
        assert!(!line_of_sight(&w, c(0, 0), c(5, 2)));
    }

    #[test]
    fn a_shut_gate_between_blocks() {
        let (w, _) = world(5, 1, &[(2, 0, T::Lock(Gate::single(1)))], c(0, 0));
        assert!(!line_of_sight(&w, c(0, 0), c(4, 0)));
    }

    #[test]
    fn the_blocking_cell_itself_is_seen() {
        let (w, _) = world(5, 1, &[(2, 0, T::Wall), (3, 0, T::Wall)], c(0, 0));
        assert!(line_of_sight(&w, c(0, 0), c(2, 0)));
        assert!(!line_of_sight(&w, c(0, 0), c(3, 0)));
    }

    /// A diagonal step passes between two walls that touch at a corner.
    #[test]
    fn a_diagonal_slips_past_both_corners() {
        let (w, _) = world(3, 3, &[(1, 0, T::Wall), (0, 1, T::Wall)], c(0, 0));
        assert!(line_of_sight(&w, c(0, 0), c(1, 1)));
        assert!(line_of_sight(&w, c(0, 0), c(2, 2)));
    }

    /// (0,0) to (5,2) steps on (2,1), never on (2,0) beside it.
    #[test]
    fn only_the_cells_walked_can_block() {
        let (w, _) = world(6, 3, &[(2, 0, T::Wall), (1, 1, T::Wall)], c(0, 0));
        assert!(line_of_sight(&w, c(0, 0), c(5, 2)));
    }

    #[test]
    fn a_sight_blocking_unit_blocks_and_a_solid_one_does_not() {
        let (mut w, _) = world(5, 1, &[], c(0, 0));
        w.spawn(Unit::new("u", 1, c(2, 0), Vec::new()));
        assert!(line_of_sight(&w, c(0, 0), c(4, 0)), "solid only");
        let mut screen = Unit::new("screen", 1, c(3, 0), Vec::new());
        screen.solid = false;
        w.spawn(screen);
        assert!(!line_of_sight(&w, c(0, 0), c(4, 0)), "blocks sight");
        assert!(line_of_sight(&w, c(0, 0), c(3, 0)), "seen itself");
    }

    #[test]
    fn leaving_the_grid_blocks() {
        let (w, _) = world(3, 3, &[], c(0, 0));
        assert!(!line_of_sight(&w, c(1, 1), c(5, 1)));
        assert!(line_of_sight(&w, c(1, 1), c(3, 1)), "only between");
    }
}
