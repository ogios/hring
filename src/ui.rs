// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use core::f32;

use eframe::egui::{
    self, Align2, FontId, Frame, Key, Response, RichText, ScrollArea, Vec2, ViewportCommand,
};

use crate::{
    app::{AssignStage, Hring, PendingAssign, PendingDelete, View},
    config,
    data::{App, AppLink, ConfApp, ConfGroup, Group},
};

impl eframe::App for Hring {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if !self.was_updated_from_config_loader
            && let Ok(groups) = self.from_config_loader.try_recv()
        {
            self.binds = groups;
            self.was_updated_from_config_loader = true;
        }

        if let Ok(apps) = self.from_search_worker.try_recv() {
            self.apps = apps;
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
            ctx.send_viewport_cmd(ViewportCommand::Close);
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
    fn create_tab_bar(&self, ctx: &eframe::egui::Context) -> Option<View> {
        let track_color = Self::get_color32(self.graphic.left_panel_color);
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
            .frame(Frame::NONE)
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

                    // Track behind both segments.
                    painter.rect_filled(rect, height / 2.0, track_color);

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
    fn draw_pages(&mut self, ctx: &eframe::egui::Context, modal_active: bool) {
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

        let base_color = Self::get_color32(self.graphic.main_panel_color);
        let mut all_apps_text_edit = None;

        // While sliding, the pages are drawn at an offset, so pointer input would
        // be hit-tested at the wrong place. Lock it until the pages settle.
        let page_input_locked = modal_active || !settled;

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(base_color))
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

        if !modal_active
            && settled
            && self.view == View::AllApps
            && let Some(text_edit) = &all_apps_text_edit
        {
            self.handle_search(text_edit, ctx);
        }
    }

    /// Full-page grid of every installed application, with search. Draws into
    /// the given page `ui`, so it can slide together with the tab bar.
    fn draw_all_apps_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &eframe::egui::Context,
        input_locked: bool,
    ) -> Response {
        let panel_color = Self::get_color32(self.graphic.main_panel_color);
        let font_color = Self::get_color32(self.graphic.menu_items_font_color);
        let hover_color = Self::get_color32(self.graphic.menu_items_hover_color);
        let placeholder_color = Self::get_color32(self.graphic.app_color_unactive);
        // Snapshot the grid tuning now, so the draw closure only captures these
        // plain values instead of borrowing `self.graphic`.
        let ap = self.graphic.all_programs.clone();
        let font_size = (self.graphic.menu_items_font_size * ap.font_scale).max(ap.font_size_min);
        let icon_size = (self.graphic.app_radius * ap.icon_radius_scale)
            .clamp(ap.icon_size_min, ap.icon_size_max);

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
        ui.painter().rect_filled(page_rect, 0.0, panel_color);

        let text_edit = ui
            .scope_builder(
                egui::UiBuilder::new().max_rect(page_rect.shrink(24.0)),
                |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("All Programs").color(font_color).size(18.0));
                        ui.add_space(8.0);

                        let text_edit = ui.add_sized(
                            [ui.available_width(), 26.0],
                            egui::TextEdit::singleline(&mut self.search_text)
                                .hint_text("Search applications..."),
                        );

                        ui.add_space(10.0);

                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
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
                                let side_margin =
                                    ((full_width - content_width) / 2.0 - gap).max(0.0);

                                // Fit as many columns as the capped width allows,
                                // then stretch them so a full row fills it.
                                let available = content_width;
                                let min_cell_width =
                                    (icon_size + padding_x * 2.0 + ap.cell_width_extra)
                                        .max(ap.min_cell_width);
                                let columns = ((available + gap) / (min_cell_width + gap))
                                    .floor()
                                    .max(1.0)
                                    as usize;
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
                                    .max(1.0)
                                    as usize;

                                if apps.is_empty() {
                                    ui.label(
                                        RichText::new("No applications found")
                                            .color(font_color)
                                            .size(font_size),
                                    );
                                }

                                for row in apps.chunks(columns) {
                                    ui.horizontal(|ui| {
                                        if side_margin > 0.0 {
                                            ui.add_space(side_margin);
                                        }
                                        for app in row {
                                            let (rect, response) = ui.allocate_exact_size(
                                                Vec2::new(cell_width, cell_height),
                                                egui::Sense::click(),
                                            );
                                            let response =
                                                response.on_hover_text(app.name.as_str());

                                            let painter = ui.painter();
                                            let hovered = response.hovered();

                                            painter.rect_filled(
                                                rect,
                                                ap.corner_radius,
                                                Self::with_alpha(
                                                    hover_color,
                                                    if hovered {
                                                        ap.hover_alpha
                                                    } else {
                                                        ap.idle_alpha
                                                    },
                                                ),
                                            );
                                            if hovered {
                                                ui.ctx().set_cursor_icon(
                                                    egui::CursorIcon::PointingHand,
                                                );
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
                                            text_painter.galley(
                                                text_pos,
                                                galley.clone(),
                                                font_color,
                                            );
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

        text_edit
    }

    /// Captures the next pressed key: the first one selects an existing group
    /// (or creates a new one), the second becomes the app's launch key.
    /// `Escape` cancels at any point.
    fn handle_pending_assign(&mut self, ctx: &eframe::egui::Context) {
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
    fn handle_pending_delete(&mut self, ctx: &eframe::egui::Context) {
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
    fn show_delete_confirm(&self, ctx: &eframe::egui::Context) -> Option<bool> {
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
    fn show_assign_overlay(&self, ctx: &eframe::egui::Context) {
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
    fn handle_search(&mut self, text_edit: &Response, ctx: &eframe::egui::Context) {
        let enter_pressed = ctx.input(|i| i.key_pressed(Key::Enter));

        if text_edit.changed() {
            self.to_search_worker
                .send(self.search_text.clone())
                .expect("SearchThread not reachable!");
        }

        if enter_pressed {
            if text_edit.has_focus() || text_edit.lost_focus() {
                if self.search_text.is_empty() {
                    text_edit.surrender_focus();
                } else if let Some(app) = self.apps.first() {
                    Self::exec_app(ctx, &app.exec);
                }
            } else {
                text_edit.request_focus();
            }
        }
    }

    /// Handles the group and application hotkeys of the keyboard launcher.
    fn handle_hotkeys(&mut self, ctx: &eframe::egui::Context) {
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
    fn draw_keyboard_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &eframe::egui::Context,
        input_locked: bool,
    ) {
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
            let available_rect = ui.max_rect();
            ui.painter()
                .rect_filled(available_rect, 0.0, Self::get_color32(g.main_panel_color));
            let painter = ui.painter().clone();
            let center = available_rect.center();

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
