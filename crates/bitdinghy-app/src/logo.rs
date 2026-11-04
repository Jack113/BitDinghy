//! The animated logo at the top of the sidebar: the website's animated logo, redrawn by the
//! window. The boat rocks and bobs, the waves roll, the wind shimmers and ones and zeros blow
//! towards the sail, with "BitDinghy" beside it.
//!
//! egui can't play an SVG's own animation, so the drawing was cut into still pictures (boat, two
//! waves, wind, spray; made by `tools/make-logo-layers.ps1` from `tools/logo-layers/*.svg`) and
//! this file moves them the same way the website's SVG does. The digits and the name are drawn
//! here directly.
//!
//! It only moves while something is downloading (the owner's choice), and never when Windows'
//! "Show animations" is off. While still, it keeps the pose it stopped in, so it never jumps.

use std::f32::consts::PI;
use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

use crate::brand;

/// The part of the website drawing the pictures cover, in the drawing's own units.
const ART_LEFT: f32 = 30.0;
const ART_TOP: f32 = 60.0;
const ART_WIDTH: f32 = 750.0;
const ART_HEIGHT: f32 = 370.0;

/// How tall the boat drawing is, in points, and how big the name beside it is.
const HEIGHT: f32 = 40.0;
const NAME_SIZE: f32 = 17.0;

/// The point the boat rocks around (the middle of its hull), in drawing units.
const ROCK_CENTER: (f32, f32) = (520.0, 315.0);

/// About 30 pictures a second while it moves.
const FRAME: Duration = Duration::from_millis(33);

/// One group of ones and zeros: text, where it starts (drawing units), its size, its slant in
/// degrees, and how far into its 2-second trip it is at time 0 (the SVG's negative `begin`).
const DIGITS: [(&str, f32, f32, f32, f32, f32); 7] = [
    ("1  0  1", 174.0, 230.0, 25.0, -9.0, 0.2),
    ("0  1  0  1", 258.0, 199.0, 20.0, -9.0, 1.2),
    ("1", 182.0, 263.0, 37.0, -8.0, 0.9),
    ("0", 276.0, 248.0, 34.0, -8.0, 1.6),
    ("1", 365.0, 227.0, 29.0, -8.0, 0.5),
    ("0  1  0  1", 228.0, 292.0, 20.0, -6.0, 1.4),
    ("0", 408.0, 179.0, 27.0, -8.0, 0.1),
];

/// The little blocks that blow along with the digits: x, y, width, height (drawing units), color,
/// how opaque at most, and how far into the trip at time 0.
const BLOCKS: [(f32, f32, f32, f32, u32, f32, f32); 3] = [
    (303.0, 207.0, 18.0, 8.0, 0x38C7A3, 0.7, 0.8),
    (384.0, 257.0, 20.0, 12.0, 0x57F000, 1.0, 1.8),
    (124.0, 287.0, 17.0, 9.0, 0x38C7A3, 0.7, 0.3),
];

