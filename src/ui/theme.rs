//! Visual theme and styling.
//!
//! Defines colors, sizes, and visual constants for the application.

use egui::{Color32, CornerRadius};

/// Theme configuration for the application.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background colors
    pub background: BackgroundColors,
    /// Node colors
    pub node: NodeColors,
    /// Port colors
    pub port: PortColors,
    /// Wire colors
    pub wire: WireColors,
    /// Meter colors
    pub meter: MeterColors,
    /// Text colors
    pub text: TextColors,
    /// Sizing
    pub sizes: Sizes,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Creates the dark theme.
    pub fn dark() -> Self {
        Self {
            background: BackgroundColors {
                primary: Color32::from_rgb(30, 30, 35),
                secondary: Color32::from_rgb(40, 40, 48),
                grid: Color32::from_rgb(45, 45, 55),
                selection: Color32::from_rgba_unmultiplied(100, 150, 255, 40),
            },
            node: NodeColors {
                background: Color32::from_rgb(55, 55, 65),
                background_selected: Color32::from_rgb(65, 70, 85),
                border: Color32::from_rgb(80, 80, 90),
                border_selected: Color32::from_rgb(100, 150, 255),
                header_source: Color32::from_rgb(80, 130, 80),
                header_sink: Color32::from_rgb(130, 80, 80),
                header_stream: Color32::from_rgb(80, 100, 140),
                header_other: Color32::from_rgb(90, 90, 100),
            },
            port: PortColors {
                audio_input: Color32::from_rgb(100, 180, 100),
                audio_output: Color32::from_rgb(180, 100, 100),
                midi_input: Color32::from_rgb(100, 100, 180),
                midi_output: Color32::from_rgb(180, 100, 180),
                video_input: Color32::from_rgb(180, 180, 100),
                video_output: Color32::from_rgb(180, 140, 100),
                control: Color32::from_rgb(150, 150, 150),
                monitor: Color32::from_rgb(100, 150, 180),
            },
            wire: WireColors {
                audio: Color32::from_rgb(100, 200, 100),
                audio_hover: Color32::from_rgb(150, 255, 150),
                midi: Color32::from_rgb(100, 100, 200),
                midi_hover: Color32::from_rgb(150, 150, 255),
                video: Color32::from_rgb(200, 200, 100),
                video_hover: Color32::from_rgb(255, 255, 150),
                inactive: Color32::from_rgb(80, 80, 80),
                creating: Color32::from_rgb(200, 200, 200),
            },
            meter: MeterColors {
                background: Color32::from_rgb(25, 25, 30),
                low: Color32::from_rgb(50, 180, 50),
                mid: Color32::from_rgb(200, 200, 50),
                high: Color32::from_rgb(200, 50, 50),
                peak_hold: Color32::from_rgb(255, 255, 255),
            },
            text: TextColors {
                primary: Color32::from_rgb(220, 220, 225),
                secondary: Color32::from_rgb(150, 150, 160),
                muted: Color32::from_rgb(100, 100, 110),
                accent: Color32::from_rgb(100, 180, 255),
                warning: Color32::from_rgb(255, 200, 100),
                error: Color32::from_rgb(255, 100, 100),
            },
            sizes: Sizes::default(),
        }
    }

    /// Creates the light theme.
    pub fn light() -> Self {
        Self {
            background: BackgroundColors {
                primary: Color32::from_rgb(240, 240, 245),
                secondary: Color32::from_rgb(230, 230, 238),
                grid: Color32::from_rgb(215, 215, 225),
                selection: Color32::from_rgba_unmultiplied(60, 120, 220, 40),
            },
            node: NodeColors {
                background: Color32::from_rgb(252, 252, 255),
                background_selected: Color32::from_rgb(235, 240, 255),
                border: Color32::from_rgb(180, 180, 195),
                border_selected: Color32::from_rgb(60, 120, 220),
                header_source: Color32::from_rgb(110, 175, 110),
                header_sink: Color32::from_rgb(185, 105, 105),
                header_stream: Color32::from_rgb(100, 130, 180),
                header_other: Color32::from_rgb(140, 140, 155),
            },
            port: PortColors {
                audio_input: Color32::from_rgb(80, 160, 80),
                audio_output: Color32::from_rgb(170, 80, 80),
                midi_input: Color32::from_rgb(80, 80, 170),
                midi_output: Color32::from_rgb(160, 80, 160),
                video_input: Color32::from_rgb(160, 160, 70),
                video_output: Color32::from_rgb(170, 130, 70),
                control: Color32::from_rgb(120, 120, 120),
                monitor: Color32::from_rgb(80, 130, 160),
            },
            wire: WireColors {
                audio: Color32::from_rgb(70, 170, 70),
                audio_hover: Color32::from_rgb(50, 200, 50),
                midi: Color32::from_rgb(70, 70, 170),
                midi_hover: Color32::from_rgb(80, 80, 220),
                video: Color32::from_rgb(170, 170, 70),
                video_hover: Color32::from_rgb(200, 200, 50),
                inactive: Color32::from_rgb(170, 170, 180),
                creating: Color32::from_rgb(100, 100, 110),
            },
            meter: MeterColors {
                background: Color32::from_rgb(210, 210, 220),
                low: Color32::from_rgb(50, 170, 50),
                mid: Color32::from_rgb(190, 190, 40),
                high: Color32::from_rgb(200, 50, 50),
                peak_hold: Color32::from_rgb(40, 40, 40),
            },
            text: TextColors {
                primary: Color32::from_rgb(30, 30, 40),
                secondary: Color32::from_rgb(80, 80, 95),
                muted: Color32::from_rgb(140, 140, 155),
                accent: Color32::from_rgb(40, 100, 200),
                warning: Color32::from_rgb(180, 130, 30),
                error: Color32::from_rgb(200, 50, 50),
            },
            sizes: Sizes::default(),
        }
    }

    /// Returns the appropriate header color for a media class.
    pub fn header_color_for_media_class(
        &self,
        media_class: Option<&crate::domain::graph::MediaClass>,
    ) -> Color32 {
        use crate::domain::graph::MediaClass;

        match media_class {
            Some(mc) if mc.is_audio() => {
                if matches!(mc, MediaClass::AudioSource | MediaClass::StreamInputAudio) {
                    self.node.header_source
                } else if matches!(mc, MediaClass::AudioSink | MediaClass::StreamOutputAudio) {
                    self.node.header_sink
                } else {
                    self.node.header_stream
                }
            }
            _ => self.node.header_other,
        }
    }

    /// Returns the port color based on direction and type.
    pub fn port_color(
        &self,
        direction: crate::domain::graph::PortDirection,
        is_audio: bool,
        is_midi: bool,
        is_video: bool,
        is_control: bool,
        is_monitor: bool,
    ) -> Color32 {
        use crate::domain::graph::PortDirection;

        if is_control {
            return self.port.control;
        }
        if is_monitor {
            return self.port.monitor;
        }

        match (direction, is_audio, is_midi, is_video) {
            (PortDirection::Input, true, _, _) => self.port.audio_input,
            (PortDirection::Output, true, _, _) => self.port.audio_output,
            (PortDirection::Input, _, true, _) => self.port.midi_input,
            (PortDirection::Output, _, true, _) => self.port.midi_output,
            (PortDirection::Input, _, _, true) => self.port.video_input,
            (PortDirection::Output, _, _, true) => self.port.video_output,
            _ => self.port.control,
        }
    }
}

