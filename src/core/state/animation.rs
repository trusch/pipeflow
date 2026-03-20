//! Animation state for node position transitions.

use crate::util::spatial::Position;

/// Animation state for a node position.
#[derive(Debug, Clone, Copy)]
pub struct PositionAnimation {
    /// Starting position
    pub from: Position,
    /// Target position
    pub to: Position,
    /// Animation progress (0.0 to 1.0)
    pub progress: f32,
    /// Animation speed (progress per second)
    pub speed: f32,
}

impl PositionAnimation {
    /// Creates a new animation.
    pub fn new(from: Position, to: Position, speed: f32) -> Self {
        Self {
            from,
            to,
            progress: 0.0,
            speed,
        }
    }

    /// Fast animation for short-lived nodes (like notification sounds).
    pub(crate) fn fast(from: Position, to: Position) -> Self {
        Self::new(from, to, 8.0) // Complete in ~125ms
    }

    /// Normal animation speed.
    pub(crate) fn normal(from: Position, to: Position) -> Self {
        Self::new(from, to, 5.0) // Complete in ~200ms
    }

    /// Returns the current interpolated position.
    pub fn current_position(&self) -> Position {
        // Use smooth ease-out interpolation
        let t = self.ease_out(self.progress);
        Position::new(
            self.from.x + (self.to.x - self.from.x) * t,
            self.from.y + (self.to.y - self.from.y) * t,
        )
    }

    /// Updates the animation progress. Returns true if animation is complete.
    pub fn update(&mut self, dt: f32) -> bool {
        self.progress = (self.progress + self.speed * dt).min(1.0);
        self.progress >= 1.0
    }

    /// Ease-out cubic function for smooth deceleration.
    fn ease_out(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }
}
