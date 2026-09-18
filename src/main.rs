// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

mod app;
mod backend;
mod config;
mod data;
mod data_impl;
mod helpers;
mod icon;
mod ui;

use app::Hring;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = egui::Context::default();
    let app = Hring::new(Some(ctx.clone()));
    backend::run(ctx, app)
}
