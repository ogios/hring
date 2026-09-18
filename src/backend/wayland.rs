// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Native Wayland runner: `smithay-client-toolkit` layer-shell overlay.
//! Presentation is GPU-only: `wgpu` + `egui-wgpu` (see [`crate::backend::gpu`]).
//!
//! Drives `egui::Context::run` once per frame, translates Wayland input into
//! egui events, and throttles redraws to the compositor via `wl_surface.frame`.

use crate::app::Hring;
use crate::backend::Presenter;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{
            BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};

// Re-exported to guarantee the same `wayland-client`/`wayland-protocols`
// versions SCTK was built against.
use smithay_client_toolkit::reexports::protocols::{
    wp::fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{Event as FractionalScaleEvent, WpFractionalScaleV1},
    },
    wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

/// Fractional scale factors are reported in 1/120ths.
const SCALE_DENOMINATOR: f32 = 120.0;

const INITIAL_WIDTH: u32 = 720;
const INITIAL_HEIGHT: u32 = 420;
const APP_ID: &str = "hring";

pub fn run(ctx: egui::Context, app: Hring) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init::<Backend>(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor missing");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell missing");

    // Fractional scaling (`wp_fractional_scale_v1`) lets us render at the exact
    // output scale (e.g. 1.5x) instead of rounding up to an integer factor,
    // which keeps the buffer at the true physical size. `wp_viewport` is required to
    // map the fractional buffer back onto the logical surface size. Both are
    // optional: when absent we fall back to the integer `wl_output` scale.
    let fractional_scale_manager = globals
        .bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
        .ok();
    let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some(APP_ID), None);
    // Anchored to every edge with size 0: the compositor configures us to the
    // full output. Exclusive zone -1 keeps the layer from reserving space.
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    layer.set_size(0, 0);

    let use_fractional = fractional_scale_manager.is_some() && viewporter.is_some();

    let fractional_scale = if use_fractional {
        fractional_scale_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(layer.wl_surface(), &qh, ()))
    } else {
        None
    };
    let viewport = if use_fractional {
        viewporter
            .as_ref()
            .map(|viewporter| viewporter.get_viewport(layer.wl_surface(), &qh, ()))
    } else {
        None
    };

    // Initial commit carries no buffer; wait for the compositor's configure.
    layer.commit();

    let presenter =
        Presenter::new(&conn, layer.wl_surface()).expect("failed to create GPU presenter");

    let repaint_deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let mut backend = Backend {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        exit: false,
        first_configure: true,
        first_present: false,
        presenter,
        width: INITIAL_WIDTH,
        height: INITIAL_HEIGHT,
        scale: 1.0,
        fractional: use_fractional,
        viewport,
        _fractional_scale: fractional_scale,
        layer,
        qh: qh.clone(),
        frame_pending: false,
        keyboard: None,
        pointer: None,
        ctx,
        app,
        events: Vec::new(),
        modifiers: egui::Modifiers::default(),
        needs_redraw: false,
        start,
        repaint_deadline: Arc::clone(&repaint_deadline),
    };

    let mut event_loop: calloop::EventLoop<Backend> = calloop::EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    // `request_repaint` can be called from the UI thread or a background worker.
    // Turn it into a calloop ping (wakes the loop) plus the earliest wake time
    // (honors egui's repaint delay so animations do not busy-loop).
    let (ping, ping_source) = calloop::ping::make_ping()?;
    loop_handle.insert_source(ping_source, |_, _, _: &mut Backend| {})?;

    backend
        .ctx
        .set_request_repaint_callback(move |info: egui::RequestRepaintInfo| {
            let when = Instant::now() + info.delay;
            if let Ok(mut deadline) = repaint_deadline.lock() {
                let merged = deadline.map_or(when, |prev| prev.min(when));
                *deadline = Some(merged);
            }
            ping.ping();
        });

    WaylandSource::new(conn, event_queue).insert(loop_handle)?;

    while !backend.exit {
        let deadline = backend
            .repaint_deadline
            .lock()
            .ok()
            .and_then(|deadline| *deadline);
        // While a frame callback is outstanding the compositor is our clock:
        // block until it (or new input / a ping) arrives.
        let timeout = if backend.frame_pending {
            None
        } else {
            deadline.map(|when| when.saturating_duration_since(Instant::now()))
        };

        event_loop.dispatch(timeout, &mut backend)?;

        if backend.exit {
            break;
        }

        let due = backend
            .repaint_deadline
            .lock()
            .ok()
            .and_then(|deadline| *deadline)
            .is_some_and(|when| when <= Instant::now());

        if (backend.needs_redraw || due) && !backend.frame_pending {
            backend.needs_redraw = false;
            if let Ok(mut deadline) = backend.repaint_deadline.lock() {
                *deadline = None;
            }
            backend.draw();
        }
    }

    Ok(())
}

