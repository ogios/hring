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

        match self.view {
            View::Keyboard => {
                if !modal_active {
                    self.handle_hotkeys(ctx);
                }

                self.create_main_panel(ctx, modal_active);
            }
            View::AllApps => {
                let text_edit = self.create_all_apps_panel(ctx, modal_active);

                if !modal_active {
                    self.handle_search(&text_edit, ctx);
                }
            }
        }

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
        // page switch reads as a slide. egui keeps repainting until it settles.
        let animated_index =
            ctx.animate_value_with_time(egui::Id::new("tab_bar_pill"), active_index as f32, 0.18);

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

    /// Full-page list of every installed application, with search, drawn on a
    /// rounded card.
    fn create_all_apps_panel(
        &mut self,
        ctx: &eframe::egui::Context,
        input_locked: bool,
    ) -> Response {
        let panel_color = Self::get_color32(self.graphic.main_panel_color);
        let font_color = Self::get_color32(self.graphic.menu_items_font_color);
        let hover_color = Self::get_color32(self.graphic.menu_items_hover_color);
        let font_size = self.graphic.menu_items_font_size;

        let mut app_to_execute = None;
        let mut assignment_request: Option<AppLink> = None;

        let text_edit = egui::CentralPanel::default()
            .frame(Frame::NONE.fill(panel_color).inner_margin(24.0))
            .show(ctx, |ui| {
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
                            let width = ui.available_width();

                            for app in &self.apps {
                                let button_text =
                                    RichText::new(&app.name).color(font_color).size(font_size);

                                let btn = egui::Button::selectable(false, button_text)
                                    .fill(hover_color);

                                let app_response = ui.add_sized([width, 24.0], btn);

                                // Right-click starts the two-key shortcut capture.
                                if app_response.secondary_clicked() && !input_locked {
                                    assignment_request = Some(app.clone());
                                }

                                if app_response.clicked() && !input_locked {
                                    app_to_execute = Some(app.exec.clone());
                                };
                            }
                        });

                    text_edit
                })
                .inner
            })
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
                // A group bind must stay unique, otherwise pressing it would
                // both switch groups and launch an app.
                if let Some(group_bind) = self
                    .binds
                    .iter()
                    .find(|group| group.bind.eq_ignore_ascii_case(&bind))
                    .map(|group| group.bind.clone())
                {
                    self.warn_assign(format!(
                        "Key [{bind}] is already the group key [{group_bind}]"
                    ));
                } else if let Some(owner) =
                    Self::app_using_bind(&self.binds, &bind, Some(&app.name))
                {
                    self.warn_assign(format!("Key [{bind}] already launches \"{owner}\""));
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

                    if let Some(group) = self.binds.get_mut(index) {
                        // Drop a previous entry for this app and any app that
                        // already uses the freshly captured key.
                        group
                            .apps
                            .retain(|a| a.name != app.name && !a.bind.eq_ignore_ascii_case(&bind));
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

    /// Returns the name of the first app in any group that uses `bind`.
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

    fn create_main_panel(&mut self, ctx: &eframe::egui::Context, input_locked: bool) {
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

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Self::get_color32(g.main_panel_color)))
            .show(ctx, |ui| {
                let available_rect = ctx.available_rect();
                let painter = ui.painter().with_clip_rect(available_rect);
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
                                self.draw_line(
                                    &painter,
                                    center,
                                    center_rad,
                                    crt_app_rad,
                                    is_selected,
                                );

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
            });

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

            self.pending_assign = Some(PendingAssign::rebind(link, group_index));
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
