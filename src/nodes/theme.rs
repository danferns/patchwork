use crate::graph::{NodeId, PortValue, Connection};
use std::collections::HashMap;
use eframe::egui;

const PORT_RADIUS: f32 = 4.0;
const PORT_SIZE: f32 = 10.0;
const IN_COLOR: egui::Color32 = egui::Color32::from_rgb(170, 170, 170);
const OUT_COLOR: egui::Color32 = egui::Color32::from_rgb(100, 180, 255);

/// Draw a small inline input port, register its position.
fn inline_input(
    ui: &mut egui::Ui,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    node_id: NodeId,
    port: usize,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) -> Option<f32> {
    let connected = connections.iter().any(|c| c.to_node == node_id && c.to_port == port);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(PORT_SIZE, PORT_SIZE), egui::Sense::click_and_drag());
    let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else if connected { egui::Color32::from_rgb(100, 200, 255) } else { IN_COLOR };
    ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
    ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(0.5, egui::Color32::WHITE));
    port_positions.insert((node_id, port, true), rect.center());
    if resp.drag_started() { *dragging_from = Some((node_id, port, false)); }
    if connected {
        for c in connections {
            if c.to_node == node_id && c.to_port == port {
                return values.get(&(c.from_node, c.from_port)).map(|v| v.as_float());
            }
        }
    }
    None
}

/// Draw a small inline output port, register its position.
fn inline_output(
    ui: &mut egui::Ui,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    node_id: NodeId,
    port: usize,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(PORT_SIZE, PORT_SIZE), egui::Sense::click_and_drag());
    let col = if resp.hovered() || resp.dragged() { egui::Color32::YELLOW } else { OUT_COLOR };
    ui.painter().circle_filled(rect.center(), PORT_RADIUS, col);
    ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(0.5, egui::Color32::WHITE));
    port_positions.insert((node_id, port, false), rect.center());
    if resp.drag_started() { *dragging_from = Some((node_id, port, true)); }
}

/// Color row: [in R] [in G] [in B] label [picker] R G B [out R] [out G] [out B]
fn color_row(
    ui: &mut egui::Ui,
    label: &str,
    rgb: &mut [u8; 3],
    use_hsl: bool,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    node_id: NodeId,
    in_base: usize,
    out_base: usize,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    // Apply overrides from inputs
    if let Some(v) = inline_input_val(values, connections, node_id, in_base) { rgb[0] = v.clamp(0.0, 255.0) as u8; }
    if let Some(v) = inline_input_val(values, connections, node_id, in_base + 1) { rgb[1] = v.clamp(0.0, 255.0) as u8; }
    if let Some(v) = inline_input_val(values, connections, node_id, in_base + 2) { rgb[2] = v.clamp(0.0, 255.0) as u8; }

    ui.horizontal(|ui| {
        // Input ports on left
        inline_input(ui, port_positions, dragging_from, node_id, in_base, values, connections);
        inline_input(ui, port_positions, dragging_from, node_id, in_base + 1, values, connections);
        inline_input(ui, port_positions, dragging_from, node_id, in_base + 2, values, connections);

        ui.label(egui::RichText::new(label).small());
        let mut color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *rgb = [color.r(), color.g(), color.b()];
        }

        // Output ports on right
        inline_output(ui, port_positions, dragging_from, node_id, out_base);
        inline_output(ui, port_positions, dragging_from, node_id, out_base + 1);
        inline_output(ui, port_positions, dragging_from, node_id, out_base + 2);
    });

    // Number editing row
    if use_hsl {
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        let mut hh = h; let mut ss = s; let mut ll = l;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.add_space(PORT_SIZE * 3.0 + 12.0); // align with above
            ui.label(egui::RichText::new("H").small());
            changed |= ui.add(egui::DragValue::new(&mut hh).range(0.0..=360.0).speed(1.0)).changed();
            ui.label(egui::RichText::new("S").small());
            changed |= ui.add(egui::DragValue::new(&mut ss).range(0.0..=100.0).speed(0.5)).changed();
            ui.label(egui::RichText::new("L").small());
            changed |= ui.add(egui::DragValue::new(&mut ll).range(0.0..=100.0).speed(0.5)).changed();
        });
        if changed {
            let (r, g, b) = hsl_to_rgb(hh, ss, ll);
            *rgb = [r, g, b];
        }
    } else {
        ui.horizontal(|ui| {
            ui.add_space(PORT_SIZE * 3.0 + 12.0);
            ui.add(egui::DragValue::new(&mut rgb[0]).range(0..=255).speed(1.0).prefix("R "));
            ui.add(egui::DragValue::new(&mut rgb[1]).range(0..=255).speed(1.0).prefix("G "));
            ui.add(egui::DragValue::new(&mut rgb[2]).range(0..=255).speed(1.0).prefix("B "));
        });
    }
}