struct Backend {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,

    exit: bool,
    first_configure: bool,
    first_present: bool,

    presenter: Presenter,
    width: u32,
    height: u32,
    /// Effective pixels per point. Integer `wl_output` scale, or the fractional
    /// scale when `wp_fractional_scale_v1` is in use.
    scale: f32,
    /// Whether the fractional-scale path (buffer scale 1 + viewport) is active.
    fractional: bool,
    viewport: Option<WpViewport>,
    /// Kept alive for the lifetime of the surface; dropping it destroys the
    /// object.
    _fractional_scale: Option<WpFractionalScaleV1>,
    layer: LayerSurface,
    qh: QueueHandle<Backend>,
    /// A `wl_surface.frame` callback is outstanding; do not draw until it fires.
    frame_pending: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,

    ctx: egui::Context,

    app: Hring,

    /// egui events accumulated from Wayland input since the last frame.
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    /// Set by any input handler; drives an immediate redraw after dispatch.
    needs_redraw: bool,
    start: Instant,
    /// Earliest time egui wants another frame (from `request_repaint`).
    repaint_deadline: Arc<Mutex<Option<Instant>>>,
}

impl Backend {
    /// Runs one egui frame, rasterizes it with the compiled presenter and
    /// presents it.
    fn draw(&mut self) {
        let trace = std::env::var_os("HRING_TRACE").is_some();
        let t_frame = Instant::now();

        let ppp = self.scale.max(1.0);
        let width = self.width;
        let height = self.height;
        // `width`/`height` are logical surface coordinates; the buffer is
        // physical, so it is scaled by the effective pixels-per-point.
        let buf_width = (width as f32 * ppp).round() as u32;
        let buf_height = (height as f32 * ppp).round() as u32;

        let t_prep = t_frame.elapsed();

        self.ctx.set_pixels_per_point(ppp);

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(width as f32, height as f32),
            )),
            events: std::mem::take(&mut self.events),
            modifiers: self.modifiers,
            time: Some(self.start.elapsed().as_secs_f64()),
            focused: true,
            ..Default::default()
        };

        let ctx = &self.ctx;
        let app = &mut self.app;

        let out = ctx.run(raw_input, |ctx| app.update_ui(ctx));

        let t_run = t_frame.elapsed();

        // `ViewportCommand::Close` (Escape, or a launched app) stops the loop.
        if out.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close))
        }) {
            self.exit = true;
        }

        let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);

        let t_tess = t_frame.elapsed();

        let surface = self.layer.wl_surface().clone();
        self.sync_surface_scale(&surface, width, height, ppp);
        self.presenter.present(
            &primitives,
            &out.textures_delta,
            out.pixels_per_point,
            &surface,
            buf_width,
            buf_height,
        );

        let t_render = t_frame.elapsed();

        if trace {
            eprintln!(
                "[hring] frame {buf_width}x{buf_height} ppp={ppp} prims={} prep={:.2}ms run={:.2}ms tess={:.2}ms render={:.2}ms total={:.2}ms",
                primitives.len(),
                t_prep.as_secs_f64() * 1000.0,
                (t_run - t_prep).as_secs_f64() * 1000.0,
                (t_tess - t_run).as_secs_f64() * 1000.0,
                (t_render - t_tess).as_secs_f64() * 1000.0,
                t_render.as_secs_f64() * 1000.0,
            );
        }

        if !self.first_present {
            self.first_present = true;
            if std::env::var("HRING_TRACE").is_ok() {
                eprintln!(
                    "[hring] first present {buf_width}x{buf_height} ppp={ppp} fractional={} at {:.2}ms",
                    self.fractional,
                    self.start.elapsed().as_secs_f64() * 1000.0,
                );
            }
        }

        let wants_more = self
            .repaint_deadline
            .lock()
            .ok()
            .and_then(|deadline| *deadline)
            .is_some();

        // Fractional scaling mandates buffer scale 1: the buffer is the exact
        // physical size and the viewport maps it back to the logical surface
        // size. `sync_surface_scale` already configured this before presenting.

        // Only ask for the next frame callback while egui wants to keep
        // animating. It throttles the rasterizer to the compositor's refresh
        // instead of redrawing in a busy loop.
        if wants_more && !self.frame_pending {
            surface.frame(&self.qh, surface.clone());
            self.frame_pending = true;
        }

        self.layer.commit();
    }

    /// Points the compositor at the physical buffer: a viewport destination in
    /// the fractional case, an integer buffer scale otherwise.
    fn sync_surface_scale(
        &self,
        surface: &wl_surface::WlSurface,
        width: u32,
        height: u32,
        ppp: f32,
    ) {
        if self.fractional {
            surface.set_buffer_scale(1);
            if let Some(viewport) = &self.viewport {
                viewport.set_destination(width as i32, height as i32);
            }
        } else {
            surface.set_buffer_scale(ppp.round().max(1.0) as i32);
        }
    }

    /// Updates the effective pixels-per-point used for buffer sizing.
    fn set_scale(&mut self, ppp: f32) {
        if (ppp - self.scale).abs() > f32::EPSILON {
            self.scale = ppp;
            self.needs_redraw = true;
        }
    }

    /// Translate one Wayland key event into egui key/text events.
    fn push_key(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        if !pressed {
            if let Some(key) = keysym_to_key(event.keysym) {
                self.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: self.modifiers,
                });
                self.needs_redraw = true;
            }
            return;
        }

        if let Some(key) = keysym_to_key(event.keysym) {
            self.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat,
                modifiers: self.modifiers,
            });
        }

        // Text input for the search box. Skip control chords (Ctrl/Command) and
        // control characters; Enter is delivered as a key only.
        let ctrl = self.modifiers.ctrl || self.modifiers.command;
        if !ctrl
            && let Some(text) = event.utf8
            && !text.is_empty()
            && text.chars().any(|c| !c.is_control())
        {
            self.events.push(egui::Event::Text(text));
        }

        self.needs_redraw = true;
    }
}

