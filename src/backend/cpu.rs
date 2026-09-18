// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! CPU presentation: `egui_software_backend` rasterizes into an ARGB8888
//! `wl_shm` buffer which is then attached to the layer surface.

use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use smithay_client_toolkit::shm::{Shm, slot::SlotPool};
use wayland_client::protocol::wl_surface::WlSurface;

const INITIAL_WIDTH: u32 = 720;
const INITIAL_HEIGHT: u32 = 420;

pub struct Presenter {
    pool: SlotPool,
    renderer: EguiSoftwareRender,
}

impl Presenter {
    pub fn new(shm: &Shm) -> Result<Self, Box<dyn std::error::Error>> {
        let pool = SlotPool::new((INITIAL_WIDTH * INITIAL_HEIGHT * 4) as usize, shm)?;
        Ok(Self {
            pool,
            renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
        })
    }

    /// Rasterizes one frame into a fresh shm buffer and attaches it to
    /// `surface` (the caller commits).
    pub fn present(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        textures: &egui::TexturesDelta,
        ppp: f32,
        surface: &WlSurface,
        buf_width: u32,
        buf_height: u32,
    ) {
        let stride = (buf_width * 4) as i32;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                buf_width as i32,
                buf_height as i32,
                stride,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            )
            .expect("failed to create buffer");

        // REQUIRED: fully transparent premultiplied clear. Without this the
        // reused buffer accumulates alpha until the overlay is opaque.
        canvas.fill(0);

        {
            let data: &mut [[u8; 4]] = bytemuck::cast_slice_mut(&mut *canvas);
            let mut buffer_ref = BufferMutRef::new(data, buf_width as usize, buf_height as usize);
            self.renderer
                .render(&mut buffer_ref, primitives, textures, ppp);
        }

        surface.damage_buffer(0, 0, buf_width as i32, buf_height as i32);
        buffer.attach_to(surface).expect("failed to attach buffer");
    }
}
