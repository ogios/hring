// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Encode, Decode)]
pub struct AppLink {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

#[derive(Debug, Encode, Decode)]
pub struct App {
    pub bind: String,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

#[derive(Debug, Encode, Decode)]
pub struct Group {
    pub bind: String,
    pub apps: Vec<App>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    QuadraticIn,
    QuadraticOut,
    QuadraticInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    ExponentialIn,
    ExponentialOut,
    SinIn,
    SinOut,
    SinInOut,
}

/// Fallback for `graphic.toml` files written before the easing option existed.
fn default_easing() -> Easing {
    Easing::QuadraticOut
}

impl Easing {
    /// Maps the configured curve to the matching `egui` easing function.
    pub fn function(self) -> fn(f32) -> f32 {
        use eframe::egui::emath::easing;

        match self {
            Easing::Linear => easing::linear,
            Easing::QuadraticIn => easing::quadratic_in,
            Easing::QuadraticOut => easing::quadratic_out,
            Easing::QuadraticInOut => easing::quadratic_in_out,
            Easing::CubicIn => easing::cubic_in,
            Easing::CubicOut => easing::cubic_out,
            Easing::CubicInOut => easing::cubic_in_out,
            Easing::ExponentialIn => easing::exponential_in,
            Easing::ExponentialOut => easing::exponential_out,
            Easing::SinIn => easing::sin_in,
            Easing::SinOut => easing::sin_out,
            Easing::SinInOut => easing::sin_in_out,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode)]
pub struct Graphic {
    // Panels
    pub main_panel_color: (u8, u8, u8, u8),
    pub left_panel_color: (u8, u8, u8, u8),
    pub left_panel_width: f32,

    // Animation
    /// Easing curve used for the tab and page transitions. Older configs that
    /// predate this field fall back to `quadratic_out`.
    #[serde(default = "default_easing")]
    pub animation_easing: Easing,

    // Menu Panel
    pub menu_items_hover_color: (u8, u8, u8, u8),
    pub menu_items_font_color: (u8, u8, u8, u8),
    pub menu_items_font_size: f32,
    // Main Panel
    pub middle_text: String,
    pub center_color: (u8, u8, u8, u8),
    pub middle_text_color: (u8, u8, u8, u8),
    pub middle_text_size: f32,
    pub center_radius: f32,

    // Apps
    pub app_color_active: (u8, u8, u8, u8),
    pub app_color_unactive: (u8, u8, u8, u8),
    pub app_font_color_active: (u8, u8, u8, u8),
    pub app_font_color_unactive: (u8, u8, u8, u8),
    pub app_title_font_color_active: (u8, u8, u8, u8),
    pub app_title_font_color_unactive: (u8, u8, u8, u8),
    pub app_title_background_color_active: (u8, u8, u8, u8),
    pub app_title_background_color_unactive: (u8, u8, u8, u8),

    pub app_font_size_active: f32,
    pub app_font_size_unactive: f32,
    pub app_title_font_size_active: f32,
    pub app_title_font_size_unactive: f32,

    pub app_radius: f32,
    pub app_offset: f32,
    pub apps_spacing_rad: f32,
    pub app_title_offset: f32,
    pub app_title_background_paddings: (f32, f32),

    // Lines
    pub line_color_active: (u8, u8, u8, u8),
    pub line_color_unactive: (u8, u8, u8, u8),

    pub line_width_active: f32,
    pub line_width_unactive: f32,

    pub line_point_scale_1: f32,
    pub line_point_scale_2: f32,
    pub line_point_scale_3: f32,

    // Segments
    pub segment_color_active: (u8, u8, u8, u8),
    pub segment_color_unactive: (u8, u8, u8, u8),
    pub segment_stroke_color_active: (u8, u8, u8, u8),
    pub segment_stroke_color_unactive: (u8, u8, u8, u8),
    pub segment_bind_color_active: (u8, u8, u8, u8),
    pub segment_bind_color_unactive: (u8, u8, u8, u8),
    pub segment_bind_font_color_active: (u8, u8, u8, u8),
    pub segment_bind_font_color_unactive: (u8, u8, u8, u8),

    pub segment_stroke_width_active: f32,
    pub segment_stroke_width_unactive: f32,
    pub segment_bind_font_size_active: f32,
    pub segment_bind_font_size_unactive: f32,

    pub segment_radius: f32,
    pub segment_points_count: u16,
    pub segment_bind_radius: f32,

    // Radar
    pub radar_color: (u8, u8, u8, u8),
    pub radar_stroke_color: (u8, u8, u8, u8),

    pub radar_stroke_width: f32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GraphicConfig {
    pub graphic: Graphic,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BindsConfig {
    pub groups: Vec<ConfGroup>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub pathes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfApp {
    pub bind: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfGroup {
    pub bind: String,
    pub apps: Vec<ConfApp>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configs written before `animation_easing` existed must still load and
    /// fall back to the default curve.
    #[test]
    fn missing_easing_falls_back_to_default() {
        let full = toml::to_string(&GraphicConfig::default()).unwrap();
        assert!(full.contains("animation_easing"));

        let without: String = full
            .lines()
            .filter(|line| !line.contains("animation_easing"))
            .collect::<Vec<_>>()
            .join("\n");

        let parsed: GraphicConfig = toml::from_str(&without).unwrap();
        assert_eq!(parsed.graphic.animation_easing, Easing::QuadraticOut);
    }
}