impl CompositorHandler for Backend {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        // When the compositor advertises fractional scaling, the preferred
        // fractional scale is authoritative; the integer hint is rounded up and
        // would make us render more pixels than needed.
        if !self.fractional {
            self.set_scale(new_factor.max(1) as f32);
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // The compositor is ready for the next frame; release the throttle.
        self.frame_pending = false;
        self.needs_redraw = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Backend {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Backend {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(INITIAL_WIDTH, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(INITIAL_HEIGHT, NonZeroU32::get);

        if self.first_configure {
            self.first_configure = false;
            self.draw();
        }
    }
}

impl SeatHandler for Backend {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Backend {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.push_key(event, true, false);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.push_key(event, true, true);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.push_key(event, false, false);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = egui_modifiers(&modifiers);
    }
}

impl PointerHandler for Backend {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let pos = egui::pos2(event.position.0 as f32, event.position.1 as f32);

            match &event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => {
                    self.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Press { button, .. } => {
                    if let Some(button) = pointer_button(*button) {
                        self.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            modifiers: self.modifiers,
                        });
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if let Some(button) = pointer_button(*button) {
                        self.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: false,
                            modifiers: self.modifiers,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    // Wayland: positive vertical = scroll down. egui wants the
                    // opposite sign (positive = content moves down).
                    self.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(horizontal.absolute as f32, -vertical.absolute as f32),
                        modifiers: self.modifiers,
                    });
                }
            }
        }

        self.needs_redraw = true;
    }
}

delegate_compositor!(Backend);
delegate_output!(Backend);
delegate_seat!(Backend);
delegate_keyboard!(Backend);
delegate_pointer!(Backend);
delegate_layer!(Backend);
delegate_registry!(Backend);

// SCTK 0.20 does not wrap `wp_fractional_scale_v1` / `wp_viewporter`, so they
// are bound and dispatched directly. The generated interfaces have no events
// except `wp_fractional_scale_v1.preferred_scale`.
impl Dispatch<WpFractionalScaleManagerV1, ()> for Backend {
    fn event(
        _: &mut Self,
        _proxy: &WpFractionalScaleManagerV1,
        _event: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: FractionalScaleEvent,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            state.fractional = true;
            state.set_scale(scale as f32 / SCALE_DENOMINATOR);
        }
    }
}

