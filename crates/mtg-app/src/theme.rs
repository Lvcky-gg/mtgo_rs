//! Shared colors and spacing for the native screens.
use egui::{Color32, FontId, Stroke, TextStyle, Vec2};

pub const INK: Color32 = Color32::from_rgb(15, 21, 29);
pub const PANEL: Color32 = Color32::from_rgb(23, 31, 42);
pub const SURFACE: Color32 = Color32::from_rgb(31, 42, 55);
pub const BORDER: Color32 = Color32::from_rgb(59, 75, 91);
pub const TEXT: Color32 = Color32::from_rgb(235, 232, 222);
pub const MUTED: Color32 = Color32::from_rgb(153, 167, 181);
pub const GOLD: Color32 = Color32::from_rgb(224, 183, 108);
pub const GOOD: Color32 = Color32::from_rgb(119, 199, 177);
pub const WARN: Color32 = Color32::from_rgb(237, 140, 111);

pub fn install(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(24.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(TEXT);
        visuals.panel_fill = PANEL;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = INK;
        visuals.faint_bg_color = SURFACE;
        visuals.window_stroke = Stroke::new(1.0, BORDER);
        visuals.window_corner_radius = 12.into();
        visuals.selection.bg_fill = Color32::from_rgb(77, 65, 43);
        visuals.selection.stroke = Stroke::new(1.5, GOLD);
        visuals.hyperlink_color = GOLD;
        visuals.warn_fg_color = GOLD;
        visuals.error_fg_color = WARN;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
        ] {
            widget.bg_fill = SURFACE;
            widget.weak_bg_fill = SURFACE;
            widget.bg_stroke = Stroke::new(1.0, BORDER);
            widget.fg_stroke = Stroke::new(1.0, TEXT);
            widget.corner_radius = 6.into();
        }
        for widget in [
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_fill = Color32::from_rgb(47, 61, 75);
            widget.weak_bg_fill = widget.bg_fill;
            widget.bg_stroke = Stroke::new(1.5, GOLD);
            widget.fg_stroke = Stroke::new(1.0, TEXT);
            widget.corner_radius = 6.into();
        }
    });
}

pub fn panel() -> egui::Frame {
    egui::Frame::new().fill(PANEL).inner_margin(12)
}
