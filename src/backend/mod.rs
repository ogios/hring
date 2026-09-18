// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Native Wayland backend: `smithay-client-toolkit` + `wlr-layer-shell` overlay.
//!
//! The windowing and input translation are shared; only the presentation
//! differs and is selected at compile time with the `cpu` / `gpu` features.
//! Exactly one of them must be enabled.

#[cfg(all(feature = "cpu", feature = "gpu"))]
compile_error!("enable exactly one of the `cpu` or `gpu` features, not both");

#[cfg(not(any(feature = "cpu", feature = "gpu")))]
compile_error!("enable one of the `cpu` or `gpu` features");

pub mod wayland;

#[cfg(feature = "cpu")]
pub mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(feature = "cpu")]
pub use cpu::Presenter;
#[cfg(feature = "gpu")]
pub use gpu::Presenter;

pub use wayland::run;
