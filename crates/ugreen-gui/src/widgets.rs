//! Look and building blocks of the GUI: colours, cards, rows, switches.
//!
//! Everything here is drawn with the egui painter, so the app needs no image
//! or icon files.

use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Shape,
    Stroke, StrokeKind, Theme, Ui, pos2, vec2,
};

/// Colours of one theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub card: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    /// Pale accent, behind a selected round button.
    pub accent_soft: Color32,
    pub danger: Color32,
    pub line: Color32,
    /// Unselected switch, round button and slider rail.
    pub track: Color32,
}

impl Palette {
    pub fn of(theme: Theme) -> Self {
        match theme {
            Theme::Light => Self {
                background: Color32::from_rgb(0xF0, 0xF1, 0xF5),
                card: Color32::WHITE,
                text: Color32::from_rgb(0x22, 0x24, 0x28),
                weak: Color32::from_rgb(0x85, 0x88, 0x8E),
                accent: Color32::from_rgb(0x1A, 0xB3, 0x4B),
                accent_soft: Color32::from_rgb(0xE1, 0xF5, 0xE8),
                danger: Color32::from_rgb(0xD9, 0x55, 0x55),
                line: Color32::from_rgb(0xEC, 0xED, 0xF0),
                track: Color32::from_rgb(0xE4, 0xE5, 0xEA),
            },
            Theme::Dark => Self {
                background: Color32::from_rgb(0x11, 0x12, 0x14),
                card: Color32::from_rgb(0x1E, 0x20, 0x24),
                text: Color32::from_rgb(0xEC, 0xED, 0xEF),
                weak: Color32::from_rgb(0x90, 0x93, 0x99),
                accent: Color32::from_rgb(0x2B, 0xC4, 0x62),
                accent_soft: Color32::from_rgb(0x1C, 0x36, 0x27),
                danger: Color32::from_rgb(0xEE, 0x70, 0x70),
                line: Color32::from_rgb(0x2C, 0x2F, 0x34),
                track: Color32::from_rgb(0x33, 0x36, 0x3C),
            },
        }
    }

    /// The palette of the theme in use, dimmed where `ui` is disabled.
    pub fn for_ui(ui: &Ui) -> Self {
        let palette = Self::of(ui.ctx().theme());
        if ui.is_enabled() {
            return palette;
        }
        let dim = |color: Color32| color.gamma_multiply(0.45);
        Self {
            text: dim(palette.text),
            weak: dim(palette.weak),
            accent: dim(palette.accent),
            danger: dim(palette.danger),
            ..palette
        }
    }
}

/// Registers both themes' colours and the text sizes. Call once at start-up.
pub fn install(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        let palette = Palette::of(theme);
        let mut visuals = match theme {
            Theme::Light => egui::Visuals::light(),
            Theme::Dark => egui::Visuals::dark(),
        };
        visuals.panel_fill = palette.background;
        visuals.window_fill = palette.card;
        visuals.window_stroke = Stroke::NONE;
        visuals.window_corner_radius = CornerRadius::same(18);
        visuals.extreme_bg_color = palette.track;
        visuals.override_text_color = Some(palette.text);
        visuals.selection.bg_fill = palette.accent;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.hyperlink_color = palette.accent;
        visuals.slider_trailing_fill = true;
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.corner_radius = CornerRadius::same(12);
        }
        visuals.widgets.inactive.bg_fill = palette.track;
        visuals.widgets.inactive.weak_bg_fill = palette.track;
        ctx.set_visuals_of(theme, visuals);
    }
    ctx.all_styles_mut(|style| {
        use egui::TextStyle;
        for (text_style, size) in [
            (TextStyle::Small, 12.5),
            (TextStyle::Body, 15.0),
            (TextStyle::Button, 15.0),
            (TextStyle::Heading, 24.0),
        ] {
            if let Some(font) = style.text_styles.get_mut(&text_style) {
                font.size = size;
            }
        }
        style.spacing.item_spacing = vec2(10.0, 6.0);
        style.spacing.button_padding = vec2(14.0, 8.0);
        style.spacing.slider_width = 220.0;
        // A slim bar in its own gutter, so it never covers the cards.
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 4.0,
            bar_inner_margin: 6.0,
            ..egui::style::ScrollStyle::solid()
        };
    });
}

fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