/// Float row: [in] label [slider] value
fn float_row(
    ui: &mut egui::Ui,
    label: &str,
    val: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    node_id: NodeId,
    port: usize,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    if let Some(v) = inline_input_val(values, connections, node_id, port) { *val = v; }
    ui.horizontal(|ui| {
        inline_input(ui, port_positions, dragging_from, node_id, port, values, connections);
        ui.label(egui::RichText::new(label).small());
        ui.add(egui::Slider::new(val, range).suffix(suffix));
    });
}

fn u8_row(
    ui: &mut egui::Ui,
    label: &str,
    val: &mut u8,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
    node_id: NodeId,
    port: usize,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    if let Some(v) = inline_input_val(values, connections, node_id, port) { *val = v.clamp(0.0, 255.0) as u8; }
    let mut f = *val as f32;
    ui.horizontal(|ui| {
        inline_input(ui, port_positions, dragging_from, node_id, port, values, connections);
        ui.label(egui::RichText::new(label).small());
        if ui.add(egui::Slider::new(&mut f, 0.0..=255.0)).changed() { *val = f as u8; }
    });
}

fn inline_input_val(values: &HashMap<(NodeId, usize), PortValue>, connections: &[Connection], node_id: NodeId, port: usize) -> Option<f32> {
    let connected = connections.iter().any(|c| c.to_node == node_id && c.to_port == port);
    if !connected { return None; }
    for c in connections {
        if c.to_node == node_id && c.to_port == port {
            return values.get(&(c.from_node, c.from_port)).map(|v| v.as_float());
        }
    }
    None
}

// Input ports: 0-2 BG, 3-5 Text, 6-8 Accent, 9-11 Win, 12-14 Grid, 15 Font, 16 Round, 17 Space, 18 Alpha
// Output ports: 0-2 BG, 3-5 Text, 6-8 Accent