const DIGIT_COLOR: u32 = 0x7BFF3A;

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Goes smoothly 0 -> 1 -> 0 over one `period` (like the SVG's ease-in-out back and forth).
pub fn swing(time: f64, period: f64) -> f32 {
    let phase = (time / period).rem_euclid(1.0) as f32;
    (PI * phase).sin().powi(2)
}

/// How visible a blowing digit is at `phase` (0..1 of its trip): fades in over the first quarter,
/// stays, and fades out over the last 30%, like the SVG's keyTimes 0;.25;.7;1.
pub fn digit_opacity(phase: f32) -> f32 {
    if phase < 0.25 {
        phase / 0.25
    } else if phase < 0.7 {
        1.0
    } else {
        ((1.0 - phase) / 0.3).max(0.0)
    }
}

/// How far into its 2-second trip something is at `time`, starting `head_start` seconds in.
fn trip(time: f64, head_start: f32) -> f32 {
    ((time + f64::from(head_start)) / 2.0).rem_euclid(1.0) as f32
}

/// The logo's pictures, ready to draw.
pub struct AnimatedLogo {
    boat: egui::TextureHandle,
    wake_back: egui::TextureHandle,
    wake_front: egui::TextureHandle,
    wind: egui::TextureHandle,
    spray: egui::TextureHandle,
}

/// The logo's own clock: it only runs while the logo moves.
#[derive(Clone, Copy, Default)]
struct Clock {
    /// Seconds of movement so far.
    moved: f64,
    /// egui's time at the last picture.
    last: f64,
}

impl AnimatedLogo {
    /// Reads the pictures. `None` if one of them can't be read (the still logo is shown then).
    pub fn load(ctx: &egui::Context) -> Option<AnimatedLogo> {
        let texture = |name: &str, bytes: &[u8]| brand::texture_from_png(ctx, name, bytes);
        Some(AnimatedLogo {
            boat: texture("logo-boat", include_bytes!("../assets/logo/boat.png"))?,
            wake_back: texture(
                "logo-wake-back",
                include_bytes!("../assets/logo/wake-back.png"),
            )?,
            wake_front: texture(
                "logo-wake-front",
                include_bytes!("../assets/logo/wake-front.png"),
            )?,
            wind: texture("logo-wind", include_bytes!("../assets/logo/wind.png"))?,
            spray: texture("logo-spray", include_bytes!("../assets/logo/spray.png"))?,
        })
    }

    /// Draws the logo at the top left of `ui`. `downloading`: something is downloading, so the
    /// boat sails.
    pub fn show(&self, ui: &mut egui::Ui, downloading: bool) {
        let moving = downloading && windows_animations_on();
        let time = self.advance_clock(ui, moving);
        if moving {
            ui.ctx().request_repaint_after(FRAME);
        }

        // The name's two parts, to know how wide everything is.
        let palette = brand::palette(ui.ctx().theme());
        let name_galley = |size: f32| {
            let mut job = egui::text::LayoutJob::default();
            let font = brand::semibold(size);
            job.append(
                "Bit",
                0.0,
                egui::TextFormat::simple(font.clone(), palette.accent),
            );
            job.append("Dinghy", 0.0, egui::TextFormat::simple(font, palette.text));
            ui.painter().layout_job(job)
        };
        let art_width = HEIGHT * ART_WIDTH / ART_HEIGHT;
        let wanted = art_width + name_galley(NAME_SIZE).size().x;
        // Shrink the whole logo if the sidebar is narrower than it.
        let fit = (ui.available_width() / wanted).min(1.0);
        let height = HEIGHT * fit;
        let scale = height / ART_HEIGHT;
        let name = name_galley(NAME_SIZE * fit);
        let size = egui::vec2(ART_WIDTH * scale + name.size().x, height);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, dinghy_core::APP_NAME)
        });
        let art = Rect::from_min_size(rect.min, egui::vec2(ART_WIDTH * scale, height));
        // A point of the drawing on the screen.
        let at =
            |x: f32, y: f32| art.min + egui::vec2((x - ART_LEFT) * scale, (y - ART_TOP) * scale);
        let painter = ui.painter_at(rect.expand(4.0));
        let paint = |texture: &egui::TextureHandle, offset: Vec2, opacity: f32| {
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
                .tint(Color32::WHITE.gamma_multiply(opacity))
                .paint_at(ui, art.translate(offset * scale));
        };

        // The wind, then the digits and blocks blowing towards the sail.
        paint(&self.wind, Vec2::ZERO, 1.0 - 0.3 * swing(time, 2.0));
        for (text, x, y, size, slant, head_start) in DIGITS {
            let phase = trip(time, head_start);
            let angle = slant.to_radians();
            let along = egui::vec2(angle.cos(), angle.sin());
            let galley = painter.layout_no_wrap(
                text.to_string(),
                egui::FontId::monospace(size * scale),
                Color32::WHITE,
            );
            let color = rgb(DIGIT_COLOR).gamma_multiply(digit_opacity(phase));
            // The SVG places text by its baseline; egui by its top left corner.
            let start = at(x, y) + along * ((-45.0 + 90.0 * phase) * scale)
                - egui::vec2(-angle.sin(), angle.cos()) * (size * 0.8 * scale);
            painter.add(egui::epaint::TextShape::new(start, galley, color).with_angle(angle));
        }
        for (x, y, width, height, color, most, head_start) in BLOCKS {
            let phase = trip(time, head_start);
            let shift = egui::vec2(-45.0 + 90.0 * phase, 8.0 - 16.0 * phase) * scale;
            let block = Rect::from_min_size(at(x, y) + shift, egui::vec2(width, height) * scale);
            let color = rgb(color).gamma_multiply(most * digit_opacity(phase));
            painter.rect_filled(block, 2.0 * scale, color);
        }

        // The waves roll in time with the boat; the boat bobs and rocks.
        let roll = swing(time, 2.0);
        paint(&self.wake_back, egui::vec2(-9.0, 3.0) * roll, 1.0);
        let bob = egui::vec2(0.0, -5.0 * roll) * scale;
        let rock = (-1.6 + 3.2 * swing(time, 4.0)).to_radians();
        let center = egui::vec2(
            (ROCK_CENTER.0 - ART_LEFT) / ART_WIDTH,
            (ROCK_CENTER.1 - ART_TOP) / ART_HEIGHT,
        );
        egui::Image::from_texture(egui::load::SizedTexture::from_handle(&self.boat))
            .rotate(rock, center)
            .paint_at(ui, art.translate(bob));
        paint(&self.wake_front, egui::vec2(8.0, 2.0) * roll, 1.0);
        paint(&self.spray, Vec2::ZERO, 1.0 - 0.45 * swing(time + 0.5, 2.0));

        // The name, its baseline level with the hull (as on the website).
        // (Inter's letters reach about 0.97 of the font size above the baseline.)
        let baseline = at(0.0, 367.0).y;
        let name_top = baseline - 0.97 * NAME_SIZE * fit;
        let name_at = Pos2::new(art.max.x, name_top.max(rect.min.y));
        ui.painter().galley(name_at, name, palette.text);
    }

    /// Moves the logo's clock on while it moves, and returns its time in seconds.
    fn advance_clock(&self, ui: &egui::Ui, moving: bool) -> f64 {
        let now = ui.input(|input| input.time);
        let id = egui::Id::new("animated-logo-clock");
        ui.ctx().data_mut(|data| {
            let clock = data.get_temp_mut_or_default::<Clock>(id);
            if moving {
                // At most a tenth of a second at once, so a pause doesn't make it jump.
                clock.moved += (now - clock.last).clamp(0.0, 0.1);
            }
            clock.last = now;
            clock.moved
        })
    }
}

