use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, CornerRadius, FontId, Margin, Stroke, Style, TextStyle, Vec2, Visuals};

static LIGHT: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub panel_2: Color32,
    pub panel_3: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub fg: Color32,
    pub muted_fg: Color32,
    pub dim_fg: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub green: Color32,
    pub amber: Color32,
    pub red: Color32,
    pub purple: Color32,
    pub cyan: Color32,
    pub shadow: Color32,
}

impl Palette {
    const fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(11, 12, 14),
            panel: Color32::from_rgb(18, 20, 23),
            panel_2: Color32::from_rgb(25, 28, 32),
            panel_3: Color32::from_rgb(32, 35, 40),
            border: Color32::from_rgb(38, 42, 48),
            border_soft: Color32::from_rgb(29, 32, 37),
            fg: Color32::from_rgb(230, 233, 238),
            muted_fg: Color32::from_rgb(139, 146, 157),
            dim_fg: Color32::from_rgb(99, 106, 117),
            accent: Color32::from_rgb(76, 141, 255),
            accent_dim: Color32::from_rgb(36, 58, 92),
            green: Color32::from_rgb(61, 220, 151),
            amber: Color32::from_rgb(240, 180, 41),
            red: Color32::from_rgb(255, 93, 93),
            purple: Color32::from_rgb(167, 139, 250),
            cyan: Color32::from_rgb(56, 189, 248),
            shadow: Color32::from_black_alpha(160),
        }
    }

    const fn light() -> Self {
        Self {
            bg: Color32::from_rgb(246, 247, 249),
            panel: Color32::from_rgb(255, 255, 255),
            panel_2: Color32::from_rgb(242, 244, 247),
            panel_3: Color32::from_rgb(232, 236, 242),
            border: Color32::from_rgb(214, 220, 229),
            border_soft: Color32::from_rgb(231, 235, 241),
            fg: Color32::from_rgb(25, 30, 38),
            muted_fg: Color32::from_rgb(95, 107, 123),
            dim_fg: Color32::from_rgb(137, 147, 160),
            accent: Color32::from_rgb(37, 99, 235),
            accent_dim: Color32::from_rgb(219, 231, 255),
            green: Color32::from_rgb(15, 145, 100),
            amber: Color32::from_rgb(180, 120, 20),
            red: Color32::from_rgb(204, 60, 60),
            purple: Color32::from_rgb(118, 86, 210),
            cyan: Color32::from_rgb(14, 116, 144),
            shadow: Color32::from_black_alpha(40),
        }
    }
}

pub fn palette() -> Palette {
    if LIGHT.load(Ordering::Relaxed) {
        Palette::light()
    } else {
        Palette::dark()
    }
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

pub fn set_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
}

pub fn toggle() -> bool {
    let next = !is_light();
    set_light(next);
    next
}

pub fn bg() -> Color32 {
    palette().bg
}
pub fn panel() -> Color32 {
    palette().panel
}
pub fn panel_2() -> Color32 {
    palette().panel_2
}
pub fn panel_3() -> Color32 {
    palette().panel_3
}
pub fn border() -> Color32 {
    palette().border
}
pub fn border_soft() -> Color32 {
    palette().border_soft
}
pub fn fg() -> Color32 {
    palette().fg
}
pub fn muted_fg() -> Color32 {
    palette().muted_fg
}
pub fn dim_fg() -> Color32 {
    palette().dim_fg
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn green() -> Color32 {
    palette().green
}
pub fn amber() -> Color32 {
    palette().amber
}
pub fn red() -> Color32 {
    palette().red
}
pub fn purple() -> Color32 {
    palette().purple
}
pub fn cyan() -> Color32 {
    palette().cyan
}

pub const GAP: f32 = 8.0;
pub const PAD: f32 = 16.0;
pub const RADIUS: u8 = 12;

pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn apply(ctx: &egui::Context) {
    let p = palette();
    let mut style = Style::default();
    let mut visuals = if is_light() {
        Visuals::light()
    } else {
        Visuals::dark()
    };

    visuals.panel_fill = p.bg;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = p.panel_2;
    visuals.faint_bg_color = p.panel_2;
    visuals.code_bg_color = p.panel_2;
    visuals.override_text_color = Some(p.fg);
    visuals.hyperlink_color = p.accent;
    visuals.warn_fg_color = p.amber;
    visuals.error_fg_color = p.red;
    visuals.window_stroke = Stroke::new(1.0, p.border);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.selection.bg_fill = if is_light() {
        Color32::from_rgba_unmultiplied(37, 99, 235, 45)
    } else {
        Color32::from_rgba_unmultiplied(76, 141, 255, 70)
    };
    visuals.selection.stroke = Stroke::new(1.0, p.accent);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: p.shadow,
    };

    visuals.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: p.panel,
        weak_bg_fill: p.panel,
        bg_stroke: Stroke::new(1.0, p.border),
        corner_radius: CornerRadius::same(8),
        fg_stroke: Stroke::new(1.0, p.fg),
        expansion: 0.0,
    };
    visuals.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: p.panel_2,
        weak_bg_fill: p.panel_2,
        bg_stroke: Stroke::new(1.0, p.border),
        corner_radius: CornerRadius::same(6),
        fg_stroke: Stroke::new(1.0, p.fg),
        expansion: 0.0,
    };
    visuals.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: p.panel_3,
        weak_bg_fill: p.panel_3,
        bg_stroke: Stroke::new(1.0, p.border),
        corner_radius: CornerRadius::same(6),
        fg_stroke: Stroke::new(1.5, p.fg),
        expansion: 0.0,
    };
    visuals.widgets.active = egui::style::WidgetVisuals {
        bg_fill: p.accent_dim,
        weak_bg_fill: p.accent_dim,
        bg_stroke: Stroke::new(1.0, p.accent),
        corner_radius: CornerRadius::same(6),
        fg_stroke: Stroke::new(1.5, p.fg),
        expansion: 0.0,
    };
    visuals.widgets.open = visuals.widgets.active;

    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(9.0, 7.0);
    style.spacing.button_padding = Vec2::new(11.0, 5.0);
    style.spacing.interact_size = Vec2::new(0.0, 28.0);
    style.spacing.window_margin = Margin::same(0);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 10.0;
    style.spacing.scroll.floating = true;
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Heading, FontId::proportional(23.0)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
    ]
    .into();

    if is_light() {
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.set_style_of(egui::Theme::Light, style);
    } else {
        ctx.set_theme(egui::ThemePreference::Dark);
        ctx.set_style_of(egui::Theme::Dark, style);
    }
}

/// Frame used for panels and cards.
pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(palette().panel)
        .stroke(Stroke::new(1.0, palette().border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(PAD as i8))
}

pub fn inset_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(palette().panel_2)
        .stroke(Stroke::new(1.0, palette().border_soft))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
}

pub fn modal_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(palette().panel)
        .stroke(Stroke::new(1.0, palette().border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20))
        .shadow(egui::epaint::Shadow {
            offset: [0, 18],
            blur: 40,
            spread: 0,
            color: palette().shadow,
        })
}
