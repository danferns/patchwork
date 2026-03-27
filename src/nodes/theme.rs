use crate::graph::{NodeId, PortValue, Connection};
use std::collections::HashMap;
use eframe::egui;

const PORT_RADIUS: f32 = 6.0;
const PORT_SIZE: f32 = 12.0;
const IN_COLOR: egui::Color32 = egui::Color32::from_rgb(70, 75, 85);
const IN_BORDER: egui::Color32 = egui::Color32::from_rgb(120, 125, 135);
const WIRED_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 140, 255);
const WIRED_BORDER: egui::Color32 = egui::Color32::from_rgb(120, 180, 255);
const OUT_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 140, 255);
const OUT_BORDER: egui::Color32 = egui::Color32::from_rgb(120, 180, 255);
const PORT_STROKE: f32 = 2.5;

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
    let (fill, border) = if resp.hovered() || resp.dragged() { (egui::Color32::YELLOW, egui::Color32::WHITE) } else if connected { (WIRED_COLOR, WIRED_BORDER) } else { (IN_COLOR, IN_BORDER) };
    ui.painter().circle_filled(rect.center(), PORT_RADIUS, fill);
    ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(PORT_STROKE, border));
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
    let (fill, border) = if resp.hovered() || resp.dragged() { (egui::Color32::YELLOW, egui::Color32::WHITE) } else { (OUT_COLOR, OUT_BORDER) };
    ui.painter().circle_filled(rect.center(), PORT_RADIUS, fill);
    ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(PORT_STROKE, border));
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

    // Label + color swatch (group header)
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small());
        let mut color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *rgb = [color.r(), color.g(), color.b()];
        }
        // Output ports
        if out_base > 0 {
            inline_output(ui, port_positions, dragging_from, node_id, out_base);
            inline_output(ui, port_positions, dragging_from, node_id, out_base + 1);
            inline_output(ui, port_positions, dragging_from, node_id, out_base + 2);
        }
    });

    // R G B — each with its own connector dot + DragValue
    let channel_labels = if use_hsl {
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        vec![("H", h, 0.0f32, 360.0f32), ("S", s, 0.0, 100.0), ("L", l, 0.0, 100.0)]
    } else {
        vec![("R", rgb[0] as f32, 0.0f32, 255.0f32), ("G", rgb[1] as f32, 0.0, 255.0), ("B", rgb[2] as f32, 0.0, 255.0)]
    };

    ui.horizontal(|ui| {
        for (ci, (ch_label, _val, min, max)) in channel_labels.iter().enumerate() {
            let port = in_base + ci;
            let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == port);

            // Port circle
            let (rect, response) = ui.allocate_exact_size(egui::vec2(PORT_SIZE, PORT_SIZE), egui::Sense::click_and_drag());
            let (fill, border) = if response.hovered() || response.dragged() {
                (egui::Color32::YELLOW, egui::Color32::WHITE)
            } else if is_wired {
                (WIRED_COLOR, WIRED_BORDER)
            } else {
                (IN_COLOR, IN_BORDER)
            };
            ui.painter().circle_filled(rect.center(), PORT_RADIUS, fill);
            ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(PORT_STROKE, border));
            port_positions.insert((node_id, port, true), rect.center());

            if response.drag_started() {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                } else {
                    *dragging_from = Some((node_id, port, false));
                }
            }

            // Label + value
            ui.label(egui::RichText::new(*ch_label).small());
            if is_wired {
                let v = inline_input_val(values, connections, node_id, port).unwrap_or(0.0);
                ui.label(egui::RichText::new(format!("{:.0}", v)).small().monospace());
            } else if use_hsl {
                let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
                let mut vals = [h, s, l];
                if ui.add(egui::DragValue::new(&mut vals[ci]).range(*min..=*max).speed(1.0)).changed() {
                    let (r, g, b) = hsl_to_rgb(vals[0], vals[1], vals[2]);
                    // Only update the changed channel's effect
                    match ci {
                        0 => { let (nr, ng, nb) = hsl_to_rgb(vals[0], s, l); *rgb = [nr, ng, nb]; }
                        1 => { let (nr, ng, nb) = hsl_to_rgb(h, vals[1], l); *rgb = [nr, ng, nb]; }
                        2 => { let (nr, ng, nb) = hsl_to_rgb(h, s, vals[2]); *rgb = [nr, ng, nb]; }
                        _ => {}
                    }
                }
            } else {
                ui.add(egui::DragValue::new(&mut rgb[ci]).range(0..=255).speed(1.0));
            }
        }
    });
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
    wire_thickness: &mut f32,
    background_path: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // Save / Open
    ui.horizontal(|ui| {
        if ui.small_button("Save...").clicked() {
            let data = serde_json::json!({
                "dark_mode": *dark_mode,
                "bg_color": *bg_color,
                "text_color": *text_color,
                "accent": *accent,
                "window_bg": *window_bg,
                "window_alpha": *window_alpha,
                "grid_color": *grid_color,
                "font_size": *font_size,
                "rounding": *rounding,
                "spacing": *spacing,
                "use_hsl": *use_hsl,
            });
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("theme.json")
                .add_filter("JSON", &["json"])
                .save_file()
            {
                let _ = std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap_or_default());
            }
        }
        if ui.small_button("Open...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file()
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(b) = v.get("dark_mode").and_then(|v| v.as_bool()) { *dark_mode = b; }
                        if let Some(a) = v.get("bg_color").and_then(|v| v.as_array()) {
                            if a.len() == 3 { *bg_color = [a[0].as_u64().unwrap_or(0) as u8, a[1].as_u64().unwrap_or(0) as u8, a[2].as_u64().unwrap_or(0) as u8]; }
                        }
                        if let Some(a) = v.get("text_color").and_then(|v| v.as_array()) {
                            if a.len() == 3 { *text_color = [a[0].as_u64().unwrap_or(0) as u8, a[1].as_u64().unwrap_or(0) as u8, a[2].as_u64().unwrap_or(0) as u8]; }
                        }
                        if let Some(a) = v.get("accent").and_then(|v| v.as_array()) {
                            if a.len() == 3 { *accent = [a[0].as_u64().unwrap_or(0) as u8, a[1].as_u64().unwrap_or(0) as u8, a[2].as_u64().unwrap_or(0) as u8]; }
                        }
                        if let Some(a) = v.get("window_bg").and_then(|v| v.as_array()) {
                            if a.len() == 3 { *window_bg = [a[0].as_u64().unwrap_or(0) as u8, a[1].as_u64().unwrap_or(0) as u8, a[2].as_u64().unwrap_or(0) as u8]; }
                        }
                        if let Some(n) = v.get("window_alpha").and_then(|v| v.as_u64()) { *window_alpha = n as u8; }
                        if let Some(a) = v.get("grid_color").and_then(|v| v.as_array()) {
                            if a.len() == 3 { *grid_color = [a[0].as_u64().unwrap_or(0) as u8, a[1].as_u64().unwrap_or(0) as u8, a[2].as_u64().unwrap_or(0) as u8]; }
                        }
                        if let Some(n) = v.get("font_size").and_then(|v| v.as_f64()) { *font_size = n as f32; }
                        if let Some(n) = v.get("rounding").and_then(|v| v.as_f64()) { *rounding = n as f32; }
                        if let Some(n) = v.get("spacing").and_then(|v| v.as_f64()) { *spacing = n as f32; }
                        if let Some(b) = v.get("use_hsl").and_then(|v| v.as_bool()) { *use_hsl = b; }
                    }
                }
            }
        }
    });

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
    color_row(ui, "BG", bg_color, *use_hsl, port_positions, dragging_from, node_id, 0, 1, values, connections);
    color_row(ui, "Text", text_color, *use_hsl, port_positions, dragging_from, node_id, 3, 0, values, connections);
    color_row(ui, "Accent", accent, *use_hsl, port_positions, dragging_from, node_id, 6, 0, values, connections);
    color_row(ui, "Window", window_bg, *use_hsl, port_positions, dragging_from, node_id, 9, 0, values, connections);
    color_row(ui, "Grid", grid_color, *use_hsl, port_positions, dragging_from, node_id, 12, 0, values, connections);

    ui.separator();

    // Float params with inline input ports
    float_row(ui, "Font", font_size, 8.0..=28.0, "px", port_positions, dragging_from, node_id, 15, values, connections);
    float_row(ui, "Wire", wire_thickness, 1.0..=16.0, "px", port_positions, dragging_from, node_id, 20, values, connections);
    float_row(ui, "Round", rounding, 0.0..=32.0, "px", port_positions, dragging_from, node_id, 16, values, connections);
    float_row(ui, "Space", spacing, 0.0..=12.0, "px", port_positions, dragging_from, node_id, 17, values, connections);
    u8_row(ui, "Opacity", window_alpha, port_positions, dragging_from, node_id, 18, values, connections);

    ui.separator();

    // ── Background image/video ───────────────────────────────────────
    // Port 19: Background (accepts Image from Image node, WGSL, Video, Blend)
    ui.horizontal(|ui| {
        // Input port circle
        let (rect, response) = ui.allocate_exact_size(egui::vec2(PORT_SIZE, PORT_SIZE), egui::Sense::click_and_drag());
        let bg_connected = connections.iter().any(|c| c.to_node == node_id && c.to_port == 19);
        let (fill, border) = if response.hovered() || response.dragged() {
            (egui::Color32::YELLOW, egui::Color32::WHITE)
        } else if bg_connected {
            (WIRED_COLOR, WIRED_BORDER)
        } else {
            (IN_COLOR, IN_BORDER)
        };
        ui.painter().circle_filled(rect.center(), PORT_RADIUS, fill);
        ui.painter().circle_stroke(rect.center(), PORT_RADIUS, egui::Stroke::new(PORT_STROKE, border));
        port_positions.insert((node_id, 19, true), rect.center());

        if response.drag_started() {
            if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == 19) {
                *dragging_from = Some((existing.from_node, existing.from_port, true));
            } else {
                *dragging_from = Some((node_id, 19, false));
            }
        }

        ui.label(egui::RichText::new("BG").small());

        if bg_connected {
            let val = crate::graph::Graph::static_input_value(connections, values, node_id, 19);
            match &val {
                PortValue::Image(img) => {
                    ui.label(egui::RichText::new(format!("{}x{}", img.width, img.height)).small().monospace());
                }
                _ => { ui.label(egui::RichText::new("connected").small().color(egui::Color32::GRAY)); }
            }
        } else {
            // File path input + open button
            if ui.small_button("Open").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                    .pick_file()
                {
                    *background_path = p.display().to_string();
                }
            }
        }
    });
    if !background_path.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(PORT_SIZE + 4.0);
            let short = if background_path.len() > 25 {
                format!("...{}", &background_path[background_path.len()-25..])
            } else {
                background_path.clone()
            };
            ui.label(egui::RichText::new(short).small().monospace().color(egui::Color32::GRAY));
        });
    }
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