/// A rounded panel, as wide as the space available.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let palette = Palette::for_ui(ui);
    egui::Frame::new()
        .fill(palette.card)
        .corner_radius(16)
        .inner_margin(egui::Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A hairline between two rows of a card.
pub fn divider(ui: &mut Ui) {
    let palette = Palette::for_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, palette.line),
    );
}

fn chevron(ui: &Ui, tip: Pos2, pointing_right: bool, color: Color32) {
    let reach = if pointing_right { -6.0 } else { 6.0 };
    let points = vec![
        pos2(tip.x + reach, tip.y - 7.0),
        tip,
        pos2(tip.x + reach, tip.y + 7.0),
    ];
    ui.painter().line(points, Stroke::new(1.8, color));
}

fn clickable(ui: &Ui, response: Response) -> Response {
    if ui.is_enabled() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Top bar of a screen: back arrow, centred title, optional text action on
/// the right. Returns `(back clicked, action clicked)`.
pub fn header(ui: &mut Ui, title: &str, action: Option<&str>) -> (bool, bool) {
    let palette = Palette::for_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        font(18.0),
        palette.text,
    );
    let back_rect = Rect::from_min_size(rect.min, vec2(40.0, 40.0));
    let back = clickable(
        ui,
        ui.interact(back_rect, ui.id().with("back"), Sense::click()),
    );
    chevron(
        ui,
        pos2(back_rect.left() + 8.0, back_rect.center().y),
        false,
        palette.text,
    );
    let mut action_clicked = false;
    if let Some(action) = action {
        let galley = ui
            .painter()
            .layout_no_wrap(action.to_owned(), font(14.5), palette.accent);
        let size = galley.size();
        let action_rect = Rect::from_min_size(
            pos2(rect.right() - size.x, rect.center().y - size.y / 2.0),
            size,
        );
        let response = clickable(
            ui,
            ui.interact(
                action_rect.expand(8.0),
                ui.id().with("header_action"),
                Sense::click(),
            ),
        );
        ui.painter().galley(action_rect.min, galley, palette.accent);
        action_clicked = response.clicked();
    }
    ui.add_space(6.0);
    (back.clicked(), action_clicked)
}

/// A row that leads somewhere or shows a value: label on the left, optional
/// value and chevron on the right.
pub fn nav_row(ui: &mut Ui, label: &str, value: Option<&str>, leads_somewhere: bool) -> Response {
    let palette = Palette::for_ui(ui);
    let sense = if leads_somewhere {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), sense);
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        font(16.0),
        palette.text,
    );
    let mut right = rect.right();
    if leads_somewhere {
        chevron(ui, pos2(right - 2.0, rect.center().y), true, palette.weak);
        right -= 20.0;
    }
    if let Some(value) = value {
        ui.painter().text(
            pos2(right, rect.center().y),
            Align2::RIGHT_CENTER,
            value,
            font(14.5),
            palette.weak,
        );
    }
    if leads_somewhere {
        clickable(ui, response)
    } else {
        response
    }
}

/// A half-width card with a title, the current value under it and a chevron.
pub fn tile(ui: &mut Ui, title: &str, value: &str) -> Response {
    card(ui, |ui| {
        let palette = Palette::for_ui(ui);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
        ui.painter().text(
            pos2(rect.left(), rect.center().y - 10.0),
            Align2::LEFT_CENTER,
            title,
            font(16.0),
            palette.text,
        );
        ui.painter().text(
            pos2(rect.left(), rect.center().y + 12.0),
            Align2::LEFT_CENTER,
            value,
            font(13.5),
            palette.weak,
        );
        chevron(
            ui,
            pos2(rect.right() - 2.0, rect.center().y - 10.0),
            true,
            palette.weak,
        );
        clickable(ui, response)
    })
}

/// The round mark at the right of a choice: green with a tick when selected.
fn check_mark(ui: &Ui, center: Pos2, selected: bool, palette: Palette) {
    let painter = ui.painter();
    if selected {
        painter.circle_filled(center, 11.0, palette.accent);
        let tick = vec![
            pos2(center.x - 5.0, center.y),
            pos2(center.x - 1.5, center.y + 3.5),
            pos2(center.x + 5.0, center.y - 3.5),
        ];
        painter.line(tick, Stroke::new(2.0, Color32::WHITE));
    } else {
        painter.circle_stroke(
            center,
            10.5,
            Stroke::new(1.2, palette.weak.gamma_multiply(0.6)),
        );
    }
}

