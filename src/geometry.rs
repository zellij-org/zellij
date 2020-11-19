/// Handles screen/pane geometry (i.e. lots of rectangles!)
use std::collections::HashMap;

/// Coordinates represent the location of a single character
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Coordinates {
    pub x: usize,
    pub y: usize,
}

impl Coordinates {
    pub fn new(x: usize, y: usize) -> Self {
        Coordinates { x, y }
    }
}

/// An edge
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EdgeType {
    Left,
    Right,
    Top,
    Bottom,
}

/// A particular corner
pub type CornerType = (EdgeType, EdgeType);

/// A rectangle
///
/// Coords start at top-left, increase down and to the right
///
/// All edges, corners and borders are inside the rectangle
pub trait Rectangle {
    fn x(&self) -> usize;
    fn y(&self) -> usize;
    fn columns(&self) -> usize;
    fn rows(&self) -> usize;
    fn borders(&self) -> &HashMap<EdgeType, usize>;

    /// Get a particular edge's coordinate
    fn edge(&self, which_edge: &EdgeType) -> usize {
        match *which_edge {
            EdgeType::Left => self.x(),
            EdgeType::Right => self.x() + self.columns(),
            EdgeType::Top => self.y(),
            EdgeType::Bottom => self.y() + self.rows(),
        }
    }

    /// Get a particular corner's coordinates
    fn corner(&self, which_corner: &CornerType) -> Coordinates {
        match *which_corner {
            (EdgeType::Top, EdgeType::Left)
            | (EdgeType::Left, EdgeType::Top)
            | (EdgeType::Bottom, EdgeType::Left)
            | (EdgeType::Left, EdgeType::Bottom)
            | (EdgeType::Top, EdgeType::Right)
            | (EdgeType::Right, EdgeType::Top)
            | (EdgeType::Bottom, EdgeType::Right)
            | (EdgeType::Right, EdgeType::Bottom) => {
                Coordinates::new(self.edge(&which_corner.0), self.edge(&which_corner.1))
            }
            (_, _) => panic!("Invalid corner type!"),
        }
    }
}
