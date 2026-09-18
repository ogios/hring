# Hring → Wayland CPU-rendering backend — migration handoff

Handoff notes for a fresh session. Goal: replace the current **eframe + glow (GPU)** stack
with a directly-driven Wayland client using **Smithay-client-toolkit + a CPU rasterizer**,
mainly to cut startup latency, while keeping transparency and gaining a proper overlay.

---

## 0. Decision / target architecture

Drop `eframe` (and `glow`/`wgpu`/`glutin`). Drive the client directly:

| Concern | Choice |
|---|---|
| Wayland client | `smithay-client-toolkit` 0.20 (`wayland-client` 0.31) |
| Surface role | `wlr-layer-shell` overlay (fallback `xdg-shell`) |
| Presentation | `wl_shm` buffer, `wl_shm::Format::Argb8888` (premultiplied) |
| UI + raster | `egui` + `egui_software_backend` (`EguiSoftwareRender`) |
| Event loop | `calloop` (SCTK default feature) |

`egui_software_backend` versions: **0.0.2 ↔ egui 0.33**, **0.0.3 ↔ egui 0.34** (both on crates.io).
hring currently uses egui 0.33 via eframe 0.33.3. Either stay on 0.33 + backend 0.0.2, or bump egui
to 0.34 + backend 0.0.3. Use `default-features = false, features = ["std"]` for the backend if you
only want the renderer (avoids pulling its winit/softbuffer integration).

---

## 1. Why this direction — measured numbers (same machine, RTX 4060 Laptop)

Startup = process start → first present (client side).

| Stack | Env | First present |
|---|---|---|
| eframe + glow (current hring) | Xvfb/X11 | ~138–181 ms |
| softbuffer + egui CPU (egui_software_backend winit runner) | Xvfb/X11 | ~56–62 ms |
| **SCTK + ARGB shm + egui CPU** (spike) | **real Wayland, niri** | **3.3–6.8 ms** |

Other facts established:
- Config/cache load in `Hring::default()` is **< 2 ms**; not a factor.
- Software GL (`LIBGL_ALWAYS_SOFTWARE=1`, llvmpipe) is **not** faster than hardware GL: it still
  creates a GL context. The win comes from skipping GL entirely, not from CPU vs GPU pixels.
- The ~100 ms of the eframe path is mostly window + GL/EGL context creation, before the first frame.

---

## 2. Critical findings / gotchas

1. **Wayland transparency works** via `wl_shm` `ARGB8888`. `softbuffer`'s Wayland backend hardcodes
   `Xrgb8888`, so softbuffer is opaque there — that is a softbuffer limitation, not Wayland's.
2. **`egui_software_backend` keeps alpha, but only if you clear the destination first.**
   You must `canvas.fill(0)` (transparent premultiplied) before every render. Otherwise previous
   frame content accumulates in the reused buffer and alpha climbs to 255 → window looks opaque.
   Verified: with clear → first-frame alpha `min=170 / max=255`; without → `255 / 255`.
3. Caching mode does **not** need to be disabled. `with_caching(true)` (default) preserves alpha as
   long as the shm buffer is cleared first. (An earlier "caching is opaque" observation was really
   the un-cleared buffer.)
4. `ColorFieldOrder::Bgra` matches `wl_shm` `ARGB8888` little-endian (bytes B,G,R,A) + egui's
   premultiplied `Color32`.
5. Requesting `wl_surface.frame` on every draw creates an **infinite redraw loop** in the spike.
   Only request a frame callback when you are animating.
6. `wlr-layer-shell` works on niri. Plan an `xdg-shell` fallback for compositors without it.

---

## 3. Repository state

- Repo: `/home/ogios/work/hring`, Rust, edition 2024.
- `git log`: HEAD `651da4b opt` (delayed background rescan already committed), previous `33b1aa0 cache`.
- Working tree clean.
- `Cargo.toml` deps today: `eframe 0.33.3`, `serde`, `toml`, `bincode`, `homedir`,
  `freedesktop_entry_parser`, `image`, `resvg`.
- `[profile.release]` is `opt-level = 'z'` (size). With CPU rasterization this should probably become
  `3`; measure both.
- eframe/viewport/winit usage: ~26 sites across `src/main.rs`, `src/app.rs`, `src/ui.rs`,
  `src/helpers.rs`, `src/data.rs`, `src/icon.rs`.

Current window options in `src/main.rs`: decorations off, always-on-top, fullscreen, transparent.
On Wayland those map to: layer-shell (overlay layer), no decorations, keyboard interactivity, no
exclusive zone.

---

## 4. Working spike (reference implementation)

