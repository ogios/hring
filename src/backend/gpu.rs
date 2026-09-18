// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! GPU presentation: `wgpu` renders egui on a surface created directly from
//! the layer-shell `wl_surface` + the Wayland display pointer.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use egui_wgpu::{
    RendererOptions, ScreenDescriptor, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew,
    wgpu::{
        self, CompositeAlphaMode, PresentMode, TextureUsages,
        rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle},
    },
};
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

pub struct Presenter {
    /// Kept alive because the `wgpu::Surface` borrows from it at creation.
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    render_state: egui_wgpu::RenderState,
    format: wgpu::TextureFormat,
    alpha_mode: CompositeAlphaMode,
    /// Currently configured physical size, if any.
    configured: Option<(u32, u32)>,
}

impl Presenter {
    pub fn new(
        conn: &Connection,
        wl_surface: &WlSurface,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let trace = std::env::var_os("HRING_TRACE").is_some();
        let t_total = Instant::now();

        // Probing every backend makes wgpu enumerate GL/EGL too, which is the
        // expensive part of `wgpu` startup on Wayland. Default to Vulkan and
        // only fall back to `all()` if that yields no usable adapter (e.g.
        // GL-only systems). Override with `HRING_GPU_BACKEND=vulkan|gl|all`.
        let requested = backends_from_env();
        let mut candidates = vec![requested];
        if requested != wgpu::Backends::all() {
            candidates.push(wgpu::Backends::all());
        }

        // The display handle must stay valid for the surface's lifetime; the
        // `Connection` outlives this presenter (it is also owned by the
        // calloop `WaylandSource`).
        let backend = conn.backend();
        let display = NonNull::new(backend.display_ptr() as *mut c_void)
            .ok_or("wayland display pointer is null")?;
        let window = NonNull::new(wl_surface.id().as_ptr() as *mut c_void)
            .ok_or("wl_surface pointer is null")?;

        let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
        let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(window));

        let mut last_err: Option<Box<dyn std::error::Error>> = None;
        for backends in candidates {
            let t_instance = Instant::now();
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            });
            let instance_ms = t_instance.elapsed();

            let surface = unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: raw_display,
                    raw_window_handle: raw_window,
                })?
            };
            let surface_ms = t_instance.elapsed();

            let render_state = match pollster::block_on(egui_wgpu::RenderState::create(
                &wgpu_configuration(backends),
                &instance,
                Some(&surface),
                RendererOptions::default(),
            )) {
                Ok(render_state) => render_state,
                Err(err) => {
                    if trace {
                        eprintln!("[hring] wgpu init with {backends:?} failed: {err}");
                    }
                    last_err = Some(Box::new(err));
                    continue;
                }
            };
            let device_ms = t_instance.elapsed();

            let caps = surface.get_capabilities(&render_state.adapter);
            // egui paints premultiplied alpha; transparent overlays need the
            // compositor to composite the buffer as such.
            let alpha_mode = if caps
                .alpha_modes
                .contains(&CompositeAlphaMode::PreMultiplied)
            {
                CompositeAlphaMode::PreMultiplied
            } else if caps.alpha_modes.contains(&CompositeAlphaMode::Inherit) {
                CompositeAlphaMode::Inherit
            } else {
                caps.alpha_modes[0]
            };

            if trace {
                let info = render_state.adapter.get_info();
                eprintln!(
                    "[hring] wgpu init backends={backends:?} adapter={:?} instance={:.0}ms surface={:.0}ms device+renderer={:.0}ms total={:.0}ms",
                    info.name,
                    instance_ms.as_secs_f64() * 1000.0,
                    (surface_ms - instance_ms).as_secs_f64() * 1000.0,
                    (device_ms - surface_ms).as_secs_f64() * 1000.0,
                    t_total.elapsed().as_secs_f64() * 1000.0,
                );
            }

            return Ok(Self {
                _instance: instance,
                surface,
                format: render_state.target_format,
                alpha_mode,
                configured: None,
                render_state,
            });
        }

        Err(last_err.unwrap_or_else(|| "no wgpu adapter found".into()))
    }

    fn configure(&mut self, width: u32, height: u32) {
        let config = self.surface_config(width, height);
        self.surface.configure(&self.render_state.device, &config);
        self.configured = Some((width, height));
    }

    fn surface_config(&self, width: u32, height: u32) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: self.format,
            width,
            height,
            present_mode: PresentMode::Fifo,
            alpha_mode: self.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }

    /// Renders one egui frame into the swapchain and presents it.
    ///
    /// `_surface` is unused (wgpu owns the surface it was created from); it
    /// keeps the signature identical to the CPU presenter.
    pub fn present(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        textures: &egui::TexturesDelta,
        ppp: f32,
        _surface: &WlSurface,
        buf_width: u32,
        buf_height: u32,
    ) {
        if self.configured != Some((buf_width, buf_height)) {
            self.configure(buf_width, buf_height);
        }

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.configure(buf_width, buf_height);
                match self.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(_) => return,
                }
            }
            Err(_) => return,
        };

        let device = self.render_state.device.clone();
        let queue = self.render_state.queue.clone();
        let renderer = self.render_state.renderer.clone();

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        for (id, delta) in &textures.set {
            renderer.write().update_texture(&device, &queue, *id, delta);
        }

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [buf_width, buf_height],
            pixels_per_point: ppp,
        };

        let _ = renderer.write().update_buffers(
            &device,
            &queue,
            &mut encoder,
            primitives,
            &screen_descriptor,
        );

        {
            let mut render_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hring egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();

            renderer
                .read()
                .render(&mut render_pass, primitives, &screen_descriptor);
        }

        for id in &textures.free {
            renderer.write().free_texture(id);
        }

        queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

/// Backends to probe, honoring `HRING_GPU_BACKEND`. Defaults to Vulkan so the
/// slow GL/EGL enumeration is skipped on typical Wayland systems.
fn backends_from_env() -> wgpu::Backends {
    match std::env::var("HRING_GPU_BACKEND").ok().as_deref() {
        Some("vulkan") | Some("vk") => wgpu::Backends::VULKAN,
        Some("gl") | Some("gles") | Some("opengl") => wgpu::Backends::GL,
        Some("all") => wgpu::Backends::all(),
        _ => wgpu::Backends::VULKAN,
    }
}

/// `WgpuConfiguration` whose adapter enumeration is restricted to `backends`,
/// matching the `wgpu::Instance` it is used with.
fn wgpu_configuration(backends: wgpu::Backends) -> WgpuConfiguration {
    WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(WgpuSetupCreateNew {
            instance_descriptor: wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            },
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }),
        ..Default::default()
    }
}