pub fn render(
    ui: &mut egui::Ui,
    dark_mode: &mut bool,
    accent: &mut [u8; 3],
    font_size: &mut f32,
    bg_color: &mut [u8; 3],
    text_color: &mut [u8; 3],
    window_bg: &mut [u8; 3],
    window_alpha: &mut u8,
    grid_color: &mut [u8; 3],
    rounding: &mut f32,
    spacing: &mut f32,
    use_hsl: &mut bool,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Presets
    ui.horizontal(|ui| {
        if ui.small_button("Dark").clicked() {
            *dark_mode = true;
            *bg_color = [30, 30, 30]; *text_color = [220, 220, 220];
            *window_bg = [40, 40, 40]; *window_alpha = 240; *grid_color = [12, 12, 12];
        }
        if ui.small_button("Light").clicked() {
            *dark_mode = false;
            *bg_color = [240, 240, 240]; *text_color = [30, 30, 30];
            *window_bg = [255, 255, 255]; *window_alpha = 240; *grid_color = [200, 200, 200];
        }
        if ui.small_button("Blue").clicked() {
            *dark_mode = true;
            *bg_color = [15, 20, 35]; *text_color = [200, 210, 230];
            *window_bg = [20, 30, 50]; *window_alpha = 230;
            *grid_color = [20, 25, 45]; *accent = [60, 140, 255];
        }
    });

    ui.horizontal(|ui| {
        ui.selectable_value(use_hsl, false, "RGB");
        ui.selectable_value(use_hsl, true, "HSL");
    });

    ui.separator();

    // Colors with inline ports: in ports (left) ● ● ● label [picker] ● ● ● out ports (right)
    color_row(ui, "BG", bg_color, *use_hsl, port_positions, dragging_from, node_id, 0, 0, values, connections);
    color_row(ui, "Text", text_color, *use_hsl, port_positions, dragging_from, node_id, 3, 3, values, connections);
    color_row(ui, "Accent", accent, *use_hsl, port_positions, dragging_from, node_id, 6, 6, values, connections);
    color_row(ui, "Window", window_bg, *use_hsl, port_positions, dragging_from, node_id, 9, 0, values, connections);
    color_row(ui, "Grid", grid_color, *use_hsl, port_positions, dragging_from, node_id, 12, 0, values, connections);

    ui.separator();

    // Float params with inline input ports
    float_row(ui, "Font", font_size, 8.0..=28.0, "px", port_positions, dragging_from, node_id, 15, values, connections);
    float_row(ui, "Round", rounding, 0.0..=80.0, "px", port_positions, dragging_from, node_id, 16, values, connections);
    float_row(ui, "Space", spacing, 0.0..=12.0, "px", port_positions, dragging_from, node_id, 17, values, connections);
    u8_row(ui, "Opacity", window_alpha, port_positions, dragging_from, node_id, 18, values, connections);
}

/// Apply theme settings to the egui context.
pub fn apply(
    ctx: &egui::Context,
    dark_mode: bool,
    accent: [u8; 3],
    font_size: f32,
    bg_color: [u8; 3],
    text_color: [u8; 3],
    window_bg: [u8; 3],
    window_alpha: u8,
    rounding: f32,
    spacing: f32,
) {
    let mut visuals = if dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() };

    let bg = egui::Color32::from_rgb(bg_color[0], bg_color[1], bg_color[2]);
    let text = egui::Color32::from_rgb(text_color[0], text_color[1], text_color[2]);
    let accent_color = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
    let win_bg = egui::Color32::from_rgba_unmultiplied(window_bg[0], window_bg[1], window_bg[2], window_alpha);

    visuals.panel_fill = bg;
    visuals.window_fill = win_bg;
    visuals.faint_bg_color = win_bg;
    visuals.extreme_bg_color = bg;
    visuals.override_text_color = Some(text);
    visuals.selection.bg_fill = accent_color;
    visuals.hyperlink_color = accent_color;
    visuals.widgets.hovered.bg_fill = accent_color.gamma_multiply(0.15);

    let r = egui::CornerRadius::same(rounding as u8);
    visuals.window_corner_radius = r;
    visuals.widgets.noninteractive.corner_radius = r;
    visuals.widgets.inactive.corner_radius = r;
    visuals.widgets.hovered.corner_radius = r;
    visuals.widgets.active.corner_radius = r;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    for (_, font_id) in style.text_styles.iter_mut() {
        font_id.size = font_size;
    }
    style.spacing.item_spacing = egui::vec2(spacing, spacing);
    ctx.set_style(style);
}

// ── HSL helpers ──────────────────────────────────────────────────────────

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l * 100.0); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - rf).abs() < 1e-6 {
        ((gf - bf) / d) + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < 1e-6 {
        ((bf - rf) / d) + 2.0
    } else {
        ((rf - gf) / d) + 4.0
    };
    (h * 60.0, s * 100.0, l * 100.0)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let s = s / 100.0;
    let l = l / 100.0;
    if s.abs() < 1e-6 { let v = (l * 255.0) as u8; return (v, v, v); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let h = h / 360.0;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}
