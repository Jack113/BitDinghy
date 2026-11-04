//! The window's building blocks in the brand style: cards, status badges, the sidebar's page
//! buttons, the green main button, quiet buttons and notice banners.
//!
//! They only draw; what a click does is decided in `main.rs`. All colors come from
//! [`crate::brand::palette`], so they follow the dark or light look.

use eframe::egui::{self, Color32, RichText, Sense, Stroke};

use crate::brand;

/// The colors of the current look.
pub fn palette(ui: &egui::Ui) -> brand::Palette {
    brand::palette(ui.ctx().theme())
}

/// What a status badge or a banner means, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Teal: something is happening.
    Busy,
    /// Green: finished.
    Done,
    /// Amber: waiting, or something to know.
    Waiting,
    /// Red: a problem.
    Problem,
    /// Gray: nothing is happening (paused).
    Quiet,
}

impl Tone {
    pub fn color(self, palette: &brand::Palette) -> Color32 {
        match self {
            Tone::Busy => palette.info,
            Tone::Done => palette.success,
            Tone::Waiting => palette.warning,
            Tone::Problem => palette.danger,
            Tone::Quiet => palette.text_muted,
        }
    }
}

/// A rounded box with the raised background, for one download or one group of settings.
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let palette = palette(ui);
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// A card's title (semi-bold, in the normal text color: never egui's "strong" color, which the
/// brand makes dark navy).
pub fn card_title(ui: &mut egui::Ui, title: &str) {
    let color = palette(ui).text;
    ui.label(
        RichText::new(title)
            .font(brand::semibold(14.5))
            .color(color),
    );
}

/// A notice across the page with a colored stripe on its left (an offer, a crash report, a
/// question). The contents go beside the stripe.
pub fn banner<R>(
    ui: &mut egui::Ui,
    tone: Tone,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let palette = palette(ui);
    let response = egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(10)
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        });
    // The stripe, painted over the left edge of the box.
    let rect = response.response.rect;
    let stripe = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 4.0, rect.max.y));
    ui.painter().rect_filled(
        stripe,
        egui::CornerRadius {
            nw: 10,
            sw: 10,
            ne: 0,
            se: 0,
        },
        tone.color(&palette),
    );
    response.inner
}

/// A small rounded label with a colored outline, like "Downloading" or "Paused".
pub fn badge(ui: &mut egui::Ui, text: &str, tone: Tone) {
    let color = tone.color(&palette(ui));
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::FontId::proportional(11.5), color);
    let padding = egui::vec2(9.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, rect.height() / 2.0, color.gamma_multiply(0.12));
    painter.rect_stroke(
        rect,
        rect.height() / 2.0,
        Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + padding, galley, color);
}

/// The green main button (one per page: "Download", "Yes").
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let palette = palette(ui);
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(brand::semibold(13.0))
                .color(palette.accent_foreground),
        )
        .fill(palette.accent)
        .stroke(Stroke::new(1.0, palette.accent)),
    )
}

/// A normal button that stands out a little from the card behind it (Pause, Resume).
pub fn button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let fill = palette(ui).surface_raised;
    ui.add(egui::Button::new(text).fill(fill))
}

/// A quiet button: only an outline (Remove, Check files, Files).
pub fn quiet_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).fill(Color32::TRANSPARENT))
}

/// One page button in the sidebar: a symbol, the page's name and an optional count on the right.
/// The chosen page is drawn in green on a raised background.
pub fn nav_item(
    ui: &mut egui::Ui,
    selected: bool,
    symbol: &str,
    label: &str,
    count: Option<usize>,
) -> egui::Response {
    let palette = palette(ui);
    let size = egui::vec2(ui.available_width(), 34.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if selected || response.hovered() {
        let fill = if selected {
            palette.surface_raised
        } else {
            palette.surface
        };
        painter.rect_filled(rect, 8, fill);
    }
    let color = if selected {
        palette.accent
    } else {
        palette.text_muted
    };
    let middle = rect.center().y;
    painter.text(
        egui::pos2(rect.min.x + 12.0, middle),
        egui::Align2::LEFT_CENTER,
        symbol,
        egui::FontId::proportional(15.0),
        color,
    );
    let font = if selected {
        brand::semibold(13.5)
    } else {
        egui::FontId::proportional(13.5)
    };
    painter.text(
        egui::pos2(rect.min.x + 36.0, middle),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    if let Some(count) = count {
        painter.text(
            egui::pos2(rect.max.x - 10.0, middle),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(11.5),
            palette.text_muted,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The "busy" spinner, in the teal of the line beside it. Not `ui.spinner()`: egui paints that in
/// the pressed-button color, which the brand makes dark navy, so it vanished on the background.
pub fn spinner(ui: &mut egui::Ui) {
    let color = palette(ui).info;
    ui.add(egui::Spinner::new().color(color));
}