/// One choice of a list: title, optional description, round mark.
pub fn option_row(ui: &mut Ui, title: &str, description: Option<&str>, selected: bool) -> Response {
    let palette = Palette::for_ui(ui);
    let height = if description.is_some() { 60.0 } else { 48.0 };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let title_y = if description.is_some() {
        rect.center().y - 11.0
    } else {
        rect.center().y
    };
    ui.painter().text(
        pos2(rect.left(), title_y),
        Align2::LEFT_CENTER,
        title,
        font(16.0),
        palette.text,
    );
    if let Some(description) = description {
        ui.painter().text(
            pos2(rect.left(), rect.center().y + 12.0),
            Align2::LEFT_CENTER,
            description,
            font(13.0),
            palette.weak,
        );
    }
    check_mark(
        ui,
        pos2(rect.right() - 12.0, rect.center().y),
        selected,
        palette,
    );
    clickable(ui, response)
}

/// A labelled on/off switch. The response is `clicked()` when the user
/// flips it; the caller decides what the new value is.
pub fn switch_row(ui: &mut Ui, label: &str, on: bool) -> Response {
    let palette = Palette::for_ui(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        font(16.0),
        palette.text,
    );
    let size = vec2(46.0, 26.0);
    let switch = Rect::from_min_size(
        pos2(rect.right() - size.x, rect.center().y - size.y / 2.0),
        size,
    );
    let how_on = ui.ctx().animate_bool(response.id, on);
    let fill = if on { palette.accent } else { palette.track };
    ui.painter().rect_filled(switch, 13, fill);
    let knob_x = egui::lerp(switch.left() + 13.0..=switch.right() - 13.0, how_on);
    ui.painter()
        .circle_filled(pos2(knob_x, switch.center().y), 10.5, Color32::WHITE);
    clickable(ui, response)
}

/// What the round noise-cancelling buttons show.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoiseIcon {
    On,
    Off,
    Ambient,
}

/// A round icon button with a caption under it.
pub fn round_choice(ui: &mut Ui, label: &str, icon: NoiseIcon, selected: bool) -> Response {
    let palette = Palette::for_ui(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::click());
    let center = pos2(rect.center().x, rect.top() + 32.0);
    let painter = ui.painter();
    let (fill, ink) = if selected {
        (palette.accent_soft, palette.accent)
    } else {
        (palette.track.gamma_multiply(0.7), palette.weak)
    };
    painter.circle_filled(center, 28.0, fill);
    let stroke = Stroke::new(1.8, ink);
    match icon {
        NoiseIcon::On => {
            painter.circle_stroke(center, 12.0, stroke);
            painter.circle_filled(center, 4.0, ink);
        }
        NoiseIcon::Off => {
            painter.circle_stroke(center, 12.0, stroke);
            painter.line_segment(
                [
                    pos2(center.x - 8.5, center.y + 8.5),
                    pos2(center.x + 8.5, center.y - 8.5),
                ],
                stroke,
            );
        }
        NoiseIcon::Ambient => {
            painter.circle_filled(center, 4.0, ink);
            for step in 0..10 {
                let angle = step as f32 * PI / 5.0;
                let dot = pos2(center.x + 12.0 * angle.cos(), center.y + 12.0 * angle.sin());
                painter.circle_filled(dot, 1.4, ink);
            }
        }
    }
    painter.text(
        pos2(rect.center().x, rect.bottom() - 14.0),
        Align2::CENTER_CENTER,
        label,
        font(14.5),
        if selected { palette.text } else { palette.weak },
    );
    clickable(ui, response)
}

/// A battery glyph with the level next to it, centred.
pub fn battery(ui: &mut Ui, level: Option<u8>) {
    let palette = Palette::for_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    let Some(level) = level else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Battery not reported",
            font(15.0),
            palette.weak,
        );
        return;
    };
    let body = Rect::from_center_size(
        pos2(rect.center().x - 24.0, rect.center().y),
        vec2(28.0, 14.0),
    );
    let painter = ui.painter();
    painter.rect_stroke(body, 4, Stroke::new(1.3, palette.weak), StrokeKind::Outside);
    painter.rect_filled(
        Rect::from_min_size(
            pos2(body.right() + 2.5, body.center().y - 3.0),
            vec2(2.0, 6.0),
        ),
        1,
        palette.weak,
    );
    let inner = body.shrink(2.0);
    let width = inner.width() * f32::from(level.min(100)) / 100.0;
    let fill = if level <= 20 {
        palette.danger
    } else {
        palette.accent
    };
    painter.rect_filled(
        Rect::from_min_size(inner.min, vec2(width, inner.height())),
        2,
        fill,
    );
    painter.text(
        pos2(body.right() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{level}%"),
        font(15.0),
        palette.text,
    );
}

