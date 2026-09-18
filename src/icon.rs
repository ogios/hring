// Copyright (c) 2026 Xhelgi
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use eframe::egui::ColorImage;

/// Size (in pixels) the icons are rasterized to before they are uploaded as textures.
const ICON_PIXEL_SIZE: u32 = 128;
/// Upper bound for a single icon file, so a broken desktop entry cannot make us read a huge file.
const MAX_ICON_FILE_BYTES: u64 = 8 * 1024 * 1024;

const SUPPORTED_EXTENSIONS: [&str; 3] = ["png", "svg", "xpm"];

/// Index of the icons installed on the system, keyed by the lowercased file stem.
///
/// Building it once is much cheaper than walking the icon directories for every
/// application, because a single icon theme easily contains tens of thousands of files.
pub struct IconIndex {
    icons: HashMap<String, PathBuf>,
}

impl IconIndex {
    pub fn build() -> Self {
        let mut icons = HashMap::new();

        for dir in icon_dirs() {
            if dir.is_dir() {
                index_dir(&dir, &mut icons);
            }
        }

        Self { icons }
    }

    /// Resolves a `Icon=` value from a `.desktop` file into a path on disk.
    ///
    /// The value can either be an absolute path or a name that is looked up in
    /// the icon index (the freedesktop.org icon naming scheme).
    pub fn resolve(&self, icon: &str) -> Option<PathBuf> {
        let icon = icon.trim();

        if icon.is_empty() {
            return None;
        }

        let direct_path = Path::new(icon);

        if direct_path.is_absolute() && direct_path.is_file() {
            return Some(direct_path.to_path_buf());
        }

        let key = Path::new(icon)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())?;

        self.icons.get(&key).cloned()
    }
}

fn icon_dirs() -> Vec<PathBuf> {
    let home = crate::config::get_home_dir();

    vec![
        home.join(".local/share/icons"),
        home.join(".icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/icons"),
        home.join(".local/share/pixmaps"),
        PathBuf::from("/usr/local/share/pixmaps"),
        PathBuf::from("/usr/share/pixmaps"),
    ]
}

fn index_dir(dir: &Path, icons: &mut HashMap<String, PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            let is_cursor_theme = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "cursors");

            if !is_cursor_theme {
                index_dir(&path, icons);
            }
        } else if let Some(stem) = icon_stem(&path) {
            let replace = match icons.get(&stem) {
                Some(existing) => icon_score(&path) > icon_score(existing),
                None => true,
            };

            if replace {
                icons.insert(stem, path);
            }
        }
    }
}

fn icon_stem(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_lowercase();

    if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }

    Some(path.file_stem()?.to_string_lossy().to_lowercase())
}

/// Ranks two icons with the same name so the best looking one wins.
///
/// Scalable icons beat every fixed size, larger fixed sizes beat smaller ones,
/// application icons beat the rest and monochrome "symbolic" icons are only a fallback.
fn icon_score(path: &Path) -> i32 {
    let path = path.to_string_lossy().to_lowercase();
    let mut score = 0;

    score += match Path::new(path.as_str())
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("svg") => 400,
        Some("png") => 300,
        Some("xpm") => 100,
        _ => 0,
    };

    score += icon_size_score(&path);

    if path.contains("/hicolor/") {
        score += 40;
    }
    if path.contains("/adwaita/") {
        score += 30;
    }
    if path.contains("/breeze") {
        score += 20;
    }
    if path.contains("/papirus/") {
        score += 15;
    }
    if path.contains("/apps/") || path.contains("/applications/") {
        score += 50;
    }
    if path.contains("symbolic") {
        score -= 60;
    }
    if path.contains("dark") {
        score -= 5;
    }

    score
}

fn icon_size_score(path: &str) -> i32 {
    let mut best = 0;

    for size in [512, 256, 128, 96, 64, 48, 32, 24, 22, 16] {
        if path.contains(&format!("{size}x{size}")) {
            best = best.max(size);
        }
    }

    if best == 0 && path.contains("scalable") {
        best = 512;
    }

    best / 4
}

/// Decodes an icon file into an egui image, rasterizing SVG icons on the fly.
pub fn load_color_image(path: &Path) -> Option<ColorImage> {
    let metadata = fs::metadata(path).ok()?;

    if metadata.len() == 0 || metadata.len() > MAX_ICON_FILE_BYTES {
        return None;
    }

    let data = fs::read(path).ok()?;

    if is_svg(path, &data) {
        rasterize_svg(&data, ICON_PIXEL_SIZE)
    } else {
        decode_raster(&data)
    }
}

fn is_svg(path: &Path, data: &[u8]) -> bool {
    let has_svg_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));

    has_svg_extension || data.starts_with(b"<svg") || data.starts_with(b"<?xml")
}

fn rasterize_svg(data: &[u8], size: u32) -> Option<ColorImage> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let tree_size = tree.size();

    if tree_size.width() <= 0.0 || tree_size.height() <= 0.0 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;

    let scale = (size as f32 / tree_size.width()).min(size as f32 / tree_size.height());
    let translate_x = (size as f32 - tree_size.width() * scale) / 2.0;
    let translate_y = (size as f32 - tree_size.height() * scale) / 2.0;

    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale)
        .post_translate(translate_x, translate_y);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(ColorImage::from_rgba_premultiplied(
        [size as usize, size as usize],
        pixmap.data(),
    ))
}

fn decode_raster(data: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(data).ok()?;

    // Keep large source icons from wasting texture memory; they are drawn much smaller anyway.
    let image = if image.width() > ICON_PIXEL_SIZE || image.height() > ICON_PIXEL_SIZE {
        image.thumbnail(ICON_PIXEL_SIZE, ICON_PIXEL_SIZE)
    } else {
        image
    };

    let image = image.to_rgba8();
    let (width, height) = image.dimensions();

    if width == 0 || height == 0 {
        return None;
    }

    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_stem_only_accepts_supported_extensions() {
        assert_eq!(
            icon_stem(Path::new("/icons/firefox.svg")).as_deref(),
            Some("firefox")
        );
        assert_eq!(
            icon_stem(Path::new("/icons/Firefox.PNG")).as_deref(),
            Some("firefox")
        );
        assert_eq!(icon_stem(Path::new("/icons/firefox.txt")), None);
    }

    #[test]
    fn scalable_icon_beats_small_raster() {
        let scalable = Path::new("/usr/share/icons/hicolor/scalable/apps/firefox.svg");
        let small = Path::new("/usr/share/icons/hicolor/16x16/apps/firefox.png");

        assert!(icon_score(scalable) > icon_score(small));
    }

    #[test]
    fn larger_raster_beats_smaller_one() {
        let large = Path::new("/usr/share/icons/hicolor/256x256/apps/firefox.png");
        let small = Path::new("/usr/share/icons/hicolor/16x16/apps/firefox.png");

        assert!(icon_score(large) > icon_score(small));
    }

    #[test]
    #[ignore = "depends on the icon theme installed on the host"]
    fn resolves_and_decodes_an_installed_icon() {
        let index = IconIndex::build();
        let path = index.resolve("firefox").expect("firefox icon is installed");
        let image = load_color_image(&path).expect("icon should decode");

        assert!(image.size[0] > 0 && image.size[1] > 0);
    }
}
