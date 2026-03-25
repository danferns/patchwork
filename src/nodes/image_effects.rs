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
) {
    let (brightness, contrast, saturation, hue, exposure, gamma) = match node_type {
        NodeType::ImageEffects { brightness, contrast, saturation, hue, exposure, gamma } =>
            (brightness, contrast, saturation, hue, exposure, gamma),
        _ => return,
    };

    // Read params from input ports (override sliders if connected)
    let read_param = |port: usize, default: &mut f32| {
        if connections.iter().any(|c| c.to_node == node_id && c.to_port == port) {
            let v = Graph::static_input_value(connections, values, node_id, port);
            *default = v.as_float();
        }
    };
    read_param(1, brightness);
    read_param(2, contrast);
    read_param(3, saturation);
    read_param(4, hue);
    read_param(5, exposure);
    read_param(6, gamma);

    ui.horizontal(|ui| { ui.label("Brightness:"); ui.add(egui::Slider::new(brightness, 0.0..=3.0)); });
    ui.horizontal(|ui| { ui.label("Contrast:"); ui.add(egui::Slider::new(contrast, 0.0..=3.0)); });
    ui.horizontal(|ui| { ui.label("Saturation:"); ui.add(egui::Slider::new(saturation, 0.0..=3.0)); });
    ui.horizontal(|ui| { ui.label("Hue:"); ui.add(egui::Slider::new(hue, 0.0..=360.0).suffix("°")); });
    ui.horizontal(|ui| { ui.label("Exposure:"); ui.add(egui::Slider::new(exposure, -3.0..=3.0)); });
    ui.horizontal(|ui| { ui.label("Gamma:"); ui.add(egui::Slider::new(gamma, 0.1..=3.0)); });

    // Preview the input image if connected
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    if let PortValue::Image(img) = &input_val {
        ui.separator();
        ui.label(egui::RichText::new(format!("Input: {}x{}", img.width, img.height)).small());
    } else {
        ui.colored_label(egui::Color32::GRAY, "Connect image input");
    }
}

/// Downsample an image for faster preview processing
fn downsample(img: &ImageData, max_dim: u32) -> ImageData {
    if img.width <= max_dim && img.height <= max_dim {
        return img.clone();
    }
    let scale = max_dim as f32 / img.width.max(img.height) as f32;
    let tw = (img.width as f32 * scale).max(1.0) as u32;
    let th = (img.height as f32 * scale).max(1.0) as u32;
    let mut pixels = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        for x in 0..tw {
            let sx = (x as f32 / scale) as u32;
            let sy = (y as f32 / scale) as u32;
            let si = ((sy * img.width + sx) * 4) as usize;
            let di = ((y * tw + x) * 4) as usize;
            if si + 3 < img.pixels.len() && di + 3 < pixels.len() {
                pixels[di..di+4].copy_from_slice(&img.pixels[si..si+4]);
            }
        }
    }
    ImageData::new(tw, th, pixels)
}

/// Process image with effects. Works on full resolution.
pub fn process(img: &ImageData, brightness: f32, contrast: f32, saturation: f32, hue: f32, exposure: f32, gamma: f32) -> Arc<ImageData> {
    let mut pixels = img.pixels.clone();
    let len = pixels.len();
    let mut i = 0;
    while i + 3 < len {
        let mut r = pixels[i] as f32 / 255.0;
        let mut g = pixels[i + 1] as f32 / 255.0;
        let mut b = pixels[i + 2] as f32 / 255.0;

        // Exposure: multiply by 2^exposure
        let exp_mult = 2.0f32.powf(exposure);
        r *= exp_mult;
        g *= exp_mult;
        b *= exp_mult;

        // Brightness
        r *= brightness;
        g *= brightness;
        b *= brightness;

        // Contrast (around 0.5 midpoint)
        r = (r - 0.5) * contrast + 0.5;
        g = (g - 0.5) * contrast + 0.5;
        b = (b - 0.5) * contrast + 0.5;

        // Saturation (lerp to grayscale)
        let gray = r * 0.299 + g * 0.587 + b * 0.114;
        r = gray + (r - gray) * saturation;
        g = gray + (g - gray) * saturation;
        b = gray + (b - gray) * saturation;

        // Hue rotation (simplified — rotate in RGB space)
        if hue.abs() > 0.1 {
            let rad = hue.to_radians();
            let cos_h = rad.cos();
            let sin_h = rad.sin();
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
        if (gamma - 1.0).abs() > 0.01 {
            let inv_gamma = 1.0 / gamma;
            r = r.max(0.0).powf(inv_gamma);
            g = g.max(0.0).powf(inv_gamma);
            b = b.max(0.0).powf(inv_gamma);
        }

        pixels[i] = (r.clamp(0.0, 1.0) * 255.0) as u8;
        pixels[i + 1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
        pixels[i + 2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
        i += 4;
    }
    Arc::new(ImageData::new(img.width, img.height, pixels))
}