- `/tmp/opencode/sctk_spike/` — buildable SCTK+ARGB+egui spike (`Cargo.toml`, `src/main.rs`).
  Env toggles: `SPIKE_RAW=1` (skip egui, raw premultiplied fill), `SPIKE_NOCACHE=1`,
  `SPIKE_ALPHA=1` (print first-frame alpha min/max).
  Run on the live session: `timeout 5 /tmp/opencode/sctk_spike/target/release/sctk_spike`.
- `/tmp/opencode/egui_sb/` — git clone of `egui_software_backend` used as a path dep during probing
  (locally instrumented; **for the real migration prefer the crates.io release**, not this clone).

Core snippet from the spike (`Spike::draw`):

```rust
let (buffer, canvas) = self
    .pool
    .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)?;

canvas.fill(0); // REQUIRED: transparent premultiplied clear

let out = self.ctx.run(raw_input, |ctx| { /* draw egui UI */ });
let primitives = self.ctx.tessellate(out.shapes, out.pixels_per_point);

let data: &mut [[u8; 4]] = bytemuck::cast_slice_mut(&mut *canvas);
let mut buffer_ref = BufferMutRef::new(data, width as usize, height as usize);
self.renderer
    .render(&mut buffer_ref, &primitives, &out.textures_delta, out.pixels_per_point);

layer_surface.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
buffer.attach_to(layer_surface.wl_surface())?;
layer_surface.commit();
```

Layer surface setup (spike):

```rust
let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("hring"), None);
layer.set_anchor(Anchor::empty());
layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
layer.set_size(w, h);
layer.commit(); // initial commit with no buffer → wait for configure → then draw
```

---

## 5. Implementation plan

### Phase 0 — branch + deps
- Branch (e.g. `wayland-software-backend`).
- `Cargo.toml`:
  - remove `eframe`.
  - add `egui` (with `default_fonts`), `egui_software_backend` (0.0.2 for egui 0.33, or 0.0.3 + egui 0.34),
    `smithay-client-toolkit = "0.20"`, `wayland-client = "0.31"`, `calloop`, `bytemuck`.
  - consider enabling `egui_software_backend`'s `rayon` feature for multi-core raster.
- Replace `use eframe::egui::...` with `use egui::...` throughout. Mechanical.