/// Whether Windows' "Show animations in Windows" is on (Settings > Accessibility > Visual
/// effects). People who turn it off get a still logo, like the website does for them.
fn windows_animations_on() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut on: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL (an i32) to the given pointer.
        let read = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut on as *mut i32).cast(),
                0,
            )
        };
        read == 0 || on != 0
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_swing_goes_there_and_back_smoothly() {
        assert!(swing(0.0, 2.0).abs() < 1e-6);
        assert!((swing(1.0, 2.0) - 1.0).abs() < 1e-6, "halfway: all the way");
        assert!(swing(2.0, 2.0).abs() < 1e-6, "one period: back again");
        assert!((swing(0.5, 2.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn digits_fade_in_stay_and_fade_out() {
        assert_eq!(digit_opacity(0.0), 0.0);
        assert!((digit_opacity(0.125) - 0.5).abs() < 1e-6);
        assert_eq!(digit_opacity(0.5), 1.0);
        assert!((digit_opacity(0.85) - 0.5).abs() < 1e-5);
        assert!(digit_opacity(1.0).abs() < 1e-6);
    }

    #[test]
    fn every_logo_picture_decodes_at_the_same_size() {
        for bytes in [
            &include_bytes!("../assets/logo/boat.png")[..],
            include_bytes!("../assets/logo/wake-back.png"),
            include_bytes!("../assets/logo/wake-front.png"),
            include_bytes!("../assets/logo/wind.png"),
            include_bytes!("../assets/logo/spray.png"),
        ] {
            let (width, height, pixels) = brand::decode_png_for_tests(bytes).unwrap();
            assert_eq!((width, height), (300, 148));
            // Transparent around the drawing, and something drawn.
            assert_eq!(pixels[3], 0, "the top left corner is see-through");
            assert!(pixels.chunks(4).any(|pixel| pixel[3] > 200));
        }
    }
}
