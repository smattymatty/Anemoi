//! Cells, the four directions and a rectangular grid of terrain. `y` grows north.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn step(self, dir: Dir) -> Self {
        let (dx, dy) = dir.offset();
        Self::new(self.x + dx, self.y + dy)
    }

    /// The direction from `self` to a neighbouring `to`, if it is one.
    pub fn toward(self, to: Self) -> Option<Dir> {
        Dir::ALL.into_iter().find(|&d| self.step(d) == to)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    North,
    South,
    West,
    East,
}

impl Dir {
    /// Fixed order: pathfinding breaks ties by it, so it is part of replay.
    pub const ALL: [Dir; 4] = [Dir::North, Dir::South, Dir::West, Dir::East];

    pub const fn offset(self) -> (i32, i32) {
        match self {
            Dir::North => (0, 1),
            Dir::South => (0, -1),
            Dir::West => (-1, 0),
            Dir::East => (1, 0),
        }
    }
}

/// `width * height` terrain values, row by row from (0, 0). Outside it there is
/// no cell at all: a step there does nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Grid<T> {
    width: i32,
    height: i32,
    cells: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn new(width: u16, height: u16, fill: T) -> Self {
        let cells = vec![fill; usize::from(width) * usize::from(height)];
        Self {
            width: width.into(),
            height: height.into(),
            cells,
        }
    }
}

impl<T> Grid<T> {
    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    fn index(&self, at: Cell) -> Option<usize> {
        let inside = (0..self.width).contains(&at.x) && (0..self.height).contains(&at.y);
        inside.then(|| (at.y * self.width + at.x) as usize)
    }

    pub fn get(&self, at: Cell) -> Option<&T> {
        self.index(at).map(|i| &self.cells[i])
    }

    /// Replaces the terrain at `at`; returns the old value, or `None` outside.
    pub fn set(&mut self, at: Cell, terrain: T) -> Option<T> {
        let i = self.index(at)?;
        Some(std::mem::replace(&mut self.cells[i], terrain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dir_steps_one_cell_its_way() {
        let o = Cell::new(5, 5);
        let got = Dir::ALL.map(|d| o.step(d));
        let want = [(5, 6), (5, 4), (4, 5), (6, 5)].map(|(x, y)| Cell::new(x, y));
        assert_eq!(got, want);
    }

    #[test]
    fn toward_names_neighbours_only() {
        let o = Cell::new(0, 0);
        assert_eq!(o.toward(Cell::new(1, 0)), Some(Dir::East));
        assert_eq!(o.toward(Cell::new(0, -1)), Some(Dir::South));
        assert_eq!(o.toward(Cell::new(1, 1)), None);
        assert_eq!(o.toward(o), None);
    }

    #[test]
    fn a_grid_keeps_each_cell_and_ends_at_its_edges() {
        let mut g = Grid::new(3, 2, 0u8);
        assert_eq!(g.set(Cell::new(2, 1), 7), Some(0));
        assert_eq!(g.set(Cell::new(1, 0), 4), Some(0));
        assert_eq!(g.get(Cell::new(2, 1)), Some(&7));
        assert_eq!(g.get(Cell::new(1, 0)), Some(&4));
        assert_eq!(g.get(Cell::new(1, 1)), Some(&0));
        for out in [(-1, 0), (0, -1), (3, 0), (0, 2)] {
            assert_eq!(g.get(Cell::new(out.0, out.1)), None, "{out:?}");
            assert_eq!(g.set(Cell::new(out.0, out.1), 9), None, "{out:?}");
        }
    }
}
