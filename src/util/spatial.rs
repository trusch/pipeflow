//! Spatial utilities for graph layout.
//!
//! Contains algorithms and helpers for positioning nodes in the graph view.

use serde::{Deserialize, Serialize};

/// A 2D position in the graph canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    /// Creates a new position.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the zero position (origin).
    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Calculates the distance to another position.
    pub fn distance_to(&self, other: &Position) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Returns a new position offset by the given delta.
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// Converts to egui Pos2.
    pub fn to_pos2(self) -> egui::Pos2 {
        egui::Pos2::new(self.x, self.y)
    }

    /// Creates from egui Pos2.
    pub fn from_pos2(pos: egui::Pos2) -> Self {
        Self { x: pos.x, y: pos.y }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<egui::Pos2> for Position {
    fn from(pos: egui::Pos2) -> Self {
        Self::from_pos2(pos)
    }
}

impl From<Position> for egui::Pos2 {
    fn from(pos: Position) -> Self {
        pos.to_pos2()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_distance() {
        let p1 = Position::new(0.0, 0.0);
        let p2 = Position::new(3.0, 4.0);

        assert!((p1.distance_to(&p2) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_position_offset() {
        let p = Position::new(10.0, 20.0);
        let offset = p.offset(5.0, -5.0);

        assert_eq!(offset.x, 15.0);
        assert_eq!(offset.y, 15.0);
    }
}
