use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

const NOISE_TYPES: &[&str] = &["Perlin", "White"];
const NOISE_MODES: &[&str] = &["1D", "2D"];

pub fn render(
    ui: &mut egui::Ui,
    node_id: NodeId,
    node_type: &mut NodeType,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let (noise_type, mode, scale, seed) = match node_type {
        NodeType::Noise { noise_type, mode, scale, seed } => (noise_type, mode, scale, seed),
        _ => return,
    };

    // Read from input ports if connected
    if connections.iter().any(|c| c.to_node == node_id && c.to_port == 0) {
        *seed = Graph::static_input_value(connections, values, node_id, 0).as_float() as u32;
    }
    if connections.iter().any(|c| c.to_node == node_id && c.to_port == 1) {
        *scale = Graph::static_input_value(connections, values, node_id, 1).as_float();
    }

    ui.horizontal(|ui| {
        ui.label("Type:");
        egui::ComboBox::from_id_salt(egui::Id::new(("noise_type", node_id)))
            .selected_text(*NOISE_TYPES.get(*noise_type as usize).unwrap_or(&"Perlin"))
            .show_ui(ui, |ui| {
                for (i, name) in NOISE_TYPES.iter().enumerate() {
                    if ui.selectable_label(*noise_type == i as u8, *name).clicked() {
                        *noise_type = i as u8;
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Mode:");
        egui::ComboBox::from_id_salt(egui::Id::new(("noise_mode", node_id)))
            .selected_text(*NOISE_MODES.get(*mode as usize).unwrap_or(&"2D"))
            .show_ui(ui, |ui| {
                for (i, name) in NOISE_MODES.iter().enumerate() {
                    if ui.selectable_label(*mode == i as u8, *name).clicked() {
                        *mode = i as u8;
                    }
                }
            });
    });

    ui.horizontal(|ui| { ui.label("Scale:"); ui.add(egui::Slider::new(scale, 0.1..=50.0)); });
    ui.horizontal(|ui| { ui.label("Seed:"); ui.add(egui::DragValue::new(seed)); });

    // Preview
    let preview_size = 128.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(preview_size, if *mode == 0 { 60.0 } else { preview_size }), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 20));

    if *mode == 0 {
        // 1D: waveform preview
        let steps = preview_size as usize;
        let mut prev = None;
        for i in 0..steps {
            let x = i as f32 / steps as f32 * *scale;
            let v = perlin_1d(x, *seed);
            let sx = rect.left() + i as f32;
            let sy = rect.center().y - v * rect.height() * 0.4;
            let pt = egui::pos2(sx, sy);
            if let Some(p) = prev {
                painter.line_segment([p, pt], egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 140)));
            }
            prev = Some(pt);
        }
    } else {
        // 2D: texture preview (low res for speed)
        let tex_size = 64usize;
        let mut pixels = vec![0u8; tex_size * tex_size * 4];
        for y in 0..tex_size {
            for x in 0..tex_size {
                let nx = x as f32 / tex_size as f32 * *scale;
                let ny = y as f32 / tex_size as f32 * *scale;
                let v = if *noise_type == 0 {
                    perlin_2d(nx, ny, *seed)
                } else {
                    white_noise_2d(x as u32, y as u32, *seed)
                };
                let byte = ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                let idx = (y * tex_size + x) * 4;
                pixels[idx] = byte;
                pixels[idx + 1] = byte;
                pixels[idx + 2] = byte;
                pixels[idx + 3] = 255;
            }
        }
        let color_image = egui::ColorImage::from_rgba_unmultiplied([tex_size, tex_size], &pixels);
        let tex = ui.ctx().load_texture(format!("noise_{}", node_id), color_image, egui::TextureOptions::LINEAR);
        let sized = egui::load::SizedTexture::new(tex.id(), rect.size());
        painter.image(sized.id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("noise_tex", node_id)), tex));
    }
}

/// Generate 2D noise image for evaluation
pub fn generate_2d(scale: f32, seed: u32, noise_type: u8, size: u32) -> Arc<ImageData> {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let nx = x as f32 / size as f32 * scale;
            let ny = y as f32 / size as f32 * scale;
            let v = if noise_type == 0 { perlin_2d(nx, ny, seed) } else { white_noise_2d(x, y, seed) };
            let byte = ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let idx = ((y * size + x) * 4) as usize;
            pixels[idx] = byte;
            pixels[idx + 1] = byte;
            pixels[idx + 2] = byte;
            pixels[idx + 3] = 255;
        }
    }
    Arc::new(ImageData::new(size, size, pixels))
}

// ── Simple Perlin noise implementation ──────────────────────────────────────

fn hash(x: i32, seed: u32) -> u32 {
    let mut h = x as u32 ^ seed;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    h
}

fn grad_1d(hash: u32, x: f32) -> f32 {
    if hash & 1 == 0 { x } else { -x }
}

fn grad_2d(hash: u32, x: f32, y: f32) -> f32 {
    match hash & 3 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        _ => -x - y,
    }
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

pub fn perlin_1d(x: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let xf = x - x.floor();
    let u = fade(xf);
    let a = grad_1d(hash(xi, seed), xf);
    let b = grad_1d(hash(xi + 1, seed), xf - 1.0);
    lerp(a, b, u)
}

pub fn perlin_2d(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let u = fade(xf);
    let v = fade(yf);

    let aa = hash(xi + hash(yi, seed) as i32, 0);
    let ab = hash(xi + hash(yi + 1, seed) as i32, 0);
    let ba = hash(xi + 1 + hash(yi, seed) as i32, 0);
    let bb = hash(xi + 1 + hash(yi + 1, seed) as i32, 0);

    let x1 = lerp(grad_2d(aa, xf, yf), grad_2d(ba, xf - 1.0, yf), u);
    let x2 = lerp(grad_2d(ab, xf, yf - 1.0), grad_2d(bb, xf - 1.0, yf - 1.0), u);
    lerp(x1, x2, v)
}

fn white_noise_2d(x: u32, y: u32, seed: u32) -> f32 {
    let h = hash((x as i32).wrapping_mul(374761393).wrapping_add(y as i32).wrapping_mul(668265263), seed);
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}
