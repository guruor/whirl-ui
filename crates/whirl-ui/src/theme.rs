//! The window's visual language: the reference's palette, and the metrics read
//! from it.
//!
//! The reference is one image the operator supplied, and the palette below was
//! **sampled from its own pixels** with an ImageMagick histogram over the file
//! rather than read by eye or guessed: each constant carries the role it plays,
//! the share of the reference it holds and the hex the histogram measured. The
//! accent is an indigo and the canvas a navy, so this is nobody's stock dark
//! theme: substituting a nearer-tailwind colour is a different design, not this
//! one.
//!
//! The reference image itself is not in this repository and is not named by a
//! path here: what the repository keeps is the values it was sampled for, and
//! the tests below are what hold them.
//!
//! The metrics are the reference's own proportions, scaled to the window: the
//! sidebar is a little under a quarter of the window's width, the pane title is
//! the largest text on screen, and a control is 28 points tall. They are read
//! from the reference the same way the colours are, and they are values here
//! rather than numbers scattered through the drawing code so one change moves
//! all of them.
//!
//! [`apply`] installs the palette on a dark theme and nothing else: the light
//! theme is left exactly as egui ships it, because this window draws the dark
//! reference and keeps no second theme to maintain.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference,
};

// ---------------------------------------------------------------------------
// The palette. Every value came from the reference the operator supplied, by an
// ImageMagick histogram over its own pixels rather than from an eye or a guess.
// ---------------------------------------------------------------------------

/// The canvas: the window's background and the sidebar behind everything.
///
/// `#0A1627`, 979,189 px of the reference, its dominant colour.
pub const CANVAS: Color32 = Color32::from_rgb(0x0A, 0x16, 0x27);

/// The panel and the card: every raised surface.
///
/// `#1B2C54`, 61,849 px.
pub const PANEL: Color32 = Color32::from_rgb(0x1B, 0x2C, 0x54);

/// The hairline: separators, card outlines and the boundary of a control.
///
/// `#354860`, 13,678 px.
pub const HAIRLINE: Color32 = Color32::from_rgb(0x35, 0x48, 0x60);

/// The accent: the active nav row, a primary button, a toggle that is on.
///
/// `#495ED7`, 4,779 px. An indigo, and the value the whole design turns on.
pub const ACCENT: Color32 = Color32::from_rgb(0x49, 0x5E, 0xD7);

/// The accent pressed, and the shade behind a hovered accent.
///
/// `#384EB6`, 1,736 px.
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0x38, 0x4E, 0xB6);

/// The accent highlight: a link, and the lift a hovered control takes.
///
/// `#6E93D3`, 1,251 px.
pub const ACCENT_HIGHLIGHT: Color32 = Color32::from_rgb(0x6E, 0x93, 0xD3);

/// The muted text: a caption, a note, the version in the footer.
///
/// `#7489A0`, 4,642 px.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x74, 0x89, 0xA0);

/// The secondary text: a row's description and a control that is not selected.
///
/// `#A3B1CB`, 5,376 px.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA3, 0xB1, 0xCB);

/// The primary text: near-white over the canvas.
///
/// The reference's table names this role and gives no sampled value, so this is
/// the near-white the reference draws its headings in rather than a colour that
/// was measured.
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF3, 0xF6, 0xFB);

/// The green of a connected status dot, and the red of a disconnected one.
pub const STATE_OK: Color32 = Color32::from_rgb(0x4C, 0xC3, 0x8A);
pub const STATE_BAD: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x6C);

// ---------------------------------------------------------------------------
// The type ramp, in points.
// ---------------------------------------------------------------------------

/// The pane's own title, the largest text on screen.
pub const TEXT_TITLE: f32 = 21.0;
/// The wordmark beside the mark in the sidebar.
pub const TEXT_WORDMARK: f32 = 17.0;
/// A section heading inside a card.
pub const TEXT_SECTION: f32 = 15.0;
/// Body text and every control label.
pub const TEXT_BODY: f32 = 13.0;
/// A caption, a note, a footer line.
pub const TEXT_CAPTION: f32 = 11.5;

// ---------------------------------------------------------------------------
// Spacing, radii and control metrics, in points.
// ---------------------------------------------------------------------------

