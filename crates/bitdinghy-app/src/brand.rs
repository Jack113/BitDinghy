//! BitDinghy's look: the brand colors, the logo and the window icon.
//!
//! The pictures come from the `assets` folder at the top of the project. The small logo files in
//! `crates/bitdinghy-app/assets` are made from them by `tools/make-app-images.ps1`. They are
//! compiled into the program, so there are no extra files to carry around.
//!
//! The colors are the ones printed on the brand palette sheet (`assets/brand/palette.png`).

use std::io::Cursor;

use eframe::egui::{self, Color32, Stroke, Theme};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x07, 0x1A, 0x2B);
pub const SURFACE: Color32 = Color32::from_rgb(0x1E, 0x31, 0x46);
pub const SOFT_GRAY: Color32 = Color32::from_rgb(0xD6, 0xD9, 0xDF);
pub const PRIMARY_GREEN: Color32 = Color32::from_rgb(0x57, 0xF0, 0x00);
pub const MINT: Color32 = Color32::from_rgb(0xB6, 0xFF, 0xC7);
pub const DEEP_GREEN: Color32 = Color32::from_rgb(0x1F, 0x8F, 0x46);
pub const ACCENT_TEAL: Color32 = Color32::from_rgb(0x38, 0xC7, 0xA3);

/// A line colour that sits quietly on the dark surfaces.
const DARK_LINE: Color32 = Color32::from_rgb(0x2C, 0x45, 0x5F);
const LIGHT_BACKGROUND: Color32 = Color32::from_rgb(0xF4, 0xF8, 0xFB);
const LIGHT_SURFACE: Color32 = Color32::from_rgb(0xE6, 0xEE, 0xF4);
const LIGHT_LINE: Color32 = Color32::from_rgb(0xC9, 0xD6, 0xE0);

const LOGO_ON_DARK: &[u8] = include_bytes!("../assets/logo-on-dark.png");
const LOGO_ON_LIGHT: &[u8] = include_bytes!("../assets/logo-on-light.png");
const WINDOW_ICON: &[u8] = include_bytes!("../../../assets/icons/windows-app-icon-256.png");

/// A picture as raw red-green-blue-alpha bytes.
struct Rgba {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

/// Reads a PNG picture into RGBA bytes. Returns `None` if it isn't a PNG we understand.
fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Turn palette and 16-bit pictures into plain 8-bit ones.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let data = &buffer[..info.buffer_size()];
    let pixels = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        // The decoder was told to expand palettes, so this does not happen.
        png::ColorType::Indexed => return None,
    };
    Some(Rgba {
        pixels,
        width: info.width,
        height: info.height,
    })
}

/// The picture shown in the title bar and the taskbar.
pub fn window_icon() -> Option<egui::IconData> {
    let icon = decode_png(WINDOW_ICON)?;
    Some(egui::IconData {
        rgba: icon.pixels,
        width: icon.width,
        height: icon.height,
    })
}

/// The two logo pictures, ready to draw: white lettering for the dark theme and navy lettering for
/// the light theme.
pub struct Logos {
    on_dark: egui::TextureHandle,
    on_light: egui::TextureHandle,
}

impl Logos {
    pub fn load(ctx: &egui::Context) -> Option<Logos> {
        let texture = |name: &str, bytes: &[u8]| {
            let picture = decode_png(bytes)?;
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [picture.width as usize, picture.height as usize],
                &picture.pixels,
            );
            Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
        };
        Some(Logos {
            on_dark: texture("logo-on-dark", LOGO_ON_DARK)?,
            on_light: texture("logo-on-light", LOGO_ON_LIGHT)?,
        })
    }

    /// The logo that is readable on the current theme's background.
    pub fn for_theme(&self, theme: Theme) -> &egui::TextureHandle {
        match theme {
            Theme::Dark => &self.on_dark,
            Theme::Light => &self.on_light,
        }
    }
}

/// The colour of the filled part of a progress bar.
pub fn progress_fill(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => PRIMARY_GREEN,
        Theme::Light => DEEP_GREEN,
    }
}

/// Gives the window the brand look, in both the dark and the light theme. The window follows the
/// computer's own light/dark setting.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, dark_visuals());
    ctx.set_visuals_of(Theme::Light, light_visuals());
}

fn dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(0x04, 0x11, 0x1D);
    visuals.hyperlink_color = ACCENT_TEAL;
    visuals.selection.bg_fill = DEEP_GREEN;
    visuals.selection.stroke = Stroke::new(1.0, MINT);

    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, SOFT_GRAY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DARK_LINE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, SOFT_GRAY);
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, DARK_LINE);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2A, 0x44, 0x5E);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = DEEP_GREEN;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals
}

fn light_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = LIGHT_BACKGROUND;
    visuals.window_fill = Color32::WHITE;
    visuals.faint_bg_color = LIGHT_SURFACE;
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.hyperlink_color = DEEP_GREEN;
    visuals.selection.bg_fill = MINT;
    visuals.selection.stroke = Stroke::new(1.0, DEEP_GREEN);

    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, BACKGROUND);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LIGHT_LINE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, BACKGROUND);
    visuals.widgets.inactive.weak_bg_fill = LIGHT_SURFACE;
    visuals.widgets.inactive.bg_fill = LIGHT_SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LIGHT_LINE);
    visuals.widgets.hovered.weak_bg_fill = MINT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, BACKGROUND);
    visuals.widgets.active.weak_bg_fill = DEEP_GREEN;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_icon_is_a_square_256_picture() {
        let icon = window_icon().expect("the icon should decode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // It is not blank: some pixels are visible.
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn both_logos_decode_with_the_same_wide_shape() {
        let dark = decode_png(LOGO_ON_DARK).expect("dark logo decodes");
        let light = decode_png(LOGO_ON_LIGHT).expect("light logo decodes");
        assert_eq!((dark.width, dark.height), (light.width, light.height));
        assert_eq!(dark.pixels.len(), (dark.width * dark.height * 4) as usize);
        let shape = dark.width as f32 / dark.height as f32;
        assert!((3.5..4.5).contains(&shape), "unexpected shape {shape}");
    }

    #[test]
    fn rubbish_is_not_mistaken_for_a_picture() {
        assert!(decode_png(b"this is not a picture").is_none());
        assert!(decode_png(&[]).is_none());
    }
}
