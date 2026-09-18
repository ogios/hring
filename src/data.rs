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
        use egui::emath::easing;

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

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[serde(default)]
pub struct AllPrograms {
    /// Name font size is `menu_items_font_size * font_scale`, at least
    /// `font_size_min`.
    pub font_scale: f32,
    pub font_size_min: f32,
    /// Overdraw offset, in points, used to fake a bold name. `0` disables it.
    pub font_bold_offset: f32,
    /// Icon size is `app_radius * icon_radius_scale`, clamped to these bounds.
    pub icon_radius_scale: f32,
    pub icon_size_min: f32,
    pub icon_size_max: f32,
    /// Gutter between cards.
    pub gap: f32,
    /// Padding inside a card.
    pub padding_x: f32,
    pub padding_top: f32,
    pub padding_bottom: f32,
    /// Space between the icon and the name.
    pub icon_text_gap: f32,
    /// A card is at least this wide, and at least
    /// `icon_size + padding_x * 2 + cell_width_extra`.
    pub min_cell_width: f32,
    pub cell_width_extra: f32,
    /// The grid is centered and never wider than this.
    pub max_content_width: f32,
    /// How far one mouse-wheel notch scrolls the grid, in points. egui's own
    /// default (`40.0`) feels sluggish on a full-screen grid, so this raises it.
    pub scroll_speed: f32,
    /// Height reserved for the name, in line heights, and the line-height
    /// factor used to count how many lines fit.
    pub text_lines: f32,
    pub text_line_height: f32,
    /// Card corner radius and background alpha when idle/hovered.
    pub corner_radius: f32,
    pub idle_alpha: u8,
    pub hover_alpha: u8,

    /// Prompt drawn at the start of the filter bar, NeoVim-style.
    pub search_prompt: String,
    /// Font size of the filter text. Monospace, so it stays readable large.
    pub search_font_size: f32,
    /// Total height of the filter bar.
    pub search_bar_height: f32,
    /// Horizontal padding inside the filter bar.
    pub search_padding_x: f32,
    pub search_corner_radius: f32,
    /// Background alpha of the filter bar when search mode is idle/active.
    pub search_idle_alpha: u8,
    pub search_active_alpha: u8,
}

#[derive(Debug, Serialize, Deserialize, Encode, Decode)]
pub struct Graphic {
    // Panels
    pub main_panel_color: (u8, u8, u8, u8),
    pub left_panel_color: (u8, u8, u8, u8),
    pub left_panel_width: f32,

    /// Tuning for the "All Programs" application grid. Missing fields fall back
    /// to their default, so the table can be edited one value at a time.
    #[serde(default)]
    pub all_programs: AllPrograms,

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

    /// The `[graphic.all_programs]` table is optional, and when present it may
    /// list only the fields being tuned.
    #[test]
    fn all_programs_table_is_optional_and_partial() {
        let full = toml::to_string(&GraphicConfig::default()).unwrap();
        assert!(full.contains("[graphic.all_programs]"));

        let without = full
            .split("[graphic.all_programs]")
            .next()
            .unwrap()
            .to_string();

        let parsed: GraphicConfig = toml::from_str(&without).unwrap();
        assert_eq!(parsed.graphic.all_programs.font_scale, 1.8);

        let partial = format!("{without}\n[graphic.all_programs]\ngap = 30.0\n");
        let parsed: GraphicConfig = toml::from_str(&partial).unwrap();
        assert_eq!(parsed.graphic.all_programs.gap, 30.0);
        assert_eq!(parsed.graphic.all_programs.padding_x, 18.0);
    }
}
