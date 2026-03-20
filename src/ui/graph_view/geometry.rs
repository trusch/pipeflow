//! Coordinate transforms and drawing geometry helpers.

use egui::{Color32, Pos2, Vec2};

/// Coordinate transform for graph view.
pub(super) struct GraphTransform {
    pub(super) center: Pos2,
    pub(super) zoom: f32,
    pub(super) pan: Vec2,
}

impl GraphTransform {
    pub(super) fn new(center: Pos2, zoom: f32, pan: Vec2) -> Self {
        Self { center, zoom, pan }
    }

    pub(super) fn graph_to_screen(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            self.center.x + (pos.x * self.zoom) + self.pan.x,
            self.center.y + (pos.y * self.zoom) + self.pan.y,
        )
    }

    pub(super) fn screen_to_graph(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            (pos.x - self.center.x - self.pan.x) / self.zoom,
            (pos.y - self.center.y - self.pan.y) / self.zoom,
        )
    }
}

/// Cubic bezier interpolation.
pub(super) fn cubic_bezier(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;

    Pos2::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

/// Interpolates between two colors.
pub(super) fn interpolate_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cubic_bezier() {
        let p0 = Pos2::new(0.0, 0.0);
        let p1 = Pos2::new(0.0, 1.0);
        let p2 = Pos2::new(1.0, 1.0);
        let p3 = Pos2::new(1.0, 0.0);

        let start = cubic_bezier(p0, p1, p2, p3, 0.0);
        let end = cubic_bezier(p0, p1, p2, p3, 1.0);

        assert!((start.x - p0.x).abs() < 0.001);
        assert!((start.y - p0.y).abs() < 0.001);
        assert!((end.x - p3.x).abs() < 0.001);
        assert!((end.y - p3.y).abs() < 0.001);
    }
}
