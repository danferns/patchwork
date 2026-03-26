use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    let (brightness, contrast, saturation, hue, exposure, gamma) = match node_type {
        NodeType::ImageEffects { brightness, contrast, saturation, hue, exposure, gamma } =>
            (brightness, contrast, saturation, hue, exposure, gamma),
        _ => return,
    };

    // Port 0: Image input
    render_image_port(ui, node_id, 0, "Image", connections, values, port_positions, dragging_from);

    ui.separator();

    // Parameters: each is ● Label: on first line, slider + value on second
    struct Param<'a> { port: usize, label: &'a str, val: &'a mut f32, min: f32, max: f32, suffix: &'a str }
    let mut params = [
        Param { port: 1, label: "Brightness", val: brightness, min: 0.0, max: 3.0, suffix: "" },
        Param { port: 2, label: "Contrast", val: contrast, min: 0.0, max: 3.0, suffix: "" },
        Param { port: 3, label: "Saturation", val: saturation, min: 0.0, max: 3.0, suffix: "" },
        Param { port: 4, label: "Hue", val: hue, min: 0.0, max: 360.0, suffix: "°" },
        Param { port: 5, label: "Exposure", val: exposure, min: -3.0, max: 3.0, suffix: "" },
        Param { port: 6, label: "Gamma", val: gamma, min: 0.1, max: 3.0, suffix: "" },
    ];

    for param in &mut params {
        let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == param.port);

        // Override from input if connected
        if is_wired {
            let v = Graph::static_input_value(connections, values, node_id, param.port);
            *param.val = v.as_float();
        }

        // Row 1: ● Label:
        ui.horizontal(|ui| {
            // Port circle
            let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
            let col = if response.hovered() || response.dragged() {
                egui::Color32::YELLOW
            } else if is_wired {
                egui::Color32::from_rgb(80, 170, 255)
            } else {
                egui::Color32::from_rgb(140, 140, 140)
            };
            ui.painter().circle_filled(rect.center(), 4.0, col);
            ui.painter().circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
            port_positions.insert((node_id, param.port, true), rect.center());
            if response.drag_started() {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == param.port) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                } else {
                    *dragging_from = Some((node_id, param.port, false));
                }
            }

            ui.label(egui::RichText::new(format!("{}:", param.label)).small());

            if is_wired {
                ui.label(egui::RichText::new(format!("{:.2}{}", *param.val, param.suffix))
                    .small().monospace().color(egui::Color32::from_rgb(80, 170, 255)));
            }
        });

        // Row 2: slider + value (only if not wired — wired shows value inline above)
        if !is_wired {
            ui.horizontal(|ui| {
                ui.add_space(16.0); // indent to align with label
                let slider = egui::Slider::new(param.val, param.min..=param.max)
                    .show_value(true);
                let slider = if !param.suffix.is_empty() { slider.suffix(param.suffix) } else { slider };
                ui.add(slider);
            });
        }
    }

    // Output port for processed image
    ui.separator();
    render_output_port(ui, node_id, 0, "Image", connections, values, port_positions, dragging_from);

    // Preview info
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    if let PortValue::Image(img) = &input_val {
        ui.label(egui::RichText::new(format!("{}x{}", img.width, img.height)).small().color(egui::Color32::GRAY));
    } else {
        ui.colored_label(egui::Color32::GRAY, "Connect image input");
    }
}

/// Render an input port with label
fn render_image_port(
    ui: &mut egui::Ui,
    node_id: NodeId,
    port: usize,
    label: &str,
    connections: &[Connection],
    values: &HashMap<(NodeId, usize), PortValue>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == port);
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if response.hovered() || response.dragged() {
            egui::Color32::YELLOW
        } else if is_wired {
            egui::Color32::from_rgb(80, 170, 255)
        } else {
            egui::Color32::from_rgb(140, 140, 140)
        };
        ui.painter().circle_filled(rect.center(), 5.0, col);
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, port, true), rect.center());
        if response.drag_started() {
            if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == port) {
                *dragging_from = Some((existing.from_node, existing.from_port, true));
            } else {
                *dragging_from = Some((node_id, port, false));
            }
        }

        ui.label(egui::RichText::new(format!("{}:", label)).small());
        if is_wired {
            let v = Graph::static_input_value(connections, values, node_id, port);
            match &v {
                PortValue::Image(img) => {
                    ui.label(egui::RichText::new(format!("[{}x{}]", img.width, img.height))
                        .small().color(egui::Color32::from_rgb(80, 170, 255)));
                }
                _ => { ui.label(egui::RichText::new("—").small()); }
            }
        } else {
            ui.label(egui::RichText::new("—").small().color(egui::Color32::GRAY));
        }
    });
}