/// A simple drawing of over-ear headphones, centred.
pub fn headphones(ui: &mut Ui) {
    let palette = Palette::for_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::hover());
    let center = pos2(rect.center().x, rect.top() + 70.0);
    let radius = 46.0;
    let band: Vec<Pos2> = (0..=32)
        .map(|step| {
            let angle = PI + PI * step as f32 / 32.0;
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();
    let band_color = palette.weak.gamma_multiply(0.55);
    ui.painter()
        .add(Shape::line(band, Stroke::new(9.0, band_color)));
    for side in [-1.0, 1.0] {
        let cup = Rect::from_center_size(
            pos2(center.x + side * radius, center.y + 22.0),
            vec2(30.0, 56.0),
        );
        ui.painter().rect_filled(cup, 14, palette.card);
        ui.painter()
            .rect_stroke(cup, 14, Stroke::new(2.0, band_color), StrokeKind::Inside);
        ui.painter().circle_filled(
            pos2(cup.center().x, cup.bottom() - 12.0),
            2.5,
            palette.accent,
        );
    }
}

/// Small grey explanations under a card, one bullet each.
pub fn notes(ui: &mut Ui, lines: &[&str]) {
    let palette = Palette::for_ui(ui);
    ui.add_space(4.0);
    for line in lines {
        ui.horizontal_top(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("•").color(palette.weak));
            ui.add(egui::Label::new(RichText::new(*line).color(palette.weak).size(13.5)).wrap());
        });
    }
}

/// Title of a card section.
pub fn card_title(ui: &mut Ui, title: &str) {
    let palette = Palette::for_ui(ui);
    ui.add_space(8.0);
    ui.label(RichText::new(title).size(16.5).color(palette.text));
    ui.add_space(2.0);
}

/// A wide pill button with centred text: `ink` on `fill`.
pub fn pill_button(ui: &mut Ui, text: &str, ink: Color32, fill: Color32, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 46.0), Sense::click());
    let fill = if response.hovered() && ui.is_enabled() {
        fill.gamma_multiply(0.9)
    } else {
        fill
    };
    ui.painter().rect_filled(rect, 23, fill);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, text, font(16.0), ink);
    clickable(ui, response)
}

/// One tab of the bottom bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabIcon {
    Device,
    More,
}

pub fn tab_button(ui: &mut Ui, label: &str, icon: TabIcon, selected: bool) -> Response {
    let palette = Palette::for_ui(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
    let ink = if selected { palette.text } else { palette.weak };
    let center = pos2(rect.center().x, rect.top() + 16.0);
    let painter = ui.painter();
    match icon {
        TabIcon::Device => {
            let body = Rect::from_center_size(center, vec2(22.0, 16.0));
            if selected {
                painter.rect_filled(body, 5, ink);
                painter.hline(
                    center.x - 4.0..=center.x + 4.0,
                    center.y + 2.0,
                    Stroke::new(2.0, palette.accent),
                );
            } else {
                painter.rect_stroke(body, 5, Stroke::new(1.6, ink), StrokeKind::Inside);
            }
        }
        TabIcon::More => {
            for (dx, dy) in [(-5.5, -5.5), (5.5, -5.5), (-5.5, 5.5), (5.5, 5.5)] {
                let square =
                    Rect::from_center_size(pos2(center.x + dx, center.y + dy), vec2(8.0, 8.0));
                let accent_corner = dx > 0.0 && dy < 0.0;
                if selected {
                    let fill = if accent_corner { palette.accent } else { ink };
                    painter.rect_filled(square, 2, fill);
                } else {
                    painter.rect_stroke(square, 2, Stroke::new(1.4, ink), StrokeKind::Inside);
                }
            }
        }
    }
    painter.text(
        pos2(rect.center().x, rect.bottom() - 10.0),
        Align2::CENTER_CENTER,
        label,
        font(13.0),
        ink,
    );
    clickable(ui, response)
}
