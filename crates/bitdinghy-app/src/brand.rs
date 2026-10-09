//! BitDinghy's look: the brand colors, the logo and the window icon.
//!
//! The pictures come from the `assets` folder at the top of the project. The small logo files in
//! `crates/bitdinghy-app/assets` are made from them by `tools/make-app-images.ps1`. They are
//! compiled into the program, so there are no extra files to carry around.
//!
//! The colors are the brand's own design tokens from `assets/themes/themes.css` (the names match),
//! and a test checks them against `assets/brand/colors.json` so the two can't drift apart.

use std::io::Cursor;

use eframe::egui::{self, Color32, Stroke, Theme};

/// The colors of one theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub border: Color32,
    pub link: Color32,
    /// The brand green used for buttons and progress.
    pub accent: Color32,
    /// Text drawn on top of `accent`.
    pub accent_foreground: Color32,
    pub success: Color32,
    pub info: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The dark theme (the brand's main look).
pub const DARK: Palette = Palette {
    background: rgb(0x071A2B),
    surface: rgb(0x1E3146),
    surface_raised: rgb(0x253C50),
    text: rgb(0xF7FAFC),
    text_muted: rgb(0xD6D9DE),
    border: rgb(0x35516A),
    link: rgb(0xB6FFC7),
    accent: rgb(0x57F000),
    accent_foreground: rgb(0x071A2B),
    success: rgb(0x57F000),
    info: rgb(0x38C7A3),
    warning: rgb(0xFFE84A),
    danger: rgb(0xFF373E),
};

/// The light theme. It uses the darker Deep Green, because the bright green is hard to read on
/// white.
pub const LIGHT: Palette = Palette {
    background: rgb(0xF5F7F9),
    surface: rgb(0xFFFFFF),
    surface_raised: rgb(0xEAF0F3),
    text: rgb(0x071A2B),
    text_muted: rgb(0x465C6E),
    border: rgb(0xC7D2DC),
    link: rgb(0x136A4D),
    accent: rgb(0x1F8F46),
    accent_foreground: rgb(0xFFFFFF),
    success: rgb(0x17733B),
    info: rgb(0x126A63),
    warning: rgb(0x956200),
    danger: rgb(0xB3262C),
};

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => DARK,
        Theme::Light => LIGHT,
    }
}

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
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
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

/// Gives the window the brand look, in both the dark and the light theme. The window follows the
/// computer's own light/dark setting.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals_for(Theme::Dark));
    ctx.set_visuals_of(Theme::Light, visuals_for(Theme::Light));
}

fn visuals_for(theme: Theme) -> egui::Visuals {
    let p = palette(theme);
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    visuals.panel_fill = p.background;
    visuals.window_fill = p.surface;
    visuals.faint_bg_color = p.surface_raised;
    // The inside of text boxes: a touch darker than the page in the dark theme, white in the light.
    visuals.extreme_bg_color = match theme {
        Theme::Dark => rgb(0x04111D),
        Theme::Light => p.surface,
    };
    visuals.hyperlink_color = p.link;
    // Secondary text (hints, the version number) is the brand's muted gray.
    visuals.weak_text_color = Some(p.text_muted);
    visuals.selection.bg_fill = match theme {
        Theme::Dark => rgb(0x1F8F46),
        Theme::Light => rgb(0xB6FFC7),
    };
    visuals.selection.stroke = Stroke::new(1.0, p.text);

    let line = Stroke::new(1.0, p.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    visuals.widgets.noninteractive.bg_stroke = line;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    visuals.widgets.inactive.bg_fill = p.surface;
    visuals.widgets.inactive.weak_bg_fill = p.surface;
    visuals.widgets.inactive.bg_stroke = line;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    visuals.widgets.hovered.weak_bg_fill = p.surface_raised;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.accent_foreground);
    visuals.widgets.active.weak_bg_fill = p.accent;
    visuals.widgets.active.bg_fill = p.accent;
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads `#RRGGBB` for `key` out of the brand pack's `colors.json`.
    fn color_in_pack(key: &str) -> Color32 {
        let json = include_str!("../../../assets/brand/colors.json");
        let after_key = json
            .split(&format!("\"{key}\""))
            .nth(1)
            .unwrap_or_else(|| panic!("{key} is not in colors.json"));
        let hex = after_key
            .split('"')
            .nth(1) // the opening quote of the value is the first quote after the key
            .unwrap_or_else(|| panic!("{key} has no value"))
            .trim_start_matches('#');
        // `split('"')` on `: "#071A2B", ...` gives ["...: ", "#071A2B", ...], so nth(1) is the value.
        rgb(u32::from_str_radix(hex, 16).expect("a hex color"))
    }

    #[test]
    fn the_colors_in_the_code_match_the_brand_pack() {
        assert_eq!(DARK.background, color_in_pack("background"));
        assert_eq!(DARK.surface, color_in_pack("surface"));
        assert_eq!(DARK.text_muted, color_in_pack("softGray"));
        assert_eq!(DARK.accent, color_in_pack("primaryGreen"));
        assert_eq!(DARK.link, color_in_pack("mint"));
        assert_eq!(DARK.info, color_in_pack("accentTeal"));
        // The light theme uses the darker green for buttons and text.
        assert_eq!(LIGHT.accent, color_in_pack("deepGreen"));
        assert_eq!(LIGHT.text, color_in_pack("background"));
    }

    #[test]
    fn the_window_icon_is_a_square_256_picture() {
        let icon = window_icon().expect("the icon should decode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // It is not blank: some pixels are visible.
        assert!(
            icon.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0)
        );
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