/// The gap between two small things: a label and its note.
pub const SPACE_XS: f32 = 4.0;
/// The gap between two rows.
pub const SPACE_SM: f32 = 8.0;
/// The gap between two sections.
pub const SPACE_MD: f32 = 12.0;
/// The gap between the window's edge and its content.
pub const SPACE_LG: f32 = 20.0;

/// The radius of a pill and a small control.
pub const RADIUS_SM: u8 = 6;
/// The radius of a row and a nav item.
pub const RADIUS_ROW: u8 = 8;
/// The radius of a card.
pub const RADIUS_MD: u8 = 10;

/// The sidebar's width, a little under a quarter of the window.
pub const SIDEBAR_WIDTH: f32 = 188.0;
/// How tall a button, a field and a nav row are.
pub const CONTROL_HEIGHT: f32 = 28.0;
/// The padding inside a card.
pub const CARD_MARGIN: f32 = 14.0;
/// The width of a toggle switch, and its height.
pub const TOGGLE_WIDTH: f32 = 38.0;
pub const TOGGLE_HEIGHT: f32 = 21.0;
/// The side of the mark drawn in the sidebar.
pub const MARK: f32 = 22.0;

// ---------------------------------------------------------------------------
// The mark, as the window draws it.
//
// The same three-arm spiral mark.svg holds: a 16x16 viewBox, three arms of one
// spiral, 150 degrees each, a third of a turn apart, wound clockwise out of a
// hollow centre. `scripts/make-icons.sh` rasterises the SVG for the platform
// sets; this draws the same geometry with the renderer the window already has,
// so the header carries the mark in the accent and no second raster is
// committed. `offset` centres the ink the way the SVG's path does.
// ---------------------------------------------------------------------------

/// The arm's inner radius, its outer radius and how far it winds, in viewBox
/// units and radians.
pub const MARK_INNER: f32 = 2.0;
pub const MARK_OUTER: f32 = 6.6;
pub const MARK_SWEEP: f32 = 2.618; // 150 degrees
/// The whole figure's rotation, and the offset that centres its ink.
pub const MARK_PHASE: f32 = -1.047; // -60 degrees
pub const MARK_OFFSET: [f32; 2] = [8.1067, 7.2255];
/// The stroke's width in viewBox units, and the box those units span.
pub const MARK_STROKE: f32 = 2.2;
pub const MARK_BOX: f32 = 16.0;
/// How many segments an arm is sampled at. Enough that the joins are invisible
/// at the size the sidebar draws it.
pub const MARK_STEPS: usize = 48;

/// Install the palette on the window's dark theme.
///
/// Only the dark theme is touched: [`egui::Context::set_style_of`] replaces one
/// theme's style, and the light one is what egui shipped. The preference is
/// pinned to dark because the reference is a dark window, and a window whose
/// colours depended on the machine's appearance would be two designs.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.set_style_of(Theme::Dark, style());
}

/// The style the dark theme carries.
pub fn style() -> egui::Style {
    let mut style = egui::Style::default();
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = CANVAS;
    visuals.window_fill = CANVAS;
    visuals.extreme_bg_color = Color32::from_rgb(0x07, 0x10, 0x1D);
    visuals.code_bg_color = PANEL;
    visuals.faint_bg_color = HAIRLINE;
    visuals.hyperlink_color = ACCENT_HIGHLIGHT;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.window_corner_radius = CornerRadius::same(RADIUS_MD);
    visuals.window_stroke = Stroke::new(1.0, HAIRLINE);

    let row = CornerRadius::same(RADIUS_ROW);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = CANVAS;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = row;

    visuals.widgets.inactive.bg_fill = PANEL;
    visuals.widgets.inactive.weak_bg_fill = PANEL;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = row;

    visuals.widgets.hovered.bg_fill = ACCENT_PRESSED;
    visuals.widgets.hovered.weak_bg_fill = ACCENT_PRESSED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_HIGHLIGHT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.corner_radius = row;

    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_HIGHLIGHT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = row;

    visuals.widgets.open.bg_fill = PANEL;
    visuals.widgets.open.weak_bg_fill = PANEL;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.open.corner_radius = row;

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(SPACE_SM, SPACE_SM);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.scroll.bar_width = 8.0;

    let proportional = FontFamily::Proportional;
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(TEXT_TITLE, proportional.clone()),
        ),
        (
            TextStyle::Body,
            FontId::new(TEXT_BODY, proportional.clone()),
        ),
        (
            TextStyle::Button,
            FontId::new(TEXT_BODY, proportional.clone()),
        ),
        (
            TextStyle::Small,
            FontId::new(TEXT_CAPTION, proportional.clone()),
        ),
        (
            TextStyle::Monospace,
            FontId::new(TEXT_BODY, FontFamily::Monospace),
        ),
    ]
    .into();
    style
}