impl Dispatch<WpViewporter, ()> for Backend {
    fn event(
        _: &mut Self,
        _proxy: &WpViewporter,
        _event: <WpViewporter as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, ()> for Backend {
    fn event(
        _: &mut Self,
        _proxy: &WpViewport,
        _event: <WpViewport as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl ProvidesRegistryState for Backend {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

fn egui_modifiers(modifiers: &Modifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: modifiers.alt,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        mac_cmd: false,
        // On Linux, egui's "command" is Ctrl.
        command: modifiers.ctrl,
    }
}

fn pointer_button(button: u32) -> Option<egui::PointerButton> {
    match button {
        BTN_LEFT => Some(egui::PointerButton::Primary),
        BTN_RIGHT => Some(egui::PointerButton::Secondary),
        BTN_MIDDLE => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

/// Map an xkb common keysym onto the subset of [`egui::Key`] that hring binds.
///
/// Numeric keysyms are stable X11/XKB values. Letters accept both cases so a
/// shifted key still maps to the same logical key.
fn keysym_to_key(keysym: Keysym) -> Option<egui::Key> {
    const LETTERS: [egui::Key; 26] = [
        egui::Key::A,
        egui::Key::B,
        egui::Key::C,
        egui::Key::D,
        egui::Key::E,
        egui::Key::F,
        egui::Key::G,
        egui::Key::H,
        egui::Key::I,
        egui::Key::J,
        egui::Key::K,
        egui::Key::L,
        egui::Key::M,
        egui::Key::N,
        egui::Key::O,
        egui::Key::P,
        egui::Key::Q,
        egui::Key::R,
        egui::Key::S,
        egui::Key::T,
        egui::Key::U,
        egui::Key::V,
        egui::Key::W,
        egui::Key::X,
        egui::Key::Y,
        egui::Key::Z,
    ];
    const DIGITS: [egui::Key; 10] = [
        egui::Key::Num0,
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
        egui::Key::Num6,
        egui::Key::Num7,
        egui::Key::Num8,
        egui::Key::Num9,
    ];
    const FUNCTION: [egui::Key; 12] = [
        egui::Key::F1,
        egui::Key::F2,
        egui::Key::F3,
        egui::Key::F4,
        egui::Key::F5,
        egui::Key::F6,
        egui::Key::F7,
        egui::Key::F8,
        egui::Key::F9,
        egui::Key::F10,
        egui::Key::F11,
        egui::Key::F12,
    ];

    let raw = keysym.raw();
    match raw {
        0x61..=0x7a => Some(LETTERS[(raw - 0x61) as usize]), // a-z
        0x41..=0x5a => Some(LETTERS[(raw - 0x41) as usize]), // A-Z
        0x30..=0x39 => Some(DIGITS[(raw - 0x30) as usize]),  // 0-9
        0xffbe..=0xffc9 => Some(FUNCTION[(raw - 0xffbe) as usize]), // F1-F12
        0x20 => Some(egui::Key::Space),
        0x21 => Some(egui::Key::Exclamationmark),
        0x22 | 0x27 => Some(egui::Key::Quote),
        0x2b => Some(egui::Key::Plus),
        0x2c | 0x3c => Some(egui::Key::Comma),
        0x2d | 0x5f => Some(egui::Key::Minus),
        0x2e | 0x3e => Some(egui::Key::Period),
        0x2f => Some(egui::Key::Slash),
        0x3a => Some(egui::Key::Colon),
        0x3b => Some(egui::Key::Semicolon),
        0x3d => Some(egui::Key::Equals),
        0x3f => Some(egui::Key::Questionmark),
        0x5b => Some(egui::Key::OpenBracket),
        0x5c => Some(egui::Key::Backslash),
        0x5d => Some(egui::Key::CloseBracket),
        0x60 | 0x7e => Some(egui::Key::Backtick),
        0x7b => Some(egui::Key::OpenCurlyBracket),
        0x7c => Some(egui::Key::Pipe),
        0x7d => Some(egui::Key::CloseCurlyBracket),
        0xff08 => Some(egui::Key::Backspace),
        0xff09 => Some(egui::Key::Tab),
        0xff0d => Some(egui::Key::Enter),
        0xff1b => Some(egui::Key::Escape),
        0xff50 => Some(egui::Key::Home),
        0xff51 => Some(egui::Key::ArrowLeft),
        0xff52 => Some(egui::Key::ArrowUp),
        0xff53 => Some(egui::Key::ArrowRight),
        0xff54 => Some(egui::Key::ArrowDown),
        0xff55 => Some(egui::Key::PageUp),
        0xff56 => Some(egui::Key::PageDown),
        0xff57 => Some(egui::Key::End),
        0xff63 => Some(egui::Key::Insert),
        0xffff => Some(egui::Key::Delete),
        _ => None,
    }
}