/// Background colors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BackgroundColors {
    pub primary: Color32,
    pub secondary: Color32,
    pub grid: Color32,
    pub selection: Color32,
}

/// Node colors.
#[derive(Debug, Clone)]
pub struct NodeColors {
    pub background: Color32,
    pub background_selected: Color32,
    pub border: Color32,
    pub border_selected: Color32,
    pub header_source: Color32,
    pub header_sink: Color32,
    pub header_stream: Color32,
    pub header_other: Color32,
}

/// Port colors.
#[derive(Debug, Clone)]
pub struct PortColors {
    pub audio_input: Color32,
    pub audio_output: Color32,
    pub midi_input: Color32,
    pub midi_output: Color32,
    pub video_input: Color32,
    pub video_output: Color32,
    pub control: Color32,
    pub monitor: Color32,
}

/// Wire colors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WireColors {
    pub audio: Color32,
    pub audio_hover: Color32,
    pub midi: Color32,
    pub midi_hover: Color32,
    pub video: Color32,
    pub video_hover: Color32,
    pub inactive: Color32,
    pub creating: Color32,
}

/// Meter colors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MeterColors {
    pub background: Color32,
    pub low: Color32,
    pub mid: Color32,
    pub high: Color32,
    pub peak_hold: Color32,
}

/// Text colors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TextColors {
    pub primary: Color32,
    pub secondary: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub warning: Color32,
    pub error: Color32,
}

/// Size constants.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Sizes {
    /// Default node width
    pub node_width: f32,
    /// Node header height
    pub node_header_height: f32,
    /// Port height
    pub port_height: f32,
    /// Port circle radius
    pub port_radius: f32,
    /// Wire thickness
    pub wire_thickness: f32,
    /// Wire thickness when hovered
    pub wire_thickness_hover: f32,
    /// Node corner rounding
    pub node_rounding: f32,
    /// Default grid spacing
    pub grid_spacing: f32,
    /// Meter width
    pub meter_width: f32,
    /// Meter height (vertical meter)
    pub meter_height: f32,
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            node_width: 200.0,
            node_header_height: 28.0,
            port_height: 22.0,
            port_radius: 6.0,
            wire_thickness: 2.0,
            wire_thickness_hover: 3.5,
            node_rounding: 6.0,
            grid_spacing: 20.0,
            meter_width: 8.0,
            meter_height: 100.0,
        }
    }
}

impl Sizes {
    /// Returns the rounding for nodes.
    pub fn node_rounding(&self) -> CornerRadius {
        CornerRadius::same(self.node_rounding as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_creation() {
        let dark = Theme::dark();

        // Dark theme should have dark backgrounds (low RGB values)
        assert!(dark.background.primary.r() < 100);
        assert!(dark.background.primary.g() < 100);
        assert!(dark.background.primary.b() < 100);
    }

    #[test]
    fn test_light_theme_creation() {
        let light = Theme::light();

        // Light theme should have light backgrounds (high RGB values)
        assert!(light.background.primary.r() > 200);
        assert!(light.background.primary.g() > 200);
        assert!(light.background.primary.b() > 200);

        // Light theme text should be dark
        assert!(light.text.primary.r() < 100);
        assert!(light.text.primary.g() < 100);
        assert!(light.text.primary.b() < 100);
    }

    #[test]
    fn test_sizes_default() {
        let sizes = Sizes::default();

        assert!(sizes.node_width > 0.0);
        assert!(sizes.port_height > 0.0);
        assert!(sizes.wire_thickness > 0.0);
    }
}
