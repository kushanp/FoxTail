use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

pub const FOX: Color32 = Color32::from_rgb(232, 122, 46);
pub const FOX_DIM: Color32 = Color32::from_rgb(180, 90, 30);
pub const BG: Color32 = Color32::from_rgb(18, 18, 22);
pub const PANEL: Color32 = Color32::from_rgb(26, 26, 32);
pub const ROW: Color32 = Color32::from_rgb(22, 22, 28);
pub const TEXT: Color32 = Color32::from_rgb(224, 224, 230);
pub const DIM: Color32 = Color32::from_rgb(130, 132, 142);
pub const FIND: Color32 = Color32::from_rgb(255, 220, 80);

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        apply_to_style(style);
    });
}

fn apply_to_style(style: &mut Style) {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(32, 32, 40);
    v.code_bg_color = BG;
    v.hyperlink_color = FOX;
    v.warn_fg_color = Color32::from_rgb(230, 180, 70);
    v.error_fg_color = Color32::from_rgb(230, 80, 80);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(232, 122, 46, 70);
    v.selection.stroke = Stroke::new(1.0, FOX);
    v.widgets.inactive.bg_fill = Color32::from_rgb(40, 40, 48);
    v.widgets.hovered.bg_fill = Color32::from_rgb(56, 48, 42);
    v.widgets.active.bg_fill = FOX_DIM;
    v.widgets.open.bg_fill = Color32::from_rgb(56, 42, 32);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(50, 50, 58));
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
