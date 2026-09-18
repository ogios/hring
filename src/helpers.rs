// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use core::f32;
use std::{path::Path, process::Command};

use eframe::{
    egui::{self, Align2, Color32, FontId, Key, Pos2, Stroke, Vec2, ViewportCommand},
    emath::Rot2,
    epaint::{self, PathShape, PathStroke},
};

use crate::{app::Hring, data::App, icon};

/// How many decoded icons are uploaded per frame. Spreading them keeps a large
/// batch from turning a single repaint into a visible hang.
const ICON_UPLOADS_PER_FRAME: usize = 16;

impl Hring {
    pub fn get_key(key: &str) -> Option<Key> {
        if let Some(key) = Key::from_name(key) {
            Some(key)
        } else {
            println!("bind '{key}' not match!");
            None
        }
    }

    pub fn exec_app(ctx: &eframe::egui::Context, exec_str: &str) {
        _ = Command::new("sh").arg("-c").arg(exec_str).spawn().ok();
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    pub fn draw_radar(&self, painter: &egui::Painter, center: Pos2, start: f32, end: f32) {
        let g = &self.graphic;

        let (color, stroke_color, stroke_width) = (
            Self::get_color32(g.radar_color),
            Self::get_color32(g.radar_stroke_color),
            g.radar_stroke_width,
        );

        painter.add(PathShape::convex_polygon(
            vec![
                center,
                center + Vec2::new(5000.0 * start.cos(), 5000.0 * -start.sin()),
                center + Vec2::new(5000.0 * end.cos(), 5000.0 * -end.sin()),
            ],
            color,
            PathStroke::new(stroke_width, stroke_color),
        ));
    }

    pub fn draw_segment(
        &self,
        painter: &egui::Painter,
        center: Pos2,
        start: f32,
        end: f32,
        is_selected: bool,
        group_bind: String,
    ) {
        let g = &self.graphic;

        let (
            segment_color,
            segment_stroke_color,
            segment_bind_color,
            segment_bind_font_color,
            segment_bind_font_size,
            segment_stroke_width,
        ) = if is_selected {
            (
                Self::get_color32(g.segment_color_active),
                Self::get_color32(g.segment_stroke_color_active),
                Self::get_color32(g.segment_bind_color_active),
                Self::get_color32(g.segment_bind_font_color_active),
                g.segment_bind_font_size_active,
                g.segment_stroke_width_active,
            )
        } else {
            (
                Self::get_color32(g.segment_color_unactive),
                Self::get_color32(g.segment_stroke_color_unactive),
                Self::get_color32(g.segment_bind_color_unactive),
                Self::get_color32(g.segment_bind_font_color_unactive),
                g.segment_bind_font_size_unactive,
                g.segment_stroke_width_unactive,
            )
        };

        let mut points: Vec<Pos2> = Vec::new();

        let step = (end - start) / (f32::from(g.segment_points_count));

        points.push(center);

        for i in 0..=g.segment_points_count {
            points.push(
                center
                    + Vec2::new(
                        g.segment_radius * (start + (step * f32::from(i))).cos(),
                        g.segment_radius * -(start + (step * f32::from(i))).sin(),
                    ),
            );
        }

        painter.add(PathShape::convex_polygon(
            points,
            segment_color,
            Stroke::new(segment_stroke_width, segment_stroke_color),
        ));

        let mid_rad = start + (end - start) / 2.0;
        let bind_pos = center
            + Vec2::new(
                g.segment_radius * mid_rad.cos(),
                g.segment_radius * -mid_rad.sin(),
            );
        painter.circle_filled(bind_pos, g.segment_bind_radius, segment_bind_color);
        painter.text(
            bind_pos,
            Align2::CENTER_CENTER,
            group_bind,
            FontId::monospace(segment_bind_font_size),
            segment_bind_font_color,
        );
    }

    pub fn draw_line(
        &self,
        painter: &egui::Painter,
        center: Pos2,
        center_rad: f32,
        app_rad: f32,
        is_selected: bool,
    ) {
        let g = &self.graphic;

        let p0 = center
            + Vec2::new(
                g.app_offset * g.line_point_scale_1 * center_rad.cos(),
                g.app_offset * g.line_point_scale_1 * -center_rad.sin(),
            );

        let p1 = center
            + Vec2::new(
                g.app_offset * g.line_point_scale_2 * center_rad.cos(),
                g.app_offset * g.line_point_scale_2 * -center_rad.sin(),
            );

        let p2 = center
            + Vec2::new(
                g.app_offset * g.line_point_scale_3 * app_rad.cos(),
                g.app_offset * g.line_point_scale_3 * -app_rad.sin(),
            );

        let p3 = center + Vec2::new(g.app_offset * app_rad.cos(), g.app_offset * -app_rad.sin());

        let (line_color, line_width) = if is_selected {
            (Self::get_color32(g.line_color_active), g.line_width_active)
        } else {
            (
                Self::get_color32(g.line_color_unactive),
                g.line_width_unactive,
            )
        };

        painter.line_segment([center, p0], Stroke::new(line_width, line_color));

        painter.add(epaint::CubicBezierShape::from_points_stroke(
            [p0, p1, p2, p3],
            false,
            Color32::TRANSPARENT,
            PathStroke::new(line_width, line_color),
        ));
    }

    /// Creates the background icon decoder on first use. It is lazy so `Hring`
    /// can still be constructed before an egui context is available.
    pub fn ensure_icon_loader(&mut self, ctx: &egui::Context) {
        if self.icon_loader.is_none() {
            self.icon_loader = Some(crate::app::IconLoader::spawn(ctx.clone()));
        }
    }

    /// Uploads icons decoded by the background worker, a few per frame so a
    /// burst of finished images does not stall a single repaint.
    pub fn pump_icon_loader(&mut self, ctx: &egui::Context) {
        let Some(loader) = self.icon_loader.as_mut() else {
            return;
        };

        let mut uploaded = 0;

        while uploaded < ICON_UPLOADS_PER_FRAME {
            let Ok((path, image)) = loader.from_worker.try_recv() else {
                break;
            };

            loader.requested.remove(&path);

            let texture = image.map(|image| {
                ctx.load_texture(
                    format!("hring_icon:{path}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });

            self.icon_textures.insert(path, texture);
            uploaded += 1;
        }

        if uploaded == ICON_UPLOADS_PER_FRAME {
            // More results may be queued: keep repainting so they drain without
            // a long stall in one frame.
            ctx.request_repaint();
        }
    }

    /// Queues the icons the "All Programs" grid needs but does not have yet.
    /// They are decoded in the background; the grid shows placeholders until
    /// each one arrives.
    pub fn request_icon_textures(&mut self, ctx: &egui::Context, paths: Vec<String>) {
        let Some(loader) = self.icon_loader.as_mut() else {
            return;
        };

        let mut queued = false;

        for path in paths {
            if self.icon_textures.contains_key(&path) || !loader.requested.insert(path.clone()) {
                continue;
            }

            if loader.to_worker.send(path).is_err() {
                break;
            }

            queued = true;
        }

        if queued {
            ctx.request_repaint();
        }
    }

    /// Uploads the texture of an icon on first use and remembers the result,
    /// so broken or missing icons are not read from disk on every frame.
    pub fn ensure_icon_texture(&mut self, ctx: &egui::Context, icon_path: &str) {
        if self.icon_textures.contains_key(icon_path) {
            return;
        }

        let texture = icon::load_color_image(Path::new(icon_path)).map(|image| {
            ctx.load_texture(
                format!("hring_icon:{icon_path}"),
                image,
                egui::TextureOptions::LINEAR,
            )
        });

        self.icon_textures.insert(icon_path.to_string(), texture);
    }

    pub fn draw_apps(
        &self,
        painter: &egui::Painter,
        center: Pos2,
        crt_app_rad: f32,
        is_selected: bool,
        app: &App,
    ) {
        let g = &self.graphic;

        let (app_color, app_font_color, app_font_size) = if is_selected {
            (
                Self::get_color32(g.app_color_active),
                Self::get_color32(g.app_font_color_active),
                g.app_font_size_active,
            )
        } else {
            (
                Self::get_color32(g.app_color_unactive),
                Self::get_color32(g.app_font_color_unactive),
                g.app_font_size_unactive,
            )
        };

        let app_pos = center
            + Vec2::new(
                g.app_offset * crt_app_rad.cos(),
                g.app_offset * -crt_app_rad.sin(),
            );

        painter.circle_filled(app_pos, g.app_radius, app_color);

        let texture = app
            .icon
            .as_deref()
            .and_then(|icon_path| self.icon_textures.get(icon_path))
            .and_then(|texture| texture.as_ref());

        if let Some(texture) = texture {
            let icon_rect = egui::Rect::from_center_size(app_pos, Vec2::splat(g.app_radius * 1.5));

            painter.image(
                texture.id(),
                icon_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );

            self.draw_app_bind_badge(
                painter,
                app_pos,
                &app.bind,
                app_color,
                app_font_color,
                app_font_size,
            );
        } else {
            painter.text(
                app_pos,
                Align2::CENTER_CENTER,
                &app.bind,
                FontId::monospace(app_font_size),
                app_font_color,
            );
        }
    }

    /// Draws a small keyboard shortcut badge on the rim of an app circle, so the
    /// bind stays visible once the icon takes over the center of the circle.
    fn draw_app_bind_badge(
        &self,
        painter: &egui::Painter,
        app_pos: Pos2,
        bind_text: &str,
        badge_color: Color32,
        text_color: Color32,
        font_size: f32,
    ) {
        let badge_radius = self.graphic.app_radius * 0.4;
        let badge_offset = self.graphic.app_radius * 0.62;
        let badge_pos = app_pos + Vec2::new(badge_offset, badge_offset);

        painter.circle_filled(badge_pos, badge_radius, badge_color);
        painter.text(
            badge_pos,
            Align2::CENTER_CENTER,
            bind_text,
            FontId::monospace(font_size * 0.8),
            text_color,
        );
    }

    pub fn draw_app_text(
        &self,
        painter: &egui::Painter,
        app_name: &str,
        center: Pos2,
        app_rad: f32,
        is_selected: bool,
    ) {
        let g = &self.graphic;

        let (font_color, background_color, font_size) = if is_selected {
            (
                Self::get_color32(g.app_title_font_color_active),
                Self::get_color32(g.app_title_background_color_active),
                g.app_title_font_size_active,
            )
        } else {
            (
                Self::get_color32(g.app_title_font_color_unactive),
                Self::get_color32(g.app_title_background_color_unactive),
                g.app_title_font_size_unactive,
            )
        };

        let app_pos =
            center + Vec2::new(g.app_offset * app_rad.cos(), g.app_offset * -app_rad.sin());

        let galley = painter.layout_no_wrap(
            app_name.to_string(),
            FontId::monospace(font_size),
            font_color,
        );

        let ray_direction = egui::vec2(app_rad.cos(), -app_rad.sin());

        let (text_angle, mut text_pos) = if app_rad.cos() < 0.0 {
            (
                -app_rad + f32::consts::PI,
                app_pos + ray_direction * (g.app_title_offset + galley.size().x),
            )
        } else {
            (-app_rad, app_pos + ray_direction * g.app_title_offset)
        };

        let text_rotation = Rot2::from_angle(text_angle);
        text_pos += text_rotation * egui::vec2(0.0, -galley.size().y / 2.0);

        let padding = egui::vec2(
            g.app_title_background_paddings.0,
            g.app_title_background_paddings.1,
        );
        let local_rect = egui::Rect::from_min_max(
            egui::pos2(-padding.x, -padding.y),
            egui::pos2(galley.size().x + padding.x, galley.size().y + padding.y),
        );

        let corners = [
            local_rect.left_top(),
            local_rect.right_top(),
            local_rect.right_bottom(),
            local_rect.left_bottom(),
        ];

        let bg_points: Vec<Pos2> = corners
            .iter()
            .map(|cornet| text_pos + text_rotation * cornet.to_vec2())
            .collect();

        painter.add(PathShape::convex_polygon(
            bg_points,
            background_color,
            PathStroke::NONE,
        ));

        painter.add(egui::epaint::TextShape {
            pos: text_pos,
            galley,
            underline: Stroke::NONE,
            fallback_color: font_color,
            override_text_color: None,
            opacity_factor: 1.0,
            angle: text_angle,
        });
    }

    pub fn get_color32(rgba: (u8, u8, u8, u8)) -> Color32 {
        Color32::from_rgba_unmultiplied(rgba.0, rgba.1, rgba.2, rgba.3)
    }

    /// Replaces the alpha channel of `color`, keeping its RGB values.
    pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }

    /// Mixes `color` toward white by `amount` (`0.0` keeps the color, `1.0`
    /// returns white), used for the highlight rim of the active tab.
    pub fn lighten(color: Color32, amount: f32) -> Color32 {
        let mix = |channel: u8| {
            let channel = f32::from(channel);
            (channel + (255.0 - channel) * amount).round() as u8
        };

        Color32::from_rgba_unmultiplied(mix(color.r()), mix(color.g()), mix(color.b()), color.a())
    }
}
