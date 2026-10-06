// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use core::f32;

use egui::{
    self, Align2, FontId, Frame, Key, Response, RichText, ScrollArea, Stroke, StrokeKind, Vec2,
    ViewportCommand,
};

use crate::{
    app::{AssignStage, Hring, PendingAssign, PendingDelete, View},
    config,
    data::{App, AppLink, ConfApp, ConfGroup, Group},
};

fn app_letter_group(name: &str) -> char {
    name.trim_start()
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map(|letter| letter.to_ascii_uppercase())
        .unwrap_or('#')
}

fn sort_app_groups(apps: &mut [AppLink]) {
    apps.sort_by_cached_key(|app| {
        let letter = app_letter_group(&app.name);
        (letter == '#', letter, app.name.to_lowercase())
    });
}

/// Hold the emphasis while the scroll settles, then fade back to normal.
fn group_focus_strength(focus: Option<(char, f64)>, now: f64) -> f32 {
    focus.map_or(0.0, |(_, started)| {
        ((2.0 - (now - started)) / 0.6).clamp(0.0, 1.0) as f32
    })
}

fn pressed_app_letter(ctx: &egui::Context) -> Option<char> {
    ctx.input(|input| {
        input.events.iter().rev().find_map(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                return None;
            };
            if modifiers.ctrl || modifiers.alt || modifiers.command {
                return None;
            }
            let name = key.name();
            (name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic())
                .then(|| name.as_bytes()[0].to_ascii_uppercase() as char)
        })
    })
}

fn draw_app_letter_index(
    ui: &mut egui::Ui,
    apps: &[AppLink],
    focused_letter: Option<char>,
    input_locked: bool,
    font_color: egui::Color32,
    accent: egui::Color32,
) -> Option<char> {
    let mut requested = None;
    let available = ui.max_rect();
    let dock_rect = egui::Rect::from_center_size(
        available.center(),
        Vec2::new(
            available.width(),
            available.height().min(27.0 * 44.0 + 16.0),
        ),
    );
    ui.allocate_rect(dock_rect, egui::Sense::hover());
    ui.painter()
        .rect_filled(dock_rect, 16.0, Hring::with_alpha(accent, 16));
    ui.painter().rect_stroke(
        dock_rect,
        16.0,
        Stroke::new(1.0_f32, Hring::with_alpha(font_color, 35)),
        StrokeKind::Inside,
    );
    let inner = dock_rect.shrink(8.0);
    let base_height = inner.height() / 27.0;
    let pointer = ui.input(|input| input.pointer.hover_pos());
    // Compute proximity from fixed reference positions, then distribute the
    // available height among the enlarged letters. Every letter stays visible.
    let letters: Vec<_> = ('A'..='Z')
        .chain(std::iter::once('#'))
        .enumerate()
        .map(|(index, letter)| {
            let reference = egui::pos2(
                inner.center().x,
                inner.top() + (index as f32 + 0.5) * base_height,
            );
            let proximity = if input_locked {
                0.0
            } else {
                pointer.map_or(0.0, |pointer| {
                    (1.0 - pointer.distance(reference) / 100.0)
                        .clamp(0.0, 1.0)
                        .powi(2)
                })
            };
            let magnification = ui.ctx().animate_value_with_time(
                ui.id().with(("letter_magnification", letter)),
                proximity,
                0.12,
            );
            (letter, magnification)
        })
        .collect();
    let total_weight: f32 = letters.iter().map(|(_, scale)| 1.0 + 1.8 * scale).sum();
    let mut top = inner.top();
    for (letter, magnification) in letters {
        let present = apps.iter().any(|app| app_letter_group(&app.name) == letter);
        let active = focused_letter == Some(letter);
        let enabled = present && !input_locked;
        let height = inner.height() * (1.0 + 1.8 * magnification) / total_weight;
        let rect = egui::Rect::from_min_size(
            egui::pos2(inner.left(), top),
            Vec2::new(inner.width(), height),
        );
        top += height;
        let response = ui.interact(
            rect,
            ui.id().with(("letter_button", letter)),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let hovered = enabled && response.hovered();
        // Paint the complete row height so adjacent targets meet without gaps.
        let button_rect = egui::Rect::from_center_size(
            rect.center(),
            Vec2::new(48.0 + 24.0 * magnification, height),
        );
        let color = if !present {
            Hring::with_alpha(font_color, 65)
        } else if active {
            accent
        } else {
            font_color
        };
        if hovered {
            ui.painter()
                .rect_filled(rect, 0.0, Hring::with_alpha(accent, 95));
        }
        ui.painter().rect_filled(
            button_rect,
            0.0,
            Hring::with_alpha(
                accent,
                if hovered {
                    110
                } else if active {
                    70
                } else {
                    (20.0 + 35.0 * magnification) as u8
                },
            ),
        );
        if active {
            ui.painter().rect_stroke(
                button_rect,
                0.0,
                Stroke::new(1.5_f32, accent),
                StrokeKind::Inside,
            );
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            letter,
            FontId::monospace((22.0 + 14.0 * magnification).min(height * 0.75)),
            color,
        );
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            requested = Some(letter);
        }
    }
    requested
}

/// Each letter starts a fresh row, including after filtering the list.
fn grouped_app_rows(apps: &[AppLink], columns: usize) -> Vec<std::ops::Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0;
    for group in apps.chunk_by(|a, b| app_letter_group(&a.name) == app_letter_group(&b.name)) {
        let end = start + group.len();
        for row_start in (start..end).step_by(columns.max(1)) {
            rows.push(row_start..row_start.saturating_add(columns.max(1)).min(end));
        }
        start = end;
    }
    rows
}

fn vertical_app_neighbor(rows: &[std::ops::Range<usize>], current: usize, down: bool) -> usize {
    let Some(row_index) = rows.iter().position(|row| row.contains(&current)) else {
        return current;
    };
    let next_row = if down {
        rows.get(row_index + 1)
    } else {
        row_index.checked_sub(1).and_then(|index| rows.get(index))
    };
    next_row.map_or(current, |row| {
        row.start + (current - rows[row_index].start).min(row.len() - 1)
    })
}

impl Hring {
    /// Per-frame UI pass. Called by the backend once per `egui::Context::run`.
    pub(crate) fn update_ui(&mut self, ctx: &egui::Context) {
        if !self.was_updated_from_config_loader
            && let Ok(groups) = self.from_config_loader.try_recv()
        {
            self.binds = groups;
            self.was_updated_from_config_loader = true;
        }

        if let Ok(mut apps) = self.from_search_worker.try_recv() {
            // Apply the same order to cached applications and search results.
            sort_app_groups(&mut apps);
            self.apps = apps;
            self.all_apps_group_focus = None;
        }

        // Start the background icon decoder and upload whatever it finished
        // since the last frame.
        self.ensure_icon_loader(ctx);
        self.pump_icon_loader(ctx);

        // A modal prompt swallows the key press and pointer click of this frame,
        // so it must not also launch an app or trigger a normal hotkey.
        let modal_active = self.pending_assign.is_some() || self.pending_delete.is_some();

        if self.pending_assign.is_some() {
            // While a shortcut is being assigned the next key press is the bind,
            // so it must not close the window or trigger a normal hotkey.
            self.handle_pending_assign(ctx);
        } else if self.pending_delete.is_some() {
            // While deleting, Enter confirms and Escape cancels.
            self.handle_pending_delete(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.all_apps_search_active {
                // The first `Escape` only leaves the filter field; a second one
                // (once search mode is off) closes the window.
                self.all_apps_search_active = false;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }

        if let Some(view) = self.create_tab_bar(ctx) {
            self.view = view;
        }

        // `Ctrl+H` / `Ctrl+L` switch pages like browser tabs. Ignored while a
        // modal prompt is up, since it captures raw key presses as binds.
        if !modal_active {
            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::H)) {
                self.view = View::Keyboard;
            } else if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::L)) {
                self.view = View::AllApps;
            }
        }

        // Search mode only makes sense on the "All Programs" page; leaving it
        // must not swallow `Escape` on the keyboard page.
        if self.view != View::AllApps {
            self.all_apps_search_active = false;
        }

        self.draw_pages(ctx, modal_active);

        if let Some(confirmed) = self.show_delete_confirm(ctx) {
            if confirmed {
                self.delete_pending();
            } else {
                self.pending_delete = None;
            }
        }

        self.show_assign_overlay(ctx);
    }
}

