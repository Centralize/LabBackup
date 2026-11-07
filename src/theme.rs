//! Colours and widget styling shared by every screen.

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(56, 152, 236);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(34, 116, 196);
pub const OK: Color32 = Color32::from_rgb(72, 187, 120);
pub const WARN: Color32 = Color32::from_rgb(236, 170, 60);
pub const ERR: Color32 = Color32::from_rgb(232, 92, 92);

pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: Color32::from_rgb(20, 23, 28),
            panel: Color32::from_rgb(26, 30, 36),
            card: Color32::from_rgb(33, 38, 46),
            border: Color32::from_rgb(48, 55, 66),
            text: Color32::from_rgb(226, 230, 236),
            muted: Color32::from_rgb(140, 150, 164),
        }
    } else {
        Palette {
            bg: Color32::from_rgb(244, 246, 249),
            panel: Color32::from_rgb(234, 238, 243),
            card: Color32::WHITE,
            border: Color32::from_rgb(214, 220, 228),
            text: Color32::from_rgb(28, 33, 40),
            muted: Color32::from_rgb(100, 110, 124),
        }
    }
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = if dark { Color32::from_rgb(17, 20, 24) } else { Color32::from_rgb(250, 251, 253) };
    v.faint_bg_color = if dark { Color32::from_rgb(29, 34, 41) } else { Color32::from_rgb(240, 243, 247) };
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = ACCENT_DARK;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(6);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.weak_bg_fill = if dark { Color32::from_rgb(44, 50, 60) } else { Color32::from_rgb(226, 231, 238) };
    v.widgets.inactive.bg_fill = v.widgets.inactive.weak_bg_fill;
    v.widgets.hovered.weak_bg_fill = if dark { Color32::from_rgb(54, 62, 74) } else { Color32::from_rgb(212, 219, 229) };
    v.widgets.hovered.bg_fill = v.widgets.hovered.weak_bg_fill;
    v.window_corner_radius = CornerRadius::same(10);
    // Pin the theme so the operating system's light/dark setting doesn't override ours.
    ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    });
}

pub fn card(dark: bool) -> egui::Frame {
    let p = palette(dark);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
}

pub fn primary_button<'a>(text: &'a str) -> egui::Button<'a> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong().size(15.0))
        .fill(ACCENT_DARK)
        .min_size(egui::vec2(150.0, 34.0))
}