### Phase 1 — runner skeleton
New module (e.g. `src/backend/wayland.rs`) implementing SCTK state + delegates:
`compositor`, `output`, `shm`, `seat`, `keyboard`, `pointer`, and `wlr_layer` (or `xdg`).
Connect, bind globals, create the overlay surface on the focused output, allocate ARGB shm pool,
draw a placeholder frame. **Verify transparency on niri** (use the spike's clear-buffer rule).

### Phase 2 — port Hring drawing
- Replace `impl eframe::App for Hring { fn update(...) }` with:
  build `egui::RawInput` → `ctx.run(...)` → tessellate → render into canvas → attach/commit.
- `Hring::default()` stays as-is: it already reads config + caches and spawns the background
  rescan thread. Icon textures still work: `ctx.load_texture(...)` results arrive in
  `out.textures_delta`, which `EguiSoftwareRender::render` consumes.
- `helpers.rs::exec_app` currently does `ctx.send_viewport_cmd(ViewportCommand::Close)`; replace with
  a signal that exits the SCTK event loop (run the child command, then quit).

### Phase 3 — input translation (largest chunk)
- `KeyboardHandler`: map xkb `Keysym`/`KeyEvent` → `egui::Key`; produce `egui::Event::Text` for text
  (needed by the All-Programs search box); modifiers; key repeat.
- `PointerHandler`: `PointerEvent` (Motion/Enter/Leave/Press/Release/Axis) → egui pointer events.
- Assemble `egui::RawInput { events, modifiers, screen_rect, time, .. }`.
- `egui-winit` cannot be reused (it requires a winit window). Either port its key map (Apache/MIT)
  or write a minimal map covering the keys hring uses.

### Phase 4 — repaint / animation / lifecycle
- `ctx.set_request_repaint_callback(...)` → wake the calloop loop and redraw.
- Use `wl_surface.frame` to throttle; request only while animating.
- Handle `configure` (size), output `scale`/enter/leave, buffer scale.
- Esc and successful launch close the surface and exit the process.

### Phase 5 — cleanup + verify
- Remove eframe-era leftovers (`NativeOptions`, `ViewportCommand`, `eframe::Frame`).
- No new **system** deps (wayland/xkbcommon already required); PKGBUILD unchanged.
- Re-measure and decide `opt-level` (3 vs `z`).

---

## 6. Acceptance criteria

- Transparent fullscreen overlay on niri; no opaque/black box; no visual artifacts.
- Time-to-first-present in the single-digit-ms range (excluding the first full-scene CPU raster).
- Hotkeys launch apps; Esc closes; a launched app closes hring; search typing works.
- Animation smooth enough (measure; enable rayon if needed).
- Dependency tree no longer contains eframe / glow / wgpu / glutin / winit / softbuffer.

---

## 6b. Perf follow-up — measured on i7-14650HX, niri, HDMI 3840x2160 @ scale 1.5

Baseline (no rayon, integer scale): buffer **5120x2880** (1.78x the physical
3840x2160 because scale 1.5 was rounded up to 2). Idle frame ~13 ms, heaviest
page-slide frames 25–44 ms → well under 30 fps.

Two fixes, both in `src/backend/wayland.rs` / `Cargo.toml`:

1. **Enabled `egui_software_backend`'s `rayon` feature** — parallel raster,
   canvas-tile compositing and blit.
2. **`wp_fractional_scale_v1` + `wp_viewport`** (bound directly; SCTK 0.20 does
   not wrap them). The buffer is now the exact physical size (3840x2160 at
   ppp 1.5), buffer scale 1 with a viewport destination of the logical size.
   Falls back to the integer `wl_output` scale when the globals are absent.
   This also removes a downscale (5120→3840) that made the UI slightly soft.

Result: idle frame ~3 ms, worst-case page-slide frames 11–17 ms (≈60–90 fps).
`HRING_TRACE=1` prints per-frame `prep/run/tess/render/total` timings.

---

## 6c. Renderer selection — `cpu` vs `gpu` Cargo features

Exactly one of the features must be enabled (`backend/mod.rs` enforces it):

| Feature | Presentation |
|---|---|
| `cpu` (default) | `egui_software_backend` rasterizes into a `wl_shm` ARGB8888 buffer. |
| `gpu` | `wgpu` + `egui-wgpu` render into a swapchain created from the layer surface. |

`src/backend/wayland.rs` keeps the shell, event loop and input translation;
only presentation moved into `src/backend/cpu.rs` / `src/backend/gpu.rs`
(identical `Presenter::present` signature). The `gpu` presenter builds a
`wgpu::Surface` from the Wayland display pointer + `wl_surface` id
(`wayland-backend`'s `client_system` feature), then an `egui_wgpu::RenderState`.
It clears to `wgpu::Color::TRANSPARENT` and uses `CompositeAlphaMode::PreMultiplied`.

Measured on the same machine/output as 6b:

| | first present | idle frame | All Programs, ~136 prims |
|---|---|---|---|
| CPU | ~30 ms | ~3 ms | ~3.5 ms (busy page-slide ~11–17 ms) |
| GPU | ~720 ms (wgpu init) | ~1.5 ms | ~2.8 ms |

Both were verified visually with `grim`; transparency and layout match.
GPU buys lower per-frame time at the cost of ~0.7 s startup.

### 6d. Cutting the GPU startup cost

`RenderState::create` calls `instance.enumerate_adapters(backends)` before
requesting one, so probing `Backends::all()` also enumerates GL/EGL — the slow
part on Wayland. `src/backend/gpu.rs` now:

- creates the `wgpu::Instance` and the `WgpuConfiguration` with the **same**
  backend set, defaulting to `Backends::VULKAN`;
- falls back to `Backends::all()` when no adapter can be created from Vulkan
  (GL-only systems keep working);
- allows `HRING_GPU_BACKEND=vulkan|gl|all` to override.

`HRING_TRACE=1` now prints `instance=/surface=/device+renderer=` phase timings
and the selected adapter name for the GPU backend.

---

## 7. Open questions / risks

- **Per-frame CPU cost**: fullscreen 1920×1080 raster of a *trivial* scene was ~20 ms single-thread.
  hring's radar/polygons/text will be more. Measure with `rayon`; consider dirty-rect / partial
  redraw and reducing overdraw.
- **Input completeness**: dead keys, IME, text input for search, pointer cursor shape.
- **Multi-monitor**: which output to cover; layer-shell is per-output.
- **Compositor compatibility**: layer-shell is not universal (GNOME → xdg fallback).
- **Keyboard interactivity** semantics vs the current focus-based hotkey model.

---

## 8. Lower-risk alternative (if full SCTK proves too costly)

Keep `winit` + `egui-winit` for window/input (no input rewrite), and only replace **presentation**:
- present egui software output through a self-managed `ARGB8888` shm buffer attached to winit's
  `wl_surface` (obtained via `raw-window-handle`), or
- vendor/patch `softbuffer`'s Wayland backend to use `Argb8888` instead of `Xrgb8888`.

This still drops eframe/glow and keeps startup fast, but does not give layer-shell.