/// Draw the mark, in `color`, in a square that is `size` points on a side.
///
/// The three arms are painted as runs of filled discs, which is what rounds both
/// the caps and the joins: egui has no round-capped stroke, and a square cap on
/// a 2-point arm is visible at the footer's size.
pub fn mark(ui: &mut egui::Ui, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    let centre = rect.center();
    let scale = size / MARK_BOX;
    let radius = MARK_STROKE / 2.0 * scale;
    let offset = egui::vec2(
        (MARK_OFFSET[0] - MARK_BOX / 2.0) * scale,
        (MARK_OFFSET[1] - MARK_BOX / 2.0) * scale,
    );
    for arm in 0..3 {
        let phase = MARK_PHASE + arm as f32 * std::f32::consts::TAU / 3.0;
        for step in 0..=MARK_STEPS {
            let t = step as f32 / MARK_STEPS as f32;
            let r = (MARK_INNER + (MARK_OUTER - MARK_INNER) * t) * scale;
            let angle = phase + MARK_SWEEP * t;
            let point = centre + offset + egui::vec2(r * angle.cos(), r * angle.sin());
            painter.circle_filled(point, radius, color);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each palette constant against the value the reference's histogram
    /// measured. This is the check that a later refactor cannot drift the
    /// colour silently: a theme that reads dark but blue-ish, and lands on a
    /// stock near-miss, is a different design and fails here.
    #[test]
    fn every_sampled_colour_is_the_one_the_reference_measured() {
        let sampled: [(&str, Color32, u32); 8] = [
            ("canvas", CANVAS, 0x0A1627),
            ("panel", PANEL, 0x1B2C54),
            ("hairline", HAIRLINE, 0x354860),
            ("accent", ACCENT, 0x495ED7),
            ("accent pressed", ACCENT_PRESSED, 0x384EB6),
            ("accent highlight", ACCENT_HIGHLIGHT, 0x6E93D3),
            ("text muted", TEXT_MUTED, 0x7489A0),
            ("text secondary", TEXT_SECONDARY, 0xA3B1CB),
        ];
        for (role, got, hex) in sampled {
            let want = Color32::from_rgb(
                ((hex >> 16) & 0xFF) as u8,
                ((hex >> 8) & 0xFF) as u8,
                (hex & 0xFF) as u8,
            );
            assert_eq!(got, want, "{role} drifted from #{hex:06X}");
        }
    }

    /// The three values the design turns on, spelled out on their own so a
    /// reader of the test sees the hex triples rather than a loop over them.
    #[test]
    fn the_three_load_bearing_values_are_exact() {
        assert_eq!(CANVAS, Color32::from_rgb(0x0A, 0x16, 0x27));
        assert_eq!(PANEL, Color32::from_rgb(0x1B, 0x2C, 0x54));
        assert_eq!(ACCENT, Color32::from_rgb(0x49, 0x5E, 0xD7));
    }

    /// The mark's arm geometry is the SVG's, so the header and the platform
    /// rasters are one drawing rather than two that drifted.
    #[test]
    fn the_mark_geometry_matches_the_svg_box() {
        assert!((MARK_SWEEP - 150f32.to_radians()).abs() < 1e-3);
        assert!((MARK_PHASE - (-60f32).to_radians()).abs() < 1e-3);
        const { assert!(MARK_INNER < MARK_OUTER) };
        // The ink stays inside the canvas the SVG is authored in.
        let reach = MARK_OUTER + MARK_STROKE / 2.0 + MARK_OFFSET[0] - MARK_BOX;
        assert!(reach <= 0.01, "the mark overflows its box by {reach}");
    }
}
