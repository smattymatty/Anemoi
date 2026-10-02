//! Light: how brightly each cell is seen from one eye, worked out fresh each call.

use crate::grid::{Cell, Grid};
use crate::sight::{line_of_sight, open};
use crate::world::{Rules, World};

/// A level per cell: `radius + 1` at the eye, falling by one a cell, 0 dark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Light {
    levels: Grid<u8>,
}

impl Light {
    /// The level at `at`; 0 off the grid.
    pub fn level(&self, at: Cell) -> u8 {
        self.levels.get(at).copied().unwrap_or(0)
    }

    /// How many cells are lit.
    pub fn lit(&self) -> usize {
        let (w, h) = (self.levels.width(), self.levels.height());
        (0..h)
            .flat_map(|y| (0..w).map(move |x| Cell::new(x, y)))
            .filter(|&at| self.level(at) > 0)
            .count()
    }
}

/// The 8 neighbours, in the reference's order.
const AROUND: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
];

/// The light from `eye` out to `radius`, measured square (Chebyshev).
/// Pass 1: an open cell in sight takes `radius + 1 - dist`. Pass 2: a blocking or
/// gate cell in the square takes its brightest open neighbour, so a seen wall is
/// lit and light never runs along walls. `radius` stops at 254 so levels fit a `u8`.
pub fn light<G: Rules>(world: &World<G>, eye: Cell, radius: u8) -> Light {
    let r = i32::from(radius.min(u8::MAX - 1));
    let grid = world.grid();
    let mut levels = Grid::new(grid.width() as u16, grid.height() as u16, 0u8);
    let square = || {
        (eye.y - r..=eye.y + r)
            .flat_map(move |y| (eye.x - r..=eye.x + r).map(move |x| Cell::new(x, y)))
    };
    for at in square().filter(|&at| open(world, at) && line_of_sight(world, eye, at)) {
        let dist = (at.x - eye.x).abs().max((at.y - eye.y).abs());
        levels.set(at, (r + 1 - dist) as u8);
    }
    // Open cells are final after pass 1, so the order here cannot matter.
    for at in square().filter(|&at| !open(world, at)) {
        let best = AROUND
            .iter()
            .map(|&(dx, dy)| Cell::new(at.x + dx, at.y + dy))
            .filter(|&n| open(world, n))
            .filter_map(|n| levels.get(n).copied())
            .max()
            .unwrap_or(0);
        levels.set(at, best);
    }
    Light { levels }
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

    #[test]
    fn levels_fall_off_square_to_the_edge_and_stop() {
        let (w, _) = world(12, 12, &[], c(0, 0));
        let l = light(&w, c(0, 0), 8);
        assert_eq!(l.level(c(0, 0)), 9, "the eye");
        assert_eq!(l.level(c(3, 1)), 6);
        assert_eq!(l.level(c(8, 3)), 1, "the edge");
        assert_eq!(l.level(c(8, 8)), 1, "a corner of the square");
        assert_eq!(l.level(c(9, 0)), 0, "past it");
        assert_eq!(l.level(c(0, 9)), 0, "past it");
        assert_eq!(l.level(c(-1, 0)), 0, "off the grid");
        assert_eq!(l.lit(), 81);
    }

    #[test]
    fn the_square_is_centred_on_the_eye() {
        let (w, _) = world(9, 9, &[], c(4, 4));
        let l = light(&w, c(4, 4), 2);
        assert_eq!(l.level(c(2, 6)), 1);
        assert_eq!(l.level(c(6, 2)), 1);
        assert_eq!(l.level(c(1, 4)), 0);
        assert_eq!(l.level(c(4, 7)), 0);
        assert_eq!(l.lit(), 25);
    }

    /// The wall takes its floor's level, not its own distance's; no cascade.
    #[test]
    fn a_wall_takes_the_floor_beside_it_and_the_wall_behind_stays_dark() {
        let (w, _) = world(6, 1, &[(2, 0, T::Wall), (3, 0, T::Wall)], c(0, 0));
        let l = light(&w, c(0, 0), 8);
        assert_eq!(l.level(c(1, 0)), 8);
        assert_eq!(l.level(c(2, 0)), 8, "from (1,0)");
        assert_eq!(l.level(c(3, 0)), 0, "only walls beside it are lit");
        assert_eq!(l.level(c(4, 0)), 0, "out of sight");
    }

    /// (1,2) at 7 comes first in neighbour order; (1,1) at 8 is the brightest.
    #[test]
    fn a_wall_takes_its_brightest_open_neighbour() {
        let (w, _) = world(6, 6, &[(2, 2, T::Wall)], c(0, 0));
        let l = light(&w, c(0, 0), 8);
        assert_eq!((l.level(c(1, 2)), l.level(c(1, 1))), (7, 8));
        assert_eq!(l.level(c(2, 2)), 8);
        assert_eq!(l.level(c(3, 3)), 0, "behind it");
    }

    /// A wall column x=3 for y<=4 and a wall row y=4 east of it hide the room.
    #[test]
    fn a_room_behind_a_corner_stays_dark() {
        let mut walls: Vec<_> = (0..=4).map(|y| (3, y, T::Wall)).collect();
        walls.extend((4..10).map(|x| (x, 4, T::Wall)));
        let (w, _) = world(10, 10, &walls, c(1, 1));
        let l = light(&w, c(1, 1), 8);
        for room in [c(4, 0), c(6, 2), c(5, 3), c(8, 1)] {
            assert_eq!(l.level(room), 0, "{room:?}");
        }
        assert_eq!(l.level(c(2, 7)), 3, "past the corner, in sight");
        assert_eq!(l.level(c(3, 1)), 8, "the wall facing the eye");
    }

    #[test]
    fn a_shut_gate_blocks_sight_and_is_lit_from_the_near_side() {
        let lock = T::Lock(Gate::single(1));
        let (w, _) = world(6, 1, &[(3, 0, lock)], c(0, 0));
        let l = light(&w, c(0, 0), 8);
        assert_eq!(l.level(c(2, 0)), 7);
        assert_eq!(l.level(c(3, 0)), 7, "from (2,0)");
        assert_eq!(l.level(c(4, 0)), 0);
    }

    /// The second pass reads the radius square only, as the reference does.
    #[test]
    fn a_wall_just_past_the_radius_stays_dark() {
        let walls = [(1, 3), (5, 3), (3, 1), (3, 5)].map(|(x, y)| (x, y, T::Wall));
        let (w, _) = world(7, 7, &walls, c(3, 3));
        let l = light(&w, c(3, 3), 1);
        assert_eq!(l.level(c(2, 3)), 1);
        for (x, y, _) in walls {
            assert_eq!(l.level(c(x, y)), 0, "({x},{y})");
        }
    }

    #[test]
    fn a_sight_blocking_unit_is_lit_and_shades_what_lies_behind() {
        let (mut w, _) = world(5, 1, &[], c(0, 0));
        w.spawn(Unit::new("screen", 1, c(2, 0), Vec::new()));
        let l = light(&w, c(0, 0), 8);
        assert_eq!(l.level(c(2, 0)), 7);
        assert_eq!(l.level(c(3, 0)), 0);
    }

    #[test]
    fn nothing_is_remembered_between_calls() {
        let (w, _) = world(12, 1, &[], c(0, 0));
        let near = light(&w, c(0, 0), 3);
        assert_eq!(near.level(c(3, 0)), 1);
        let far = light(&w, c(10, 0), 3);
        assert_eq!((far.level(c(0, 0)), far.level(c(3, 0))), (0, 0));
        assert_eq!(far.lit(), 5);
    }

    #[test]
    fn the_biggest_radius_still_fits_a_level() {
        let (w, _) = world(2, 1, &[], c(0, 0));
        let l = light(&w, c(0, 0), u8::MAX);
        assert_eq!((l.level(c(0, 0)), l.level(c(1, 0))), (255, 254));
    }

    /// Radius 0: the eye alone at 1; its open and wall neighbours stay dark.
    #[test]
    fn radius_zero_lights_the_eye_alone() {
        let (w, _) = world(3, 1, &[(2, 0, T::Wall)], c(1, 0));
        let l = light(&w, c(1, 0), 0);
        assert_eq!([0, 1, 2].map(|x| l.level(c(x, 0))), [0, 1, 0]);
        assert_eq!(l.lit(), 1);
    }
}