impl Hring {
    /// Top bar holding the page tabs. Returns the view selected this frame, if
    /// the user clicked a tab.
    fn create_tab_bar(&self, ctx: &egui::Context) -> Option<View> {
        let active_color = Self::get_color32(self.graphic.app_color_active);
        let active_text_color = Self::get_color32(self.graphic.app_font_color_active);
        let inactive_text_color = Self::get_color32(self.graphic.menu_items_font_color);
        let hover_color = Self::get_color32(self.graphic.menu_items_hover_color);
        let font_size = self.graphic.menu_items_font_size.max(15.0);

        let items = [
            (View::Keyboard, "Keyboard"),
            (View::AllApps, "All Programs"),
        ];

        let segment_width = 180.0;
        let height = 46.0;
        let inset = 5.0;
        let width = segment_width * items.len() as f32;

        let active_index = match self.view {
            View::Keyboard => 0,
            View::AllApps => 1,
        };

        // The pill glides towards the active segment instead of snapping, so a
        // page switch reads as a slide. The configured easing curve shapes it,
        // and egui keeps repainting until the animation settles.
        let animated_index = ctx.animate_bool_with_time_and_easing(
            egui::Id::new("tab_bar_pill"),
            self.view == View::AllApps,
            0.22,
            self.graphic.animation_easing.function(),
        );

        let mut requested_view = None;

        egui::TopBottomPanel::top("tab_bar")
            // Same fill as the pages below, so the tab strip does not read as a
            // darker band above the central panel.
            .frame(Frame::NONE.fill(Self::get_color32(self.graphic.main_panel_color)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                ui.vertical_centered(|ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
                    let response =
                        response.on_hover_text("Ctrl+H — Keyboard    Ctrl+L — All Programs");

                    let hovered_index = response.hover_pos().map(|pos| {
                        ((pos.x - rect.left()) / segment_width)
                            .floor()
                            .clamp(0.0, items.len() as f32 - 1.0) as usize
                    });

                    let painter = ui.painter();

                    // Subtle highlight on the segment the pointer is over.
                    if let Some(index) = hovered_index
                        && index != active_index
                    {
                        let segment_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.left() + segment_width * index as f32, rect.top()),
                            Vec2::new(segment_width, height),
                        );
                        painter.rect_filled(segment_rect, height / 2.0, hover_color);
                    }

                    // Sliding pill under the selected segment, with a soft glow.
                    let pill_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            rect.left() + segment_width * animated_index + inset,
                            rect.top() + inset,
                        ),
                        Vec2::new(segment_width - inset * 2.0, height - inset * 2.0),
                    );
                    let pill_radius = (height - inset * 2.0) / 2.0;
                    painter.rect_filled(
                        pill_rect.expand(3.0),
                        pill_radius + 3.0,
                        Self::with_alpha(active_color, 55),
                    );
                    painter.rect_filled(pill_rect, pill_radius, active_color);
                    painter.rect_stroke(
                        pill_rect,
                        pill_radius,
                        egui::Stroke::new(1.5, Self::lighten(active_color, 0.45)),
                        egui::StrokeKind::Inside,
                    );

                    // Labels, drawn last so they sit on top of the pill.
                    for (index, (_view, label)) in items.iter().enumerate() {
                        let color = if index == active_index {
                            active_text_color
                        } else {
                            inactive_text_color
                        };

                        painter.text(
                            egui::pos2(
                                rect.left() + segment_width * (index as f32 + 0.5),
                                rect.center().y,
                            ),
                            Align2::CENTER_CENTER,
                            *label,
                            FontId::proportional(font_size),
                            color,
                        );
                    }