/// Render an output port with label
fn render_output_port(
    ui: &mut egui::Ui,
    node_id: NodeId,
    port: usize,
    label: &str,
    _connections: &[Connection],
    values: &HashMap<(NodeId, usize), PortValue>,
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{}:", label)).small());
        let v = values.get(&(node_id, port));
        match v {
            Some(PortValue::Image(img)) => {
                ui.label(egui::RichText::new(format!("[{}x{}]", img.width, img.height))
                    .small().color(egui::Color32::from_rgb(80, 170, 255)));
            }
            _ => { ui.label(egui::RichText::new("—").small()); }
        }
        let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
        let col = if response.hovered() || response.dragged() {
            egui::Color32::YELLOW
        } else {
            egui::Color32::from_rgb(80, 170, 255)
        };
        ui.painter().circle_filled(rect.center(), 5.0, col);
        ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        port_positions.insert((node_id, port, false), rect.center());
        if response.drag_started() {
            *dragging_from = Some((node_id, port, true));
        }
    });
}

/// Process image with effects. Works on full resolution.
pub fn process(img: &ImageData, brightness: f32, contrast: f32, saturation: f32, hue: f32, exposure: f32, gamma: f32) -> Arc<ImageData> {
    let mut pixels = img.pixels.clone();
    let len = pixels.len();
    let hue_rad = hue * std::f32::consts::PI / 180.0;

    let mut i = 0;
    while i + 3 < len {
        let mut r = pixels[i] as f32 / 255.0;
        let mut g = pixels[i+1] as f32 / 255.0;
        let mut b = pixels[i+2] as f32 / 255.0;

        // Exposure (applied first, in linear space)
        if exposure.abs() > 0.001 {
            let mult = 2.0f32.powf(exposure);
            r *= mult; g *= mult; b *= mult;
        }

        // Brightness
        r *= brightness; g *= brightness; b *= brightness;

        // Contrast (around 0.5 midpoint)
        r = (r - 0.5) * contrast + 0.5;
        g = (g - 0.5) * contrast + 0.5;
        b = (b - 0.5) * contrast + 0.5;

        // Saturation
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        r = lum + (r - lum) * saturation;
        g = lum + (g - lum) * saturation;
        b = lum + (b - lum) * saturation;

        // Hue rotation
        if hue_rad.abs() > 0.001 {
            let cos_h = hue_rad.cos();
            let sin_h = hue_rad.sin();
            let nr = r * (0.213 + 0.787 * cos_h - 0.213 * sin_h)
                   + g * (0.715 - 0.715 * cos_h - 0.715 * sin_h)
                   + b * (0.072 - 0.072 * cos_h + 0.928 * sin_h);
            let ng = r * (0.213 - 0.213 * cos_h + 0.143 * sin_h)
                   + g * (0.715 + 0.285 * cos_h + 0.140 * sin_h)
                   + b * (0.072 - 0.072 * cos_h - 0.283 * sin_h);
            let nb = r * (0.213 - 0.213 * cos_h - 0.787 * sin_h)
                   + g * (0.715 - 0.715 * cos_h + 0.715 * sin_h)
                   + b * (0.072 + 0.928 * cos_h + 0.072 * sin_h);
            r = nr; g = ng; b = nb;
        }

        // Gamma
        if (gamma - 1.0).abs() > 0.001 {
            let inv_g = 1.0 / gamma;
            r = r.max(0.0).powf(inv_g);
            g = g.max(0.0).powf(inv_g);
            b = b.max(0.0).powf(inv_g);
        }

        pixels[i]   = (r.clamp(0.0, 1.0) * 255.0) as u8;
        pixels[i+1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
        pixels[i+2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
        i += 4;
    }

    Arc::new(ImageData { width: img.width, height: img.height, pixels })
}
