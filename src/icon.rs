//! Application icon generation.
//!
//! Generates the application icon programmatically for embedding.

use egui::IconData;

/// Icon size in pixels (square).
const ICON_SIZE: u32 = 256;

/// Generates the application icon.
///
/// Creates a stylized flow/routing icon representing audio signal routing.
pub fn create_app_icon() -> IconData {
    let size = ICON_SIZE as usize;
    let mut rgba = vec![0u8; size * size * 4];

    // Colors (RGBA)
    let bg_color = [0x1a, 0x1a, 0x2e, 0xff]; // Dark blue-gray background
    let accent_color = [0x00, 0xd4, 0xaa, 0xff]; // Teal/cyan accent (matches theme)
    let secondary_color = [0x60, 0x60, 0x80, 0xff]; // Muted purple-gray

    // Fill background with rounded rectangle effect
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let corner_radius = 48.0;

            // Calculate distance from nearest corner for rounded effect
            let fx = x as f32;
            let fy = y as f32;
            let fsize = size as f32;

            let in_rounded_rect = is_in_rounded_rect(fx, fy, fsize, fsize, corner_radius);

            if in_rounded_rect {
                rgba[idx..idx + 4].copy_from_slice(&bg_color);
            } else {
                rgba[idx..idx + 4].copy_from_slice(&[0, 0, 0, 0]); // Transparent
            }
        }
    }

    // Draw flow lines representing audio routing
    // Three curved lines going from left to right with connections

    let center_y = size as f32 / 2.0;
    let margin = 40.0;
    let line_spacing = 50.0;

    // Draw three horizontal flow lines with slight curves
    for (i, y_offset) in [-1.0, 0.0, 1.0].iter().enumerate() {
        let base_y = center_y + y_offset * line_spacing;
        let color = if i == 1 { &accent_color } else { &secondary_color };
        let thickness = if i == 1 { 12.0 } else { 8.0 };

        // Draw a flowing curve
        for x in (margin as usize)..(size - margin as usize) {
            let fx = x as f32;
            let progress = (fx - margin) / (size as f32 - 2.0 * margin);

            // Sine wave with phase offset for visual interest
            let wave = (progress * std::f32::consts::PI * 2.0 + i as f32 * 0.5).sin() * 15.0;
            let y = base_y + wave;

            draw_circle(&mut rgba, size, fx, y, thickness / 2.0, color);
        }
    }

    // Draw connection nodes (circles at endpoints)
    let node_radius = 16.0;
    let node_positions = [
        (margin + 10.0, center_y - line_spacing),
        (margin + 10.0, center_y),
        (margin + 10.0, center_y + line_spacing),
        (size as f32 - margin - 10.0, center_y - line_spacing),
        (size as f32 - margin - 10.0, center_y),
        (size as f32 - margin - 10.0, center_y + line_spacing),
    ];

    for (i, (nx, ny)) in node_positions.iter().enumerate() {
        let color = if i == 1 || i == 4 {
            &accent_color
        } else {
            &secondary_color
        };
        draw_filled_circle(&mut rgba, size, *nx, *ny, node_radius, color);
        // Inner darker circle for depth
        draw_filled_circle(&mut rgba, size, *nx, *ny, node_radius * 0.5, &bg_color);
    }

    // Draw a central "hub" node
    draw_filled_circle(&mut rgba, size, center_y, center_y, 24.0, &accent_color);
    draw_filled_circle(&mut rgba, size, center_y, center_y, 12.0, &bg_color);

    IconData {
        rgba,
        width: ICON_SIZE,
        height: ICON_SIZE,
    }
}

/// Checks if a point is inside a rounded rectangle.
fn is_in_rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> bool {
    // Check if inside the main rectangle minus corners
    if x >= radius && x <= width - radius && y >= 0.0 && y <= height {
        return true;
    }
    if y >= radius && y <= height - radius && x >= 0.0 && x <= width {
        return true;
    }

    // Check corners
    let corners = [
        (radius, radius),                     // top-left
        (width - radius, radius),             // top-right
        (radius, height - radius),            // bottom-left
        (width - radius, height - radius),    // bottom-right
    ];

    for (cx, cy) in corners {
        let dx = x - cx;
        let dy = y - cy;
        if dx * dx + dy * dy <= radius * radius {
            return true;
        }
    }

    false
}

/// Draws a filled circle with anti-aliasing.
fn draw_filled_circle(rgba: &mut [u8], size: usize, cx: f32, cy: f32, radius: f32, color: &[u8; 4]) {
    let min_x = (cx - radius - 1.0).max(0.0) as usize;
    let max_x = (cx + radius + 1.0).min(size as f32 - 1.0) as usize;
    let min_y = (cy - radius - 1.0).max(0.0) as usize;
    let max_y = (cy + radius + 1.0).min(size as f32 - 1.0) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let idx = (y * size + x) * 4;
                // Anti-aliasing at edges
                let alpha = if dist > radius - 1.0 {
                    ((radius - dist) * color[3] as f32) as u8
                } else {
                    color[3]
                };

                // Alpha blend
                blend_pixel(&mut rgba[idx..idx + 4], color, alpha);
            }
        }
    }
}

/// Draws a circle outline (used for flow lines).
fn draw_circle(rgba: &mut [u8], size: usize, cx: f32, cy: f32, radius: f32, color: &[u8; 4]) {
    draw_filled_circle(rgba, size, cx, cy, radius, color);
}

/// Alpha blends a pixel.
fn blend_pixel(dest: &mut [u8], src: &[u8; 4], alpha: u8) {
    let a = alpha as f32 / 255.0;
    let inv_a = 1.0 - a;

    dest[0] = (src[0] as f32 * a + dest[0] as f32 * inv_a) as u8;
    dest[1] = (src[1] as f32 * a + dest[1] as f32 * inv_a) as u8;
    dest[2] = (src[2] as f32 * a + dest[2] as f32 * inv_a) as u8;
    dest[3] = (alpha as f32 + dest[3] as f32 * inv_a).min(255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_app_icon() {
        let icon = create_app_icon();
        assert_eq!(icon.width, ICON_SIZE);
        assert_eq!(icon.height, ICON_SIZE);
        assert_eq!(icon.rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn test_rounded_rect() {
        // Center should be inside
        assert!(is_in_rounded_rect(50.0, 50.0, 100.0, 100.0, 10.0));
        // Far outside should not be
        assert!(!is_in_rounded_rect(-10.0, -10.0, 100.0, 100.0, 10.0));
    }
}