                    if let Some(index) = hovered_index {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                        if response.clicked() {
                            requested_view = Some(items[index].0);
                        }
                    }
                });
            });

        requested_view
    }

    /// Draws the active page and slides the two pages horizontally past each
    /// other while the view changes.
    fn draw_pages(&mut self, ctx: &egui::Context, modal_active: bool) {
        let target = match self.view {
            View::Keyboard => 0.0,
            View::AllApps => 1.0,
        };

        // Lags behind `target`, so the pages move instead of snapping. The
        // configured easing curve shapes the motion.
        let anim = ctx.animate_bool_with_time_and_easing(
            egui::Id::new("page_slide"),
            self.view == View::AllApps,
            0.28,
            self.graphic.animation_easing.function(),
        );
        let settled = (anim - target).abs() < 0.001;

        // Hotkeys are only handled once the pages have settled, so a key press
        // cannot land on a page that is still sliding.
        if !modal_active && settled && self.view == View::Keyboard {
            self.handle_hotkeys(ctx);
        }

        let mut all_apps_text_edit = None;

        // While sliding, the pages are drawn at an offset, so pointer input would
        // be hit-tested at the wrong place. Lock it until the pages settle.
        let page_input_locked = modal_active || !settled;

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Self::get_color32(self.graphic.main_panel_color)))
            .show(ctx, |ui| {
                let viewport = ui.max_rect();

                for (index, view) in [View::Keyboard, View::AllApps].into_iter().enumerate() {
                    let offset = viewport.width() * (index as f32 - anim);

                    // Skip a page once it has left the viewport completely.
                    if offset <= -viewport.width() || offset >= viewport.width() {
                        continue;
                    }

                    let page_rect = viewport.translate(Vec2::new(offset, 0.0));

                    ui.scope_builder(egui::UiBuilder::new().max_rect(page_rect), |ui| {
                        ui.set_clip_rect(viewport);

                        ui.push_id(index, |ui| match view {
                            View::Keyboard => self.draw_keyboard_page(ui, ctx, page_input_locked),
                            View::AllApps => {
                                all_apps_text_edit =
                                    Some(self.draw_all_apps_page(ui, ctx, page_input_locked));
                            }
                        });
                    });
                }
            });

        if !modal_active && settled && self.view == View::AllApps {
            if let Some(text_edit) = &all_apps_text_edit {
                self.handle_search(text_edit, ctx);
            }

            self.handle_all_apps_nav(ctx);
        }
    }

    /// Full-page grid of every installed application, with search. Draws into
    /// the given page `ui`, so it can slide together with the tab bar.
    fn draw_all_apps_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        input_locked: bool,
    ) -> Response {
        let font_color = Self::get_color32(self.graphic.menu_items_font_color);
        let hover_color = Self::get_color32(self.graphic.menu_items_hover_color);
        let placeholder_color = Self::get_color32(self.graphic.app_color_unactive);
        let selection_color = Self::get_color32(self.graphic.app_color_active);
        // Text drawn inside the inverted block cursor uses the panel colour, the
        // way NeoVim swaps foreground and background under the cursor. Forced
        // opaque: the panel colour itself carries alpha.
        let cursor_text_color = {
            let (r, g, b, _) = self.graphic.main_panel_color;
            egui::Color32::from_rgb(r, g, b)
        };
        // Snapshot the grid tuning now, so the draw closure only captures these
        // plain values instead of borrowing `self.graphic`.
        let ap = self.graphic.all_programs.clone();
        let font_size = (self.graphic.menu_items_font_size * ap.font_scale).max(ap.font_size_min);
        let icon_size = (self.graphic.app_radius * ap.icon_radius_scale)
            .clamp(ap.icon_size_min, ap.icon_size_max);

        // egui's default wheel step (`40.0`) feels sluggish on a full-screen
        // grid, so it is raised here. The value is configurable, so the feel
        // can be retuned without touching the code.
        ctx.options_mut(|options| {
            options.input_options.line_scroll_speed = ap.scroll_speed;
        });

        // Keep the keyboard selection inside the (possibly filtered) list. A
        // valid index survives filtering, so the cursor does not jump while
        // typing; a fresh grid starts on the first card.
        if self.apps.is_empty() {
            self.selected_app = None;
        } else {
            self.selected_app = Some(self.selected_app.unwrap_or(0).min(self.apps.len() - 1));
        }

        // Navigation state produced by the key handler on the previous frame.
        let mut selected_app = self.selected_app;
        let scroll_to_selected = std::mem::take(&mut self.all_apps_scroll_to_selected);
        let pending_scroll = std::mem::take(&mut self.all_apps_scroll_delta);
        // Geometry is measured while drawing and stored back afterwards, so the
        // next frame's key handler can move by rows and scroll by pages.
        let mut measured_columns = self.all_apps_columns.max(1);
        let mut measured_scroll_step = self.all_apps_scroll_step;
        let search_active = self.all_apps_search_active;
        let now = ctx.input(|input| input.time);
        let mut group_focus = self
            .all_apps_group_focus
            .filter(|_| group_focus_strength(self.all_apps_group_focus, now) > 0.0);
        let mut jump_to_letter = None;
        if !input_locked
            && !search_active
            && let Some(letter) = pressed_app_letter(ctx)
            && let Some(index) = self
                .apps
                .iter()
                .position(|app| app_letter_group(&app.name) == letter)
        {
            jump_to_letter = Some(letter);
            group_focus = Some((letter, now));
            selected_app = Some(index);
        }

        let mut app_to_execute = None;
        let mut assignment_request: Option<AppLink> = None;

        // Queue every icon the grid needs. They are decoded on a background
        // thread and uploaded a few per frame, so switching to this page is
        // instant: each cell shows a placeholder until its icon arrives.
        let icon_paths: Vec<String> = self
            .apps
            .iter()
            .filter_map(|app| app.icon.clone())
            .collect();
        self.request_icon_textures(ctx, icon_paths);

        let apps = &self.apps;
        let textures = &self.icon_textures;

        let page_rect = ui.max_rect();

        let text_edit = ui
            .scope_builder(
                egui::UiBuilder::new().max_rect(page_rect.shrink(24.0)),
                |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("All Programs")
                                .font(egui::FontId::monospace(20.0))
                                .color(Self::with_alpha(font_color, 210))
                                .strong(),
                        );
                        ui.add_space(10.0);

                        // NeoVim-style command bar: monospace prompt, large
                        // type, and a border that lights up while search mode is
                        // active. The stroke width is kept constant, because
                        // egui counts it as part of the frame margin: changing
                        // it would resize the bar and shove the grid down.
                        let search_font = egui::FontId::monospace(ap.search_font_size);
                        let prompt_color = if search_active {
                            selection_color
                        } else {
                            Self::with_alpha(font_color, 140)
                        };
                        let border_width = ap.search_border_width.clamp(0.0, 8.0);
                        let bar_stroke = Stroke::new(
                            border_width,
                            if search_active {
                                selection_color
                            } else {
                                Self::with_alpha(font_color, 45)
                            },
                        );
                        let bar_fill = Self::with_alpha(
                            hover_color,
                            if search_active {
                                ap.search_active_alpha
                            } else {
                                ap.search_idle_alpha
                            },
                        );

                        let search_bar = egui::Frame::NONE
                            .fill(bar_fill)
                            .stroke(bar_stroke)
                            .corner_radius(ap.search_corner_radius)
                            .inner_margin(egui::Margin::symmetric(
                                ap.search_padding_x.clamp(0.0, 127.0) as i8,
                                0,
                            ))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());

                                // A fixed-height row is allocated first, so the
                                // prompt and the field are centered vertically
                                // against the full bar instead of being
                                // top-aligned by the first widget added.
                                ui.allocate_ui_with_layout(
                                    Vec2::new(ui.available_width(), ap.search_bar_height),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 12.0;

                                        ui.label(
                                            RichText::new(ap.search_prompt.as_str())
                                                .font(search_font.clone())
                                                .color(prompt_color)
                                                .strong(),
                                        );

                                        // egui's thin caret is hidden; a
                                        // NeoVim-style block is painted instead.
                                        ui.visuals_mut().text_cursor.stroke = Stroke::NONE;
                                        ui.visuals_mut().text_cursor.blink = false;

                                        let v_margin = ((ap.search_bar_height
                                            - ap.search_font_size * ap.text_line_height)
                                            / 2.0)
                                            .clamp(0.0, 127.0)
                                            .round() as i8;

                                        let output = egui::TextEdit::singleline(
                                            &mut self.search_text,
                                        )
                                        .font(search_font.clone())
                                        .text_color(font_color)
                                        .hint_text(
                                            RichText::new("Search applications...")
                                                .color(Self::with_alpha(font_color, 90)),
                                        )
                                        .hint_text_font(search_font.clone())
                                        .frame(false)
                                        .margin(egui::Margin::symmetric(0, v_margin))
                                        .vertical_align(egui::Align::Center)
                                        .desired_width(ui.available_width())
                                        .show(ui);

                                        if output.response.has_focus()
                                            && let Some(range) = output.cursor_range
                                        {
                                            let local = egui::text_selection::text_cursor_state::cursor_rect(
                                                &output.galley,
                                                &range.primary,
                                                ap.search_font_size,
                                            );
                                            let caret =
                                                local.translate(output.galley_pos.to_vec2());

                                            let under =
                                                self.search_text.chars().nth(range.primary.index);
                                            let painter = ui
                                                .painter()
                                                .with_clip_rect(output.text_clip_rect);

                                            // The block covers the character to
                                            // the right of the caret, exactly
                                            // matching a monospace cell.
                                            let (block_width, under) = match under {
                                                Some(ch) => (
                                                    ui.fonts_mut(|f| {
                                                        f.glyph_width(&search_font, ch)
                                                    })
                                                    .max(4.0),
                                                    Some(ch),
                                                ),
                                                None => (ap.search_font_size * 0.62, None),
                                            };

                                            let block = egui::Rect::from_min_size(
                                                egui::pos2(caret.left(), caret.top()),
                                                Vec2::new(block_width, caret.height()),
                                            );
                                            painter.rect_filled(block, 0.0, selection_color);

                                            if let Some(ch) = under {
                                                painter.text(
                                                    block.center(),
                                                    Align2::CENTER_CENTER,
                                                    ch,
                                                    search_font.clone(),
                                                    cursor_text_color,
                                                );
                                            }
                                        }

                                        output.response
                                    },
                                )
                                .inner
                            });

                        // Search mode (`/`) keeps the filter field focused; it
                        // is re-requested every frame, so a lost focus can never
                        // leave the page unable to filter again. Leaving search
                        // mode (`Escape`) hands the keyboard back to navigation.
                        let text_edit = search_bar.inner;
                        if search_active {
                            if !text_edit.has_focus() {
                                text_edit.request_focus();
                            }
                        } else if text_edit.has_focus() {
                            text_edit.surrender_focus();
                        }

                        ui.add_space(10.0);

                        let body_rect = ui.available_rect_before_wrap();
                        let index_rect = egui::Rect::from_min_max(
                            body_rect.left_top(),
                            egui::pos2(body_rect.left() + 88.0, body_rect.bottom()),
                        );
                        let grid_rect = egui::Rect::from_min_max(
                            egui::pos2(index_rect.right() + 16.0, body_rect.top()),
                            body_rect.right_bottom(),
                        );
                        let requested_letter = ui.scope_builder(
                            egui::UiBuilder::new().id_salt("all_apps_letter_sidebar").max_rect(index_rect),
                            |ui| draw_app_letter_index(
                            ui,
                            apps,
                            group_focus.map(|(letter, _)| letter),
                            input_locked,
                            font_color,
                            selection_color,
                        )).inner;
                        if let Some(letter) = requested_letter {
                            jump_to_letter = Some(letter);
                            group_focus = Some((letter, now));
                            selected_app = apps.iter().position(|app| app_letter_group(&app.name) == letter);
                        }

                        // Named so the scroll offset can be driven by the
                        // keyboard (`PageDown`/`PageUp`) as well as the wheel.
                        let scroll_area = ScrollArea::vertical()
                            .id_salt("all_apps_scroll")
                            .auto_shrink([false, false]);

                        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid_rect));
                        scroll_area.show(&mut grid_ui, |ui| {
                            // `PageDown`/`PageUp` move the content; a
                            // negative delta scrolls the view down.
                            if pending_scroll != 0.0 {
                                ui.scroll_with_delta(Vec2::new(0.0, -pending_scroll));
                            }

                            // Cards are spaced apart generously; the gutter
                            // is the only separation between them.
                            let gap = ap.gap;
                            let padding_x = ap.padding_x;
                            let padding_top = ap.padding_top;
                            let padding_bottom = ap.padding_bottom;
                            let icon_text_gap = ap.icon_text_gap;
                            ui.spacing_mut().item_spacing = Vec2::new(gap, gap);

                            // Keep the grid away from the window edges and
                            // cap how wide it grows, then center the block.
                            let full_width = ui.available_width();
                            let content_width = full_width.min(ap.max_content_width);
                            let side_margin = ((full_width - content_width) / 2.0 - gap).max(0.0);

                            // Fit as many columns as the capped width allows,
                            // then stretch them so a full row fills it.
                            let available = content_width;
                            let min_cell_width =
                                (icon_size + padding_x * 2.0 + ap.cell_width_extra)
                                    .max(ap.min_cell_width);
                            let columns = ((available + gap) / (min_cell_width + gap))
                                .floor()
                                .max(1.0) as usize;
                            let cell_width =
                                (available - gap * (columns as f32 - 1.0)) / columns as f32;
                            let cell_height = padding_top
                                + icon_size
                                + icon_text_gap
                                + font_size * ap.text_lines
                                + padding_bottom;
                            // Lines of name text that fit between the icon
                            // and the bottom of the card before it would
                            // spill into the row below.
                            let text_max_rows = ((cell_height
                                - (padding_top + icon_size + icon_text_gap + padding_bottom))
                                / (font_size * ap.text_line_height))
                                .floor()
                                .max(1.0) as usize;

                            measured_columns = columns;
                            measured_scroll_step = (cell_height * 3.0).max(60.0);

                            // Screen rect of the selected card, captured
                            // while drawing so it can be scrolled into view.
                            let mut selected_rect: Option<egui::Rect> = None;

                            if apps.is_empty() {
                                ui.label(
                                    RichText::new("No applications found")
                                        .color(font_color)
                                        .size(font_size),
                                );
                            }

                            let rows = grouped_app_rows(apps, columns);
                            // Enough leading space to center the first group too.
                            if let Some(first) = apps.first() {
                                let letter = app_letter_group(&first.name);
                                let first_rows = rows.iter().take_while(|row| app_letter_group(&apps[row.start].name) == letter).count();
                                let group_height = font_size.max(24.0) + 14.0
                                    + first_rows as f32 * (cell_height + gap);
                                ui.add_space(((ui.clip_rect().height() - group_height) / 2.0).max(0.0));
                            }
                            let mut previous_letter = None;
                            let mut group_heading_rect = None;
                            let mut jump_rect = None;
                            for row in rows {
                                let letter = app_letter_group(&apps[row.start].name);
                                let strength = group_focus_strength(group_focus, now);
                                let focused = group_focus.is_some_and(|(active, _)| active == letter);
                                let opacity = if focused { 1.0 } else { 1.0 - strength * 0.5 };
                                if previous_letter != Some(letter) {
                                    if previous_letter.is_some() {
                                        ui.add_space(gap.max(16.0));
                                    }
                                    ui.horizontal(|ui| {
                                        ui.multiply_opacity(opacity);
                                        if side_margin > 0.0 {
                                            ui.add_space(side_margin);
                                        }
                                        let heading_size = font_size.max(24.0);
                                        let (rect, response) = ui.allocate_exact_size(
                                            Vec2::new(content_width, heading_size + 14.0),
                                            egui::Sense::click(),
                                        );
                                        group_heading_rect = Some(rect);
                                        let heading_color = if focused {
                                            font_color.lerp_to_gamma(selection_color, strength)
                                        } else {
                                            font_color
                                        };
                                        ui.painter().text(
                                            rect.left_top(),
                                            Align2::LEFT_TOP,
                                            letter,
                                            FontId::monospace(heading_size),
                                            heading_color,
                                        );
                                        ui.painter().line_segment(
                                            [rect.left_bottom(), rect.right_bottom()],
                                            Stroke::new(1.0_f32, Self::with_alpha(heading_color, 90)),
                                        );
                                        if !input_locked {
                                            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                                            if response.clicked() {
                                                jump_to_letter = Some(letter);
                                                group_focus = Some((letter, now));
                                                selected_app = Some(row.start);
                                            }
                                        }
                                    });
                                    previous_letter = Some(letter);
                                }
                                ui.horizontal(|ui| {
                                    ui.multiply_opacity(opacity);
                                    if side_margin > 0.0 {
                                        ui.add_space(side_margin);
                                    }
                                    for index in row {
                                        let app = &apps[index];
                                        let is_selected = selected_app == Some(index);

                                        let (rect, response) = ui.allocate_exact_size(
                                            Vec2::new(cell_width, cell_height),
                                            egui::Sense::click(),
                                        );
                                        let response = response.on_hover_text(app.name.as_str());

                                        let painter = ui.painter();
                                        let hovered = response.hovered();

                                        painter.rect_filled(
                                            rect,
                                            ap.corner_radius,
                                            Self::with_alpha(
                                                hover_color,
                                                if hovered || is_selected {
                                                    ap.hover_alpha
                                                } else if focused {
                                                    egui::lerp(
                                                        ap.idle_alpha as f32..=ap.hover_alpha.max(ap.idle_alpha) as f32,
                                                        strength * 0.45,
                                                    ) as u8
                                                } else {
                                                    ap.idle_alpha
                                                },
                                            ),
                                        );

                                        // Keyboard selection ring.
                                        if is_selected {
                                            painter.rect_stroke(
                                                rect,
                                                ap.corner_radius,
                                                Stroke::new(2.0, selection_color),
                                                StrokeKind::Inside,
                                            );
                                            selected_rect = Some(rect);
                                        }

                                        if hovered {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }

                                        let icon_center = egui::pos2(
                                            rect.center().x,
                                            rect.top() + padding_top + icon_size / 2.0,
                                        );
                                        let icon_rect = egui::Rect::from_center_size(
                                            icon_center,
                                            Vec2::splat(icon_size),
                                        );

                                        let texture = app
                                            .icon
                                            .as_deref()
                                            .and_then(|path| textures.get(path))
                                            .and_then(|texture| texture.as_ref());

                                        if let Some(texture) = texture {
                                            painter.image(
                                                texture.id(),
                                                icon_rect,
                                                egui::Rect::from_min_max(
                                                    egui::pos2(0.0, 0.0),
                                                    egui::pos2(1.0, 1.0),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                        } else {
                                            // No icon resolved: fall back to
                                            // the app's initial on a chip.
                                            painter.rect_filled(
                                                icon_rect,
                                                ap.corner_radius,
                                                placeholder_color,
                                            );
                                            let initial = app
                                                .name
                                                .chars()
                                                .next()
                                                .map(|c| c.to_uppercase().to_string())
                                                .unwrap_or_default();
                                            painter.text(
                                                icon_center,
                                                Align2::CENTER_CENTER,
                                                initial,
                                                FontId::proportional(icon_size * 0.55),
                                                font_color,
                                            );
                                        }

                                        // Clamp the name to the card: wrap it
                                        // over a few lines and ellipsize the
                                        // rest, so a long title cannot spill
                                        // over the icon of the next row.
                                        let mut job = egui::text::LayoutJob::single_section(
                                            app.name.clone(),
                                            egui::text::TextFormat {
                                                font_id: FontId::proportional(font_size),
                                                color: font_color,
                                                ..Default::default()
                                            },
                                        );
                                        job.wrap = egui::text::TextWrapping {
                                            max_width: cell_width - padding_x * 2.0,
                                            max_rows: text_max_rows,
                                            ..Default::default()
                                        };
                                        let galley = painter.layout_job(job);
                                        // epaint ships no bold weight, so the
                                        // name is thickened by overdrawing it
                                        // with a tiny offset.
                                        let text_pos = egui::pos2(
                                            rect.center().x - galley.size().x / 2.0,
                                            icon_rect.bottom() + icon_text_gap,
                                        );
                                        let text_painter = painter.with_clip_rect(rect);
                                        text_painter.galley(text_pos, galley.clone(), font_color);
                                        text_painter.galley(
                                            text_pos + Vec2::new(ap.font_bold_offset, 0.0),
                                            galley.clone(),
                                            font_color,
                                        );
                                        text_painter.galley(
                                            text_pos + Vec2::new(0.0, ap.font_bold_offset),
                                            galley,
                                            font_color,
                                        );

                                        if response.clicked() && !input_locked {
                                            app_to_execute = Some(app.exec.clone());
                                        }

                                        if response.secondary_clicked() && !input_locked {
                                            assignment_request = Some(app.clone());
                                        }
                                    }
                                });
                                if jump_to_letter == Some(letter) && let Some(heading) = group_heading_rect {
                                    jump_rect = Some(egui::Rect::from_min_max(
                                        heading.min,
                                        egui::pos2(heading.right(), ui.min_rect().bottom()),
                                    ));
                                }
                            }

                            // Trailing space lets the last group reach the center.
                            if let Some(heading) = group_heading_rect {
                                let group_height = ui.min_rect().bottom() - heading.top();
                                ui.add_space(((ui.clip_rect().height() - group_height) / 2.0 - gap).max(0.0));
                            }
                            if let Some(rect) = jump_rect {
                                ui.scroll_to_rect_animation(
                                    rect,
                                    Some(egui::Align::Center),
                                    egui::style::ScrollAnimation::duration(0.35),
                                );
                            }

                            // Arrow keys moved the cursor: pull the
                            // selected card back into view if it scrolled off.
                            if jump_to_letter.is_none() && scroll_to_selected && let Some(rect) = selected_rect {
                                ui.scroll_to_rect(rect, None);
                            }
                        });

                        text_edit
                    })
                    .inner
                },
            )
            .inner;

        if let Some(app) = assignment_request {
            self.pending_assign = Some(PendingAssign::new(app));
        }

        if let Some(exec_path) = app_to_execute {
            Self::exec_app(ctx, &exec_path);
        }

        // Remembered for the next frame's `handle_all_apps_nav`.
        self.all_apps_columns = measured_columns;
        self.all_apps_scroll_step = measured_scroll_step;
        self.selected_app = selected_app;
        self.all_apps_group_focus = group_focus;
        if group_focus.is_some() {
            ctx.request_repaint();
        }

        text_edit
    }

    /// Captures the next pressed key: the first one selects an existing group
    /// (or creates a new one), the second becomes the app's launch key.
    /// `Escape` cancels at any point.
    fn handle_pending_assign(&mut self, ctx: &egui::Context) {
        let pressed_key = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some(*key),
                _ => None,
            })
        });

        let Some(key) = pressed_key else {
            return;
        };

        if key == Key::Escape {
            self.pending_assign = None;
            return;
        }

        let Some(pending) = self.pending_assign.as_ref() else {
            return;
        };

        let stage = pending.stage;
        let app = pending.app.clone();
        let group_index = pending.group_index;
        let new_group_bind = pending.new_group_bind.clone();

        let bind = key.name().to_ascii_lowercase();

        match stage {
            AssignStage::GroupKey => {
                // Reuse an existing group with the same bind, otherwise create a
                // new group once the app key arrives.
                if let Some(index) = self
                    .binds
                    .iter()
                    .position(|group| group.bind.eq_ignore_ascii_case(&bind))
                {
                    if let Some(pending) = self.pending_assign.as_mut() {
                        pending.group_index = Some(index);
                        pending.new_group_bind = None;
                        pending.stage = AssignStage::AppKey;
                        pending.warning = None;
                    }
                } else if let Some(owner) = Self::app_using_bind(&self.binds, &bind, None) {
                    self.warn_assign(format!("Key [{bind}] already launches \"{owner}\""));
                } else if let Some(pending) = self.pending_assign.as_mut() {
                    pending.new_group_bind = Some(bind);
                    pending.group_index = None;
                    pending.stage = AssignStage::AppKey;
                    pending.warning = None;
                }
            }
            AssignStage::AppKey => {
                // Another group's key must stay unique: pressing it would switch
                // to that group instead of launching the app. The app's own
                // group is exempt, so a group can reuse its key as an app key.
                if let Some(group_bind) =
                    Self::group_using_bind_excluding(&self.binds, &bind, group_index)
                {
                    self.warn_assign(format!(
                        "Key [{bind}] is already the group key [{group_bind}]"
                    ));
                } else if let Some(owner) = group_index
                    .and_then(|index| self.binds.get(index))
                    .and_then(|group| Self::app_using_bind_in_group(group, &bind, Some(&app.name)))
                {
                    self.warn_assign(format!(
                        "Key [{bind}] already launches \"{owner}\" in this group"
                    ));
                } else {
                    let index = match group_index {
                        Some(index) => index,
                        None => {
                            let group_bind = new_group_bind
                                .expect("New group bind must be captured before the app bind!");

                            self.binds.push(Group {
                                bind: group_bind,
                                apps: Vec::new(),
                            });

                            self.binds.len() - 1
                        }
                    };

                    // The app lives in exactly one group: drop any previous
                    // entry, so a rebind that switches groups moves it instead
                    // of duplicating it.
                    for group in self.binds.iter_mut() {
                        group.apps.retain(|a| a.name != app.name);
                    }

                    if let Some(group) = self.binds.get_mut(index) {
                        // Any app that already uses the freshly captured key is
                        // replaced.
                        group.apps.retain(|a| !a.bind.eq_ignore_ascii_case(&bind));
                        group.apps.push(App {
                            bind,
                            name: app.name,
                            exec: app.exec,
                            icon: app.icon,
                        });
                    }

                    self.pending_assign = None;
                    self.persist_binds();
                }
            }
        }
    }

    /// Returns the bind of the first group that uses `bind`, ignoring the group
    /// at `exclude_index`.
    ///
    /// Used while capturing an app key: an app may reuse its own group's key
    /// (the group is already selected then, so a second press launches), but not
    /// the key of a different group, which would be shadowed by group selection.
    fn group_using_bind_excluding(
        binds: &[Group],
        bind: &str,
        exclude_index: Option<usize>,
    ) -> Option<String> {
        binds
            .iter()
            .enumerate()
            .find(|(index, group)| {
                Some(*index) != exclude_index && group.bind.eq_ignore_ascii_case(bind)
            })
            .map(|(_, group)| group.bind.clone())
    }

    /// Returns the name of the first app inside `group` that uses `bind`.
    ///
    /// App keys only have to be unique within their own group: the same letter
    /// may launch different apps in different groups.
    fn app_using_bind_in_group(
        group: &Group,
        bind: &str,
        exclude_name: Option<&str>,
    ) -> Option<String> {
        group
            .apps
            .iter()
            .find(|app| {
                exclude_name != Some(app.name.as_str()) && app.bind.eq_ignore_ascii_case(bind)
            })
            .map(|app| app.name.clone())
    }

    /// Returns the name of the first app in any group that uses `bind`.
    ///
    /// Group keys are global, so creating a group whose key is already used as
    /// an app key would shadow that launch. That check spans every group.
    fn app_using_bind(binds: &[Group], bind: &str, exclude_name: Option<&str>) -> Option<String> {
        binds
            .iter()
            .flat_map(|group| group.apps.iter())
            .find(|app| {
                exclude_name != Some(app.name.as_str()) && app.bind.eq_ignore_ascii_case(bind)
            })
            .map(|app| app.name.clone())
    }

    fn warn_assign(&mut self, message: String) {
        if let Some(pending) = self.pending_assign.as_mut() {
            pending.warning = Some(message);
        }
    }

    /// `Enter` confirms the pending deletion, `Escape` cancels it.
    fn handle_pending_delete(&mut self, ctx: &egui::Context) {
        if self.pending_delete.is_none() {
            return;
        }

        let (enter_pressed, escape_pressed) =
            ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));

        if escape_pressed {
            self.pending_delete = None;
        } else if enter_pressed {
            self.delete_pending();
        }
    }

    /// Removes the app entry that is currently pending deletion and persists.
    fn delete_pending(&mut self) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };

        if let Some(group) = self.binds.get_mut(pending.group_index)
            && pending.app_index < group.apps.len()
        {
            group.apps.remove(pending.app_index);
        }

        self.persist_binds();
    }

    /// Draws the deletion confirmation box. Returns `Some(true)` when the user
    /// confirmed, `Some(false)` when cancelled and `None` while waiting.
    fn show_delete_confirm(&self, ctx: &egui::Context) -> Option<bool> {
        let Some(pending) = &self.pending_delete else {
            return None;
        };

        let app_name = self
            .binds
            .get(pending.group_index)
            .and_then(|group| group.apps.get(pending.app_index))
            .map(|app| app.name.clone())
            .unwrap_or_default();

        let group_bind = self
            .binds
            .get(pending.group_index)
            .map(|group| group.bind.clone())
            .unwrap_or_default();

        let mut result = None;

        egui::Window::new("Delete shortcut")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("Delete \"{app_name}\" from group [{group_bind}]?"))
                            .size(15.0),
                    );
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            result = Some(true);
                        }

                        if ui.button("Cancel").clicked() {
                            result = Some(false);
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(RichText::new("Enter — delete, Esc — cancel").weak());
                });
            });

        result
    }

    /// Converts the in-memory groups back into the configuration layout and
    /// writes both `binds.toml` and its cache.
    ///
    /// Groups that no longer hold any application are dropped, since groups are
    /// only useful while they launch something.
    fn persist_binds(&mut self) {
        let before = self.binds.len();
        self.binds.retain(|group| !group.apps.is_empty());

        // Removing a group can invalidate the current selection.
        if self.binds.len() != before {
            self.selected_group = None;
        }

        let conf_groups: Vec<ConfGroup> = self
            .binds
            .iter()
            .map(|group| ConfGroup {
                bind: group.bind.clone(),
                apps: group
                    .apps
                    .iter()
                    .map(|app| ConfApp {
                        bind: app.bind.clone(),
                        name: app.name.clone(),
                    })
                    .collect(),
            })
            .collect();

        config::save_binds_config(conf_groups);
        config::create_new_cache_for_groups(&self.binds);
    }

    /// Small centered box shown while waiting for the two shortcut keys.
    fn show_assign_overlay(&self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_assign else {
            return;
        };

        let (title, detail) = match pending.stage {
            AssignStage::GroupKey => (
                "1. Press the GROUP key",
                String::from("An existing group opens, a new one is created."),
            ),
            AssignStage::AppKey => {
                let group = match pending.group_index {
                    Some(index) => self
                        .binds
                        .get(index)
                        .map(|group| format!("Group [{}]", group.bind))
                        .unwrap_or_else(|| String::from("Group [?]")),
                    None => format!(
                        "New group [{}]",
                        pending.new_group_bind.as_deref().unwrap_or("?")
                    ),
                };

                ("Press the APP key", group)
            }
        };

        egui::Window::new("Assign shortcut")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(&pending.app.name).strong().size(18.0));
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).size(16.0));
                    ui.label(RichText::new(detail).weak());

                    if let Some(warning) = &pending.warning {
                        ui.add_space(6.0);
                        ui.colored_label(egui::Color32::LIGHT_RED, warning);
                    }

                    ui.add_space(6.0);
                    ui.label(RichText::new("Esc — cancel").weak());
                });
            });
    }

    /// Handles typing in the search box of the "All Programs" page.
    ///
    /// `Enter` launches the selected app, whether or not the field has focus,
    /// which keeps the vim-style flow (`/` to filter, `Enter` to run).
    fn handle_search(&mut self, text_edit: &Response, ctx: &egui::Context) {
        if text_edit.changed() {
            // The list changes under the cursor; start again from the top.
            self.selected_app = Some(0);
            self.all_apps_group_focus = None;
            self.all_apps_scroll_to_selected = true;
            self.to_search_worker
                .send(self.search_text.clone())
                .expect("SearchThread not reachable!");
        }

        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.launch_selected(ctx);
        }
    }

    /// Launches the app the "All Programs" grid has selected.
    fn launch_selected(&mut self, ctx: &egui::Context) {
        let Some(index) = self
            .selected_app
            .or(if self.apps.is_empty() { None } else { Some(0) })
        else {
            return;
        };

        if let Some(app) = self.apps.get(index) {
            Self::exec_app(ctx, &app.exec);
        }
    }

    /// Keyboard navigation for the "All Programs" grid:
    /// arrows move the selection, `PageDown`/`PageUp` scroll the grid,
    /// `/` focuses the filter field and `Enter` launches the selected app.
    ///
    /// Letter keys jump to their group during drawing. Search mode keeps
    /// those letters in the filter field instead.
    fn handle_all_apps_nav(&mut self, ctx: &egui::Context) {
        if self.all_apps_search_active || self.apps.is_empty() {
            return;
        }

        // Binds are bare keys: a modified press is a page shortcut, not a move.
        // `Shift` is the exception, since Shift+Up/Down also scroll.
        let modifiers = ctx.input(|i| i.modifiers);
        if modifiers.ctrl || modifiers.alt || modifiers.command {
            return;
        }

        let (down, up, left, right, slash, page_down, page_up) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::Slash),
                i.key_pressed(Key::PageDown),
                i.key_pressed(Key::PageUp),
            )
        });

        if slash {
            self.all_apps_search_active = true;
            return;
        }

        if page_down || page_up || (modifiers.shift && (down || up)) {
            // Positive delta scrolls the view down.
            if page_down || down {
                self.all_apps_scroll_delta += self.all_apps_scroll_step;
            } else {
                self.all_apps_scroll_delta -= self.all_apps_scroll_step;
            }
            return;
        }

        let columns = self.all_apps_columns.max(1);
        let last = self.apps.len() - 1;
        let current = self.selected_app.unwrap_or(0).min(last);

        let next = if down || up {
            let rows = grouped_app_rows(&self.apps, columns);
            Some(vertical_app_neighbor(&rows, current, down))
        } else if right {
            Some((current + 1).min(last))
        } else if left {
            Some(current.saturating_sub(1))
        } else {
            None
        };

        if let Some(next) = next
            && self.selected_app != Some(next)
        {
            self.selected_app = Some(next);
            self.all_apps_scroll_to_selected = true;
        }
    }

    /// Handles the group and application hotkeys of the keyboard launcher.
    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        // Binds are stored as bare keys, so a modified press (`Ctrl+H`, ...) is
        // a page shortcut, not an application hotkey.
        let modifiers = ctx.input(|i| i.modifiers);
        if modifiers.ctrl || modifiers.alt || modifiers.command {
            return;
        }

        // Selecting a group must never launch an app in the same frame:
        // even a single-app (or same-key) group requires two key presses.
        let mut selection_changed = false;

        for (index, group) in self.binds.iter().enumerate() {
            if let Some(key) = Self::get_key(&group.bind)
                && ctx.input(|i| i.key_pressed(key))
                && self.selected_group != Some(index)
            {
                self.selected_group = Some(index);
                selection_changed = true;
            }
        }

        if !selection_changed
            && let Some(group) = self.selected_group.and_then(|index| self.binds.get(index))
        {
            for app in &group.apps {
                if let Some(key) = Self::get_key(&app.bind)
                    && ctx.input(|i| i.key_pressed(key))
                {
                    Self::exec_app(ctx, &app.exec);
                    break;
                }
            }
        }
    }

    /// Radial group/app graph for the keyboard launcher, drawn into the given
    /// page `ui` so it can slide together with the tab bar.
    fn draw_keyboard_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, input_locked: bool) {
        let icon_paths: Vec<String> = self
            .binds
            .iter()
            .flat_map(|group| group.apps.iter())
            .filter_map(|app| app.icon.clone())
            .collect();

        for icon_path in icon_paths {
            self.ensure_icon_texture(ctx, &icon_path);
        }

        let g = &self.graphic;

        // Pointer input is sampled once per frame and hit-tested manually against
        // the painted shapes, since the graph is drawn with a raw `Painter`
        // instead of interactive egui widgets.
        let (click_pos, rebind_pos, delete_pos, hover_pos) = if input_locked {
            // Ignore graph input while a shortcut is being assigned or deleted.
            (None, None, None, None)
        } else {
            ctx.input(|i| {
                let clicked = |button| {
                    i.pointer
                        .button_clicked(button)
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                };

                (
                    clicked(egui::PointerButton::Primary),
                    clicked(egui::PointerButton::Secondary),
                    clicked(egui::PointerButton::Middle),
                    i.pointer.hover_pos(),
                )
            })
        };

        let mut app_to_execute: Option<String> = None;
        let mut group_to_select: Option<usize> = None;
        let mut rebind_request: Option<(usize, usize)> = None;
        let mut delete_request: Option<(usize, usize)> = None;
        let mut hovering_app = false;

        {
            let painter = ui.painter().clone();
            let center = ui.max_rect().center();

            if !self.binds.is_empty() {
                let groups_count = self.binds.len();
                let step_rad = f32::consts::TAU / groups_count as f32;

                self.binds.iter().enumerate().for_each(|(index, group)| {
                    let is_selected = self.selected_group.is_some_and(|s| s == index);

                    let start_rad = step_rad * (index as f32);
                    let end_rad = start_rad + step_rad;

                    // Click inside the wedge selects its group.
                    if let Some(pos) = click_pos {
                        let delta = pos - center;
                        let mut angle = (-delta.y).atan2(delta.x);
                        if angle < 0.0 {
                            angle += f32::consts::TAU;
                        }

                        if delta.length() <= g.segment_radius
                            && delta.length() >= g.center_radius
                            && angle >= start_rad
                            && angle <= end_rad
                        {
                            group_to_select = Some(index);
                        }
                    }

                    if !group.apps.is_empty() {
                        let apps_count = group.apps.len();
                        let center_rad = step_rad / 2.0 + start_rad;
                        let apps_start_deg = center_rad
                            - (((apps_count as i32 / 2) - 1) as f32 * g.apps_spacing_rad + {
                                if apps_count % 2 == 1 {
                                    g.apps_spacing_rad
                                } else {
                                    g.apps_spacing_rad / 2.0
                                }
                            });

                        // Draw Radar
                        if is_selected {
                            self.draw_radar(&painter, center, start_rad, end_rad);
                        }

                        group.apps.iter().enumerate().for_each(|(i, app)| {
                            let crt_app_rad = apps_start_deg + g.apps_spacing_rad * i as f32;

                            let app_pos = center
                                + Vec2::new(
                                    g.app_offset * crt_app_rad.cos(),
                                    g.app_offset * -crt_app_rad.sin(),
                                );

                            if let Some(pos) = click_pos
                                && pos.distance(app_pos) <= g.app_radius
                            {
                                app_to_execute = Some(app.exec.clone());
                            }

                            // Right-click rewrites the launch key.
                            if let Some(pos) = rebind_pos
                                && pos.distance(app_pos) <= g.app_radius
                            {
                                rebind_request = Some((index, i));
                            }

                            // Middle-click asks to delete the shortcut.
                            if let Some(pos) = delete_pos
                                && pos.distance(app_pos) <= g.app_radius
                            {
                                delete_request = Some((index, i));
                            }

                            if let Some(pos) = hover_pos
                                && pos.distance(app_pos) <= g.app_radius
                            {
                                hovering_app = true;
                            }

                            // Draw Lines
                            self.draw_line(&painter, center, center_rad, crt_app_rad, is_selected);

                            // Draw AppText
                            self.draw_app_text(
                                &painter,
                                &app.name,
                                center,
                                crt_app_rad,
                                is_selected,
                            );

                            // Draw Apps
                            self.draw_apps(&painter, center, crt_app_rad, is_selected, app);
                        });
                    }

                    // Draw Segment
                    self.draw_segment(
                        &painter,
                        center,
                        start_rad,
                        end_rad,
                        is_selected,
                        group.bind.clone(),
                    );
                });
            }

            // Draw CenterCircle
            painter.circle_filled(center, g.center_radius, Self::get_color32(g.center_color));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                g.middle_text.clone(),
                FontId::monospace(g.middle_text_size),
                Self::get_color32(g.middle_text_color),
            );
        }

        if let Some(index) = group_to_select {
            self.selected_group = Some(index);
        }

        if let Some((group_index, app_index)) = rebind_request
            && let Some(app) = self
                .binds
                .get(group_index)
                .and_then(|group| group.apps.get(app_index))
        {
            let link = AppLink {
                name: app.name.clone(),
                exec: app.exec.clone(),
                icon: app.icon.clone(),
            };

            self.pending_assign = Some(PendingAssign::new(link));
        }

        if let Some((group_index, app_index)) = delete_request {
            self.pending_delete = Some(PendingDelete {
                group_index,
                app_index,
            });
        }

        if let Some(exec) = app_to_execute {
            Self::exec_app(ctx, &exec);
        } else if hovering_app {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_links(names: &[&str]) -> Vec<AppLink> {
        names
            .iter()
            .map(|name| AppLink {
                name: (*name).to_string(),
                exec: String::new(),
                icon: None,
            })
            .collect()
    }

    fn test_launcher(names: &[&str]) -> Hring {
        Hring {
            apps: app_links(names),
            binds: Vec::new(),
            graphic: crate::data::Graphic::default(),
            icon_textures: Default::default(),
            icon_loader: None,
            from_config_loader: std::sync::mpsc::channel().1,
            to_search_worker: std::sync::mpsc::channel().0,
            from_search_worker: std::sync::mpsc::channel().1,
            was_updated_from_config_loader: true,
            search_text: String::new(),
            view: View::AllApps,
            selected_group: None,
            pending_assign: None,
            pending_delete: None,
            selected_app: None,
            all_apps_columns: 1,
            all_apps_scroll_delta: 0.0,
            all_apps_scroll_step: 120.0,
            all_apps_scroll_to_selected: false,
            all_apps_group_focus: None,
            all_apps_search_active: false,
        }
    }

    fn grid_frame(
        ctx: &egui::Context,
        launcher: &mut Hring,
        time: f64,
        events: Vec<egui::Event>,
        input_locked: bool,
    ) -> egui::FullOutput {
        grid_frame_with_size(
            ctx,
            launcher,
            time,
            events,
            input_locked,
            Vec2::new(1000.0, 700.0),
        )
    }

    fn grid_frame_with_size(
        ctx: &egui::Context,
        launcher: &mut Hring,
        time: f64,
        events: Vec<egui::Event>,
        input_locked: bool,
        size: Vec2,
    ) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(time),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    launcher.draw_all_apps_page(ui, ctx, input_locked);
                });
                if !input_locked {
                    launcher.handle_all_apps_nav(ctx);
                }
            },
        )
    }

    fn key_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn every_letter_including_shifted_navigation_letters_jumps_to_its_group() {
        let names: Vec<_> = ('a'..='z').map(|letter| format!("{letter}-app")).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        for shift in [false, true] {
            for (index, letter) in ('A'..='Z').enumerate() {
                let ctx = egui::Context::default();
                let mut launcher = test_launcher(&names);
                let modifiers = egui::Modifiers {
                    shift,
                    ..Default::default()
                };
                let key = Key::from_name(&letter.to_string()).unwrap();
                grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
                grid_frame(
                    &ctx,
                    &mut launcher,
                    0.1,
                    vec![key_event(key, modifiers)],
                    false,
                );
                assert_eq!(launcher.all_apps_group_focus, Some((letter, 0.1)));
                assert_eq!(launcher.selected_app, Some(index));
                assert_eq!(launcher.all_apps_scroll_delta, 0.0);
                assert!(launcher.search_text.is_empty());
            }
        }
    }

    #[test]
    fn keyboard_letter_jump_scrolls_and_replaces_the_highlight() {
        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["alpha", "bravo", "charlie", "delta", "zulu"]);
        grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
        let requested = grid_frame(
            &ctx,
            &mut launcher,
            0.1,
            vec![key_event(Key::Z, Default::default())],
            false,
        );
        grid_frame(&ctx, &mut launcher, 0.7, Vec::new(), false);
        let settled = grid_frame(&ctx, &mut launcher, 0.8, Vec::new(), false);
        assert!(
            letter_text(&settled, "Z", 1).pos.y < letter_text(&requested, "Z", 1).pos.y - 100.0
        );
        assert!(letter_text(&settled, "A", 1).fallback_color.a() < 200);
        assert_group_is_centered(&settled, "Z");

        grid_frame(
            &ctx,
            &mut launcher,
            1.0,
            vec![key_event(Key::A, Default::default())],
            false,
        );
        assert_eq!(launcher.all_apps_group_focus, Some(('A', 1.0)));
        assert_eq!(launcher.selected_app, Some(0));
        grid_frame(&ctx, &mut launcher, 1.6, Vec::new(), false);
        let first_centered = grid_frame(&ctx, &mut launcher, 1.7, Vec::new(), false);
        assert_group_is_centered(&first_centered, "A");
        grid_frame(&ctx, &mut launcher, 3.2, Vec::new(), false);
        assert!(launcher.all_apps_group_focus.is_none());
    }

    #[test]
    fn keyboard_jumps_respect_search_locks_modifiers_and_missing_groups() {
        for (searching, locked, modifiers, key) in [
            (true, false, egui::Modifiers::NONE, Key::J),
            (false, true, egui::Modifiers::NONE, Key::J),
            (false, false, egui::Modifiers::CTRL, Key::J),
            (false, false, egui::Modifiers::ALT, Key::J),
            (false, false, egui::Modifiers::COMMAND, Key::J),
            (false, false, egui::Modifiers::NONE, Key::B),
        ] {
            let ctx = egui::Context::default();
            let mut launcher = test_launcher(&["alpha", "juliet", "zulu"]);
            launcher.all_apps_search_active = searching;
            grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), locked);
            let mut events = vec![key_event(key, modifiers)];
            if searching {
                events.push(egui::Event::Text("j".to_string()));
            }
            grid_frame(&ctx, &mut launcher, 0.1, events, locked);
            assert!(launcher.all_apps_group_focus.is_none());
            assert_eq!(launcher.selected_app, Some(0));
            if searching {
                assert_eq!(launcher.search_text, "j");
            }
        }
    }

    #[test]
    fn arrows_navigate_group_rows_and_page_keys_scroll() {
        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["a1", "a2", "a3", "a4", "b1", "b2", "c1"]);
        launcher.graphic.all_programs.min_cell_width = 250.0;
        grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
        assert_eq!(launcher.all_apps_columns, 3);
        launcher.selected_app = Some(2);
        grid_frame(
            &ctx,
            &mut launcher,
            0.1,
            vec![key_event(Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(launcher.selected_app, Some(3));
        grid_frame(
            &ctx,
            &mut launcher,
            0.2,
            vec![key_event(Key::ArrowRight, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(launcher.selected_app, Some(4));
        grid_frame(
            &ctx,
            &mut launcher,
            0.3,
            vec![key_event(Key::PageDown, egui::Modifiers::NONE)],
            false,
        );
        assert!(launcher.all_apps_scroll_delta > 0.0);
        assert_eq!(launcher.selected_app, Some(4));
        assert!(launcher.all_apps_group_focus.is_none());
    }

    fn letter_text(
        output: &egui::FullOutput,
        letter: &str,
        occurrence: usize,
    ) -> egui::epaint::TextShape {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == letter => Some(text.clone()),
                _ => None,
            })
            .nth(occurrence)
            .expect("letter should be drawn")
    }

    fn letter_clip(output: &egui::FullOutput, letter: &str, occurrence: usize) -> egui::Rect {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == letter => Some(shape.clip_rect),
                _ => None,
            })
            .nth(occurrence)
            .expect("letter should be drawn")
    }

    fn selected_card_rect(output: &egui::FullOutput) -> egui::Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.stroke.width == 2.0 && rect.stroke.color.a() == 255 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("selected app should have an outline")
    }

    fn assert_group_is_centered(output: &egui::FullOutput, letter: &str) {
        let heading = letter_text(output, letter, 1);
        let group_center = (heading.pos.y + selected_card_rect(output).bottom()) / 2.0;
        let viewport_center = letter_clip(output, letter, 1).center().y;
        assert!(
            (group_center - viewport_center).abs() < 8.0,
            "group center={group_center}, viewport center={viewport_center}, heading={:?}, card={:?}",
            heading.pos,
            selected_card_rect(output)
        );
    }

    fn index_button_rect(output: &egui::FullOutput, letter: &str) -> egui::Rect {
        let text = letter_text(output, letter, 0);
        let center = text.pos + text.galley.size() * 0.5;
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.width() <= 72.1 && rect.rect.center().distance(center) < 1.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("letter button should be drawn")
    }

    #[test]
    fn left_dock_magnifies_near_pointer_without_moving_the_grid() {
        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["alpha", "bravo", "zulu"]);
        grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
        let initial = grid_frame(&ctx, &mut launcher, 0.05, Vec::new(), false);
        assert_group_is_centered(&initial, "A");
        let button = index_button_rect(&initial, "A");
        let next = index_button_rect(&initial, "B");
        assert!(button.center().x < 120.0);
        assert_eq!(button.center().x, next.center().x);
        assert!(next.center().y > button.center().y);
        assert!(button.width() >= 48.0);
        let card = selected_card_rect(&initial);
        assert!(card.left() > button.right());

        grid_frame(
            &ctx,
            &mut launcher,
            0.1,
            vec![egui::Event::PointerMoved(button.center())],
            false,
        );
        grid_frame(&ctx, &mut launcher, 0.3, Vec::new(), false);
        let enlarged = grid_frame(&ctx, &mut launcher, 0.4, Vec::new(), false);
        let enlarged_button = index_button_rect(&enlarged, "A");
        assert!(enlarged_button.width() > button.width() + 10.0);
        assert!(
            letter_text(&enlarged, "A", 0).galley.size().y
                > letter_text(&initial, "A", 0).galley.size().y
        );
        assert_eq!(enlarged_button.center().x, button.center().x);
        for letter in ('A'..='Z').chain(std::iter::once('#')) {
            let label = letter.to_string();
            let text = letter_text(&enlarged, &label, 0);
            assert!(
                letter_clip(&enlarged, &label, 0)
                    .contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size()))
            );
        }
        assert_eq!(selected_card_rect(&enlarged), card);

        grid_frame(
            &ctx,
            &mut launcher,
            0.5,
            vec![egui::Event::PointerMoved(egui::Pos2::ZERO)],
            false,
        );
        grid_frame(&ctx, &mut launcher, 0.7, Vec::new(), false);
        let restored = grid_frame(&ctx, &mut launcher, 0.8, Vec::new(), false);
        assert!((index_button_rect(&restored, "A").width() - button.width()).abs() < 0.1);
        assert_eq!(selected_card_rect(&restored), card);
    }

    #[test]
    fn dock_keeps_all_letters_visible_across_screen_sizes_and_ignores_wheel_scrolling() {
        let names: Vec<_> = ('a'..='z').map(|letter| format!("{letter}-app")).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        for size in [
            Vec2::new(800.0, 480.0),
            Vec2::new(1000.0, 700.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(2560.0, 2160.0),
        ] {
            let ctx = egui::Context::default();
            let mut launcher = test_launcher(&names);
            grid_frame_with_size(&ctx, &mut launcher, 0.0, Vec::new(), false, size);
            let initial = grid_frame_with_size(&ctx, &mut launcher, 0.05, Vec::new(), false, size);
            let pointer = index_button_rect(&initial, "M").center();
            grid_frame_with_size(
                &ctx,
                &mut launcher,
                0.1,
                vec![egui::Event::PointerMoved(pointer)],
                false,
                size,
            );
            grid_frame_with_size(&ctx, &mut launcher, 0.3, Vec::new(), false, size);
            let enlarged = grid_frame_with_size(&ctx, &mut launcher, 0.4, Vec::new(), false, size);
            grid_frame_with_size(
                &ctx,
                &mut launcher,
                0.5,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -300.0),
                    modifiers: Default::default(),
                }],
                false,
                size,
            );
            let after_wheel =
                grid_frame_with_size(&ctx, &mut launcher, 0.6, Vec::new(), false, size);
            for output in [&initial, &enlarged, &after_wheel] {
                let mut previous_bottom = 0.0;
                for letter in ('A'..='Z').chain(std::iter::once('#')) {
                    let label = letter.to_string();
                    let text = letter_text(output, &label, 0);
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                    assert!(
                        letter_clip(output, &label, 0).contains_rect(bounds),
                        "{label} clipped at {size:?}"
                    );
                    assert!(
                        bounds.top() >= previous_bottom,
                        "letters overlap at {size:?}"
                    );
                    previous_bottom = bounds.bottom();
                    assert_eq!(
                        letter_text(&enlarged, &label, 0).pos,
                        letter_text(&after_wheel, &label, 0).pos
                    );
                }
            }
            assert_eq!(
                selected_card_rect(&enlarged),
                selected_card_rect(&after_wheel)
            );
        }
    }

    #[test]
    fn dock_rows_touch_and_boundary_clicks_match_the_hover_highlight() {
        let names: Vec<_> = ('a'..='z').map(|letter| format!("{letter}-app")).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        for offset in [-1.0, 0.0, 1.0] {
            let ctx = egui::Context::default();
            let mut launcher = test_launcher(&names);
            grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
            let initial = grid_frame(&ctx, &mut launcher, 0.05, Vec::new(), false);
            let before = index_button_rect(&initial, "M");
            let pointer = egui::pos2(before.center().x, before.bottom() + offset);
            grid_frame(
                &ctx,
                &mut launcher,
                0.1,
                vec![egui::Event::PointerMoved(pointer)],
                false,
            );
            grid_frame(&ctx, &mut launcher, 0.3, Vec::new(), false);
            let hovered = grid_frame(&ctx, &mut launcher, 0.4, Vec::new(), false);
            for output in [&initial, &hovered] {
                let mut previous_bottom: Option<f32> = None;
                for letter in ('A'..='Z').chain(std::iter::once('#')) {
                    let row = index_button_rect(output, &letter.to_string());
                    if let Some(bottom) = previous_bottom {
                        assert!((row.top() - bottom).abs() < 0.001, "gap before {letter}");
                    }
                    previous_bottom = Some(row.bottom());
                }
            }
            let highlights: Vec<_> = hovered
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill.a() == 95 && (rect.rect.width() - 72.0).abs() < 0.01 =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                highlights.len(),
                1,
                "exactly one click target should be highlighted"
            );
            assert!(highlights[0].contains(pointer));
            let highlighted_letter = ('A'..='Z')
                .find(|letter| {
                    let text = letter_text(&hovered, &letter.to_string(), 0);
                    highlights[0].contains(text.pos + text.galley.size() * 0.5)
                })
                .expect("the hovered row should identify its letter");
            for (time, pressed) in [(0.5, true), (0.6, false)] {
                grid_frame(
                    &ctx,
                    &mut launcher,
                    time,
                    vec![egui::Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                    false,
                );
            }
            assert_eq!(
                launcher.all_apps_group_focus,
                Some((highlighted_letter, 0.6))
            );
            assert_eq!(
                launcher.selected_app,
                Some((highlighted_letter as u8 - b'A') as usize)
            );
        }
    }

    fn click_letter(
        ctx: &egui::Context,
        launcher: &mut Hring,
        letter: &str,
        locked: bool,
    ) -> egui::FullOutput {
        grid_frame(ctx, launcher, 0.0, Vec::new(), locked);
        let initial = grid_frame(ctx, launcher, 0.05, Vec::new(), locked);
        assert!(letter_clip(&initial, letter, 0).contains(letter_text(&initial, letter, 0).pos));
        let text = letter_text(&initial, letter, 0);
        let pos = text.pos + text.galley.size() * 0.5;
        grid_frame(
            ctx,
            launcher,
            0.1,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            locked,
        );
        grid_frame(
            ctx,
            launcher,
            0.15,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            locked,
        )
    }

    #[test]
    fn letter_click_scrolls_to_last_group_and_emphasis_expires() {
        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["alpha", "bravo", "charlie", "delta", "zulu"]);
        let clicked = click_letter(&ctx, &mut launcher, "Z", false);
        assert_eq!(launcher.selected_app, Some(4));
        assert_eq!(launcher.all_apps_group_focus, Some(('Z', 0.15)));

        grid_frame(&ctx, &mut launcher, 0.7, Vec::new(), false);
        let settled = grid_frame(&ctx, &mut launcher, 0.8, Vec::new(), false);
        let heading = letter_text(&settled, "Z", 1);
        assert!(
            heading.pos.y < letter_text(&clicked, "Z", 1).pos.y - 100.0,
            "heading before={} after={}",
            letter_text(&clicked, "Z", 1).pos.y,
            heading.pos.y
        );
        assert_group_is_centered(&settled, "Z");
        assert_eq!(heading.fallback_color.a(), 255);
        let dim_alpha = letter_text(&settled, "A", 1).fallback_color.a();
        assert!(dim_alpha < 200);

        let fading = grid_frame(&ctx, &mut launcher, 1.85, Vec::new(), false);
        let alpha = letter_text(&fading, "A", 1).fallback_color.a();
        assert!(alpha > dim_alpha && alpha < 255);
        let restored = grid_frame(&ctx, &mut launcher, 2.3, Vec::new(), false);
        assert!(launcher.all_apps_group_focus.is_none());
        assert_eq!(letter_text(&restored, "A", 1).fallback_color.a(), 255);
    }

    #[test]
    fn unavailable_and_locked_letters_do_not_jump() {
        for (letter, locked) in [("B", false), ("A", true)] {
            let ctx = egui::Context::default();
            let mut launcher = test_launcher(&["alpha", "zulu"]);
            click_letter(&ctx, &mut launcher, letter, locked);
            assert!(launcher.all_apps_group_focus.is_none());
            assert_eq!(launcher.selected_app, Some(0));
        }
    }

    #[test]
    fn group_heading_and_other_character_index_are_clickable() {
        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["alpha", "7zip"]);
        click_letter(&ctx, &mut launcher, "#", false);
        assert_eq!(launcher.all_apps_group_focus, Some(('#', 0.15)));
        assert_eq!(launcher.selected_app, Some(1));

        let ctx = egui::Context::default();
        let mut launcher = test_launcher(&["alpha", "bravo"]);
        grid_frame(&ctx, &mut launcher, 0.0, Vec::new(), false);
        let frame = grid_frame(&ctx, &mut launcher, 0.05, Vec::new(), false);
        let heading = letter_text(&frame, "A", 1);
        let pos = heading.pos + heading.galley.size() * 0.5;
        for (time, pressed) in [(0.1, true), (0.15, false)] {
            grid_frame(
                &ctx,
                &mut launcher,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                false,
            );
        }
        assert_eq!(launcher.all_apps_group_focus, Some(('A', 0.15)));
    }

    #[test]
    fn letter_groups_are_case_insensitive_and_keep_other_names() {
        for (name, expected) in [
            ("Alacritty", 'A'),
            ("alacritty", 'A'),
            ("  Blender", 'B'),
            ("7zip", '#'),
            ("应用", '#'),
            ("Éditor", '#'),
            ("", '#'),
        ] {
            assert_eq!(app_letter_group(name), expected);
        }

        let mut apps = app_links(&["7zip", "blender", "Zulu", "应用", "Alacritty", "audacity"]);
        sort_app_groups(&mut apps);
        let names: Vec<_> = apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(
            names,
            ["Alacritty", "audacity", "blender", "Zulu", "7zip", "应用"]
        );
    }

    #[test]
    fn each_letter_starts_a_new_grid_row_even_after_filtering() {
        let apps = app_links(&["a1", "a2", "a3", "a4", "b1", "b2", "c1"]);
        assert_eq!(grouped_app_rows(&apps, 3), [0..3, 3..4, 4..6, 6..7]);
        let filtered: Vec<_> = apps
            .iter()
            .filter(|app| app.name.ends_with('1'))
            .cloned()
            .collect();
        assert_eq!(grouped_app_rows(&filtered, 3), [0..1, 1..2, 2..3]);
        assert_eq!(grouped_app_rows(&apps, 0).len(), apps.len());
        assert!(grouped_app_rows(&[], 3).is_empty());
    }

    #[test]
    fn vertical_navigation_follows_partial_rows_and_letter_boundaries() {
        let apps = app_links(&["a1", "a2", "a3", "a4", "b1", "b2", "c1"]);
        let rows = grouped_app_rows(&apps, 3);
        assert_eq!(vertical_app_neighbor(&rows, 2, true), 3);
        assert_eq!(vertical_app_neighbor(&rows, 3, true), 4);
        assert_eq!(vertical_app_neighbor(&rows, 5, false), 3);
        assert_eq!(vertical_app_neighbor(&rows, 5, true), 6);
        assert_eq!(vertical_app_neighbor(&rows, 6, false), 4);
        assert_eq!(vertical_app_neighbor(&rows, 2, false), 2);
        assert_eq!(vertical_app_neighbor(&rows, 6, true), 6);
        assert_eq!(vertical_app_neighbor(&[], 0, true), 0);
    }

    fn app(bind: &str, name: &str) -> App {
        App {
            bind: bind.to_string(),
            name: name.to_string(),
            exec: String::new(),
            icon: None,
        }
    }

    /// The same app key may launch different apps in different groups. Only the
    /// group-scoped lookup is used while capturing an app key.
    #[test]
    fn same_app_key_allowed_in_different_groups() {
        let existing = Group {
            bind: "q".to_string(),
            apps: vec![app("w", "firefox")],
        };
        let target = Group {
            bind: "a".to_string(),
            apps: Vec::new(),
        };

        // The global lookup (used for group keys) still sees the other group.
        assert_eq!(
            Hring::app_using_bind(std::slice::from_ref(&existing), "w", None).as_deref(),
            Some("firefox")
        );

        // Reusing `w` in a fresh group must not be reported as a conflict.
        assert_eq!(Hring::app_using_bind_in_group(&target, "w", None), None);
    }

    /// Within one group the app keys stay unique.
    #[test]
    fn duplicate_app_key_rejected_within_group() {
        let group = Group {
            bind: "a".to_string(),
            apps: vec![app("w", "firefox")],
        };

        assert_eq!(
            Hring::app_using_bind_in_group(&group, "w", None).as_deref(),
            Some("firefox")
        );
        // The app being rebound is not its own conflict.
        assert_eq!(
            Hring::app_using_bind_in_group(&group, "w", Some("firefox")),
            None
        );
    }

    /// Rebinding an app inside group `[a]` to key `a` is allowed, but another
    /// group owning `a` still conflicts.
    #[test]
    fn app_key_may_reuse_own_group_key() {
        let group = Group {
            bind: "a".to_string(),
            apps: Vec::new(),
        };

        assert_eq!(
            Hring::group_using_bind_excluding(std::slice::from_ref(&group), "a", Some(0)),
            None
        );
        assert_eq!(
            Hring::group_using_bind_excluding(std::slice::from_ref(&group), "a", None).as_deref(),
            Some("a")
        );
    }
}
