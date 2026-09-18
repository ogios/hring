// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Native Wayland backend: `smithay-client-toolkit` + `wlr-layer-shell` overlay.
//!
//! The windowing, event loop and input translation live in [`wayland`]; the
//! presentation is GPU-only (`wgpu` + `egui-wgpu`) in [`gpu`].

pub mod gpu;
pub mod wayland;

pub use gpu::Presenter;
pub use wayland::run;
