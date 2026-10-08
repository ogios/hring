# Welcome to version 0.2.0 of Hring Launcher

**Hring** is a lightweight launcher for Linux systems. It was created with the goal of providing a beautiful, graph-like interface for improved visual perception (I hate lists).

However, the main focus was on **quick keyboard control**.

Controls are divided into groups and applications within them — you can assign any keyboard key to each of them.

# What does it look like?
The app has extensive customization options, including transparency.

![](assets/green_example.png)
![](assets/orange_example.png)
![](assets/blue_example.png)

## Configuration

The project configuration was divided into three files:
- `graphics.toml` --- contains all graphics settings
- `binds.toml` --- contains settings for groups, applications and hotkeys
- `config.toml` --- contains a list of directories for searching .desktop links.

**You don't need to create them manually.** The Hring Launcher automatically creates configuration files in `~/.config/hring/` when you first launch it. You can edit them afterward. 

You can also assign hotkeys without touching `binds.toml`:

- **Right-click an application in the left panel** to add it to a group. A small box appears and asks for two keys — press the **group key** first (an existing group is selected, or a new one is created) and then the **application key**.
- **Right-click an application on the graph** to rewrite the key that launches it.
- **Middle-click an application on the graph** to delete that shortcut after confirming with `Enter`.

Press `Esc` to cancel any of these prompts. `binds.toml` is updated automatically, and a group that is left without applications is removed.

The window has two pages, selected with the tabs at the top: **Keyboard** (the launcher graph) and **All Programs** (the searchable application list). You can also switch with `Ctrl+H` for the keyboard page and `Ctrl+L` for the application list.

On **Keyboard**, application nodes and group buttons grow smoothly as the pointer approaches. The application area includes its icon and the full rotated title background. Entering either area highlights the node using the current theme and enlarges it to its maximum size; moving within that area keeps the size constant. Icons, shortcut badges, and application labels scale together. Moving away restores the size and color, and mouse actions follow the enlarged icon and title areas.

**All Programs** groups applications by their initial from **A** to **Z**, with a letter heading and divider above each group. Names starting with other characters appear in a final **#** group. Search results keep the same grouping.

The initial application list starts at the top. Centered positioning is used after you click or type a group letter.

Click a letter in the vertical Dock on the left (or a group heading) to scroll smoothly and center that group in the application viewport. All letters fit within the screen without an index scrollbar. Nearby buttons and letters grow smoothly as the pointer approaches, while the other letters share the remaining space. The group is briefly highlighted while the other groups dim, then the grid returns to its normal appearance. Letters without matching applications are disabled.

Letter rows meet without gaps. The row under the pointer is highlighted across its entire click area so the next click's target is clear.

Outside search mode, pressing any **A–Z** key jumps to its group with the same highlight. Use the arrow keys to move the application selection, **PageUp/PageDown** (or **Shift+Up/Down**) to scroll, and **Enter** to launch. Click the search field or press **/** to search; letters typed in search mode only update the filter. **Escape** leaves search mode.

The **All Programs** grid can be tuned in the `[graphic.all_programs]` table of `graphic.toml` (card size, padding, gaps, icon and font scale, grid width, ...). Every key is optional, so you can add one at a time and keep adjusting.

If the application doesn't launch after editing the configuration files, the error is most likely due to incorrect formatting. Try deleting them and letting the launcher create new ones.

### Examples

Various graphics configuration templates [are available here.](examples/) 

*You can submit your graphics.toml to the [discussion forum](https://github.com/Xhelgi/hring/discussions) or to xhelgi@proton.me; they may be included in the repository.*

## Installation

### Prerequisites:

- `Rust` (latest stable)
- `Linux` with `Wayland` and any compositor providing `xdg-shell` (niri, sway, Hyprland, ...)
- A working Vulkan driver (the `wgpu` renderer probes Vulkan by default)

### Build from source
``` Bash
git clone https://github.com/Xhelgi/hring
cd hring
cargo build --release
# The binary will be available at target/release/hring
```

The default build opens a fullscreen, undecorated `xdg-shell` toplevel. For a
compositor-native `wlr-layer-shell` overlay instead, build with the
`layer-shell` feature:

``` Bash
cargo build --release --features layer-shell
```

Presentation goes through `wgpu` + `egui-wgpu` on the surface's swapchain. To
keep startup fast the GPU build only probes the **Vulkan** backend by default,
which skips the slow GL/EGL enumeration. Set `HRING_GPU_BACKEND=all` (or `gl`)
to use other backends; if Vulkan yields no adapter the `all` backends are tried
automatically. Run with `HRING_TRACE=1` to print the per-phase wgpu init
timings and the chosen adapter.

### Build an Arch package

`PKGBUILD` contains a `pkgver()` function that derives the version from git
(`git describe`, or commit count + short hash when there are no tags). Every new
commit therefore gets a unique version, and `makepkg -si` rebuilds instead of
reinstalling a stale package.

``` Bash
makepkg -si     # rebuild + install (skips when the current commit is already built)
makepkg -sif    # force a rebuild, e.g. after uncommitted local changes
```

### Setup Execution
To run `hring` from anywhere, copy the binary to your local bin directory:
``` Bash
cp target/release/hring ~/.local/bin/
```
**Note:** Ensure that `~/.local/bin` is in your environment's `$PATH`. If it's not, add the following line to your `~/.bashrc` or `~/.zshrc` configuration file:
```
export PATH="$HOME/.local/bin:$PATH"
```

### If you are using `i3`

Add **your path** to the executable (you can skip the steps of adding `.local/bin` to `$PATH`) and the desired launch buttons.

1. Open `~/.config/i3/config`
2. Add line `bindsym $mod+d exec --no-startup-id /home/yuki/.local/bin/hring`

## Contributing
I'm always happy if you decide to help develop the Hring. See the [Contributing](CONTRIBUTING.md).

## Feedback & Issueus
[Click Me!](docs/FeedbackIssues.md)

## Build With
- `Rust`
- `egui`
- `smithay-client-toolkit`
- `wgpu` + `egui-wgpu`
- `serde`
- `toml`
- `bincode`
- `homedir`
- `freedesktop_entry_parser`
- `image`
- `resvg`

*Thanks to the creators of these crates for the excellent functionality and documentation.*

## License
Distributed under the GPL-3.0 License. See [LICENSE](LICENSE) for more information.
