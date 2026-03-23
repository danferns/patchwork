use crate::graph::*;
use eframe::egui;
use std::collections::HashMap;

fn input_val(connections: &[Connection], values: &HashMap<(NodeId, usize), PortValue>, node_id: NodeId, port: usize) -> f32 {
    Graph::static_input_value(connections, values, node_id, port).as_float()
}

fn is_connected(connections: &[Connection], node_id: NodeId, port: usize) -> bool {
    connections.iter().any(|c| c.to_node == node_id && c.to_port == port)
}

pub fn render(
    ui: &mut egui::Ui,
    uniform_names: &mut Vec<String>,
    uniform_types: &mut Vec<String>,
    uniform_values: &mut Vec<f32>,
    uniform_min: &mut Vec<f32>,
    uniform_max: &mut Vec<f32>,
    canvas_w: &mut f32,
    canvas_h: &mut f32,
    resolution: &mut u32,
    expanded: &mut bool,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
) {
    let input_code = Graph::static_input_value(connections, values, node_id, 0);
    let code = match &input_code { PortValue::Text(s) => s.clone(), _ => String::new() };

    // ── Canvas settings ──
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Canvas").small());
        ui.add(egui::DragValue::new(canvas_w).speed(10.0).prefix("W ").range(100.0..=1920.0));
        ui.add(egui::DragValue::new(canvas_h).speed(10.0).prefix("H ").range(100.0..=1080.0));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Resolution").small());
        ui.add(egui::Slider::new(resolution, 40..=300));
        if ui.button(if *expanded { "Collapse" } else { "Expand" }).clicked() {
            *expanded = !*expanded;
        }
    });

    ui.separator();

    // ── Uniforms ──
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Uniforms").small().strong());
        if ui.small_button("+").clicked() {
            let idx = uniform_names.len();
            uniform_names.push(format!("u{}", idx));
            uniform_types.push("float".to_string());
            uniform_values.push(0.0);
            uniform_min.push(0.0);
            uniform_max.push(1.0);
        }
    });

    let mut remove_idx: Option<usize> = None;
    let mut port_cursor: usize = 1;
    let mut effective_values: Vec<f32> = Vec::new();

    for i in 0..uniform_names.len() {
        while uniform_types.len() <= i { uniform_types.push("float".to_string()); }
        while uniform_values.len() <= i { uniform_values.push(0.0); }
        while uniform_min.len() <= i { uniform_min.push(0.0); }
        while uniform_max.len() <= i { uniform_max.push(1.0); }

        let utype = uniform_types[i].clone();
        let connected = is_connected(connections, node_id, port_cursor);

        ui.horizontal(|ui| {
            if ui.small_button("-").clicked() { remove_idx = Some(i); }
            ui.add(egui::TextEdit::singleline(&mut uniform_names[i]).desired_width(50.0));
            egui::ComboBox::from_id_salt(format!("utype_{}_{}", node_id, i))
                .selected_text(&utype).width(55.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut uniform_types[i], "float".to_string(), "float");
                    ui.selectable_value(&mut uniform_types[i], "range".to_string(), "range");
                    ui.selectable_value(&mut uniform_types[i], "color".to_string(), "color");
                });
        });

        match utype.as_str() {
            "range" => {
                let val = if connected { input_val(connections, values, node_id, port_cursor) } else { uniform_values[i] };
                uniform_values[i] = val;
                effective_values.push(val);
                if connected {
                    ui.label(egui::RichText::new(format!("  = {:.3}", val)).small().color(egui::Color32::from_rgb(100, 200, 255)));
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.add(egui::Slider::new(&mut uniform_values[i], uniform_min[i]..=uniform_max[i]));
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.add(egui::DragValue::new(&mut uniform_min[i]).speed(0.01).prefix("min "));
                        ui.add(egui::DragValue::new(&mut uniform_max[i]).speed(0.01).prefix("max "));
                    });
                }
                port_cursor += 1;
            }
            "color" => {
                let cr = if is_connected(connections, node_id, port_cursor) { input_val(connections, values, node_id, port_cursor) } else { uniform_values[i] };
                let cg = if is_connected(connections, node_id, port_cursor + 1) { input_val(connections, values, node_id, port_cursor + 1) } else { 0.0 };
                let cb = if is_connected(connections, node_id, port_cursor + 2) { input_val(connections, values, node_id, port_cursor + 2) } else { 0.0 };
                effective_values.push(cr);
                let any_conn = is_connected(connections, node_id, port_cursor) || is_connected(connections, node_id, port_cursor + 1) || is_connected(connections, node_id, port_cursor + 2);
                if any_conn {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(cr as u8, cg as u8, cb as u8));
                        ui.label(egui::RichText::new(format!("({:.0},{:.0},{:.0})", cr, cg, cb)).small().color(egui::Color32::from_rgb(100, 200, 255)));
                    });
                } else {
                    let mut c = egui::Color32::from_rgb(uniform_values[i] as u8, 128, 128);
                    ui.horizontal(|ui| { ui.add_space(20.0); ui.color_edit_button_srgba(&mut c); });
                    uniform_values[i] = c.r() as f32;
                }
                port_cursor += 3;
            }
            _ => {
                let val = if connected { input_val(connections, values, node_id, port_cursor) } else { uniform_values[i] };
                uniform_values[i] = val;
                effective_values.push(val);
                if connected {
                    ui.label(egui::RichText::new(format!("  = {:.3}", val)).small().color(egui::Color32::from_rgb(100, 200, 255)));
                } else {
                    ui.horizontal(|ui| { ui.add_space(20.0); ui.add(egui::DragValue::new(&mut uniform_values[i]).speed(0.01)); });
                }
                port_cursor += 1;
            }
        }
    }
    if let Some(idx) = remove_idx {
        uniform_names.remove(idx);
        if idx < uniform_types.len() { uniform_types.remove(idx); }
        if idx < uniform_values.len() { uniform_values.remove(idx); }
        if idx < uniform_min.len() { uniform_min.remove(idx); }
        if idx < uniform_max.len() { uniform_max.remove(idx); }
    }

    ui.separator();

    let has_code = !code.is_empty();

    // ── Preview — render to ColorImage then display as texture ──
    let display_w = if *expanded { ui.available_width().max(400.0) } else { ui.available_width().max(180.0) };
    let aspect = *canvas_h / canvas_w.max(1.0);
    let display_h = if *expanded { (display_w * aspect).max(300.0) } else { (display_w * aspect).min(200.0).max(100.0) };

    if has_code {
        let res = *resolution as usize;
        let res_y = (res as f32 * aspect) as usize;
        let res_x = res;

        // Build pixel buffer
        let mut pixels = Vec::with_capacity(res_x * res_y);
        let u_time = effective_values.first().copied().unwrap_or(0.0);
        let u1 = effective_values.get(1).copied().unwrap_or(1.0);
        let u2 = effective_values.get(2).copied().unwrap_or(128.0);
        let u3 = effective_values.get(3).copied().unwrap_or(128.0);
        let u4 = effective_values.get(4).copied().unwrap_or(255.0);

        let has_noise = code.contains("noise") || code.contains("hash");
        let has_circle = code.contains("circle") || code.contains("distance");
        let has_sdf = code.contains("sdBox") || code.contains("sdf");
        let has_mandelbrot = code.contains("andelbrot") || (code.contains("z.x * z.x") && code.contains("iter"));

        for py in 0..res_y {
            for px in 0..res_x {
                let uv_x = px as f32 / res_x as f32;
                let uv_y = py as f32 / res_y as f32;

                let (r, g, b) = if has_mandelbrot {
                    let zoom = u1.max(0.5);
                    let cx = effective_values.get(1).copied().unwrap_or(-0.5) + (uv_x * 2.0 - 1.0) / zoom;
                    let cy = effective_values.get(2).copied().unwrap_or(0.0) + (uv_y * 2.0 - 1.0) / zoom;
                    let mut zx = 0.0f32; let mut zy = 0.0f32; let mut i = 0u32;
                    for _ in 0..80 {
                        let nx = zx * zx - zy * zy + cx;
                        zy = 2.0 * zx * zy + cy; zx = nx;
                        if zx * zx + zy * zy > 4.0 { break; }
                        i += 1;
                    }
                    let t = i as f32 / 80.0;
                    ((t * 9.0 % 1.0 * 255.0) as u8, (t * 5.0 % 1.0 * 255.0) as u8, (t * 15.0 % 1.0 * 255.0) as u8)
                } else if has_noise {
                    let sx = uv_x * u1 + u_time * 0.5;
                    let sy = uv_y * u1 + u_time * 0.3;
                    let n = simple_noise(sx, sy);
                    ((u2 / 255.0 * n * 255.0).clamp(0.0, 255.0) as u8,
                     (u3 / 255.0 * n * 255.0).clamp(0.0, 255.0) as u8,
                     (u4 / 255.0 * n * 255.0).clamp(0.0, 255.0) as u8)
                } else if has_sdf {
                    let p = (uv_x * 2.0 - 1.0, uv_y * 2.0 - 1.0);
                    let angle = u_time * 0.5;
                    let (cs, sn) = (angle.cos(), angle.sin());
                    let rx = p.0 * cs - p.1 * sn;
                    let ry = p.0 * sn + p.1 * cs;
                    let thickness = u1.max(0.001);
                    let d1 = (sd_box(rx, ry, 0.4, 0.4).abs() - thickness).abs();
                    let d2 = ((p.0 * p.0 + p.1 * p.1).sqrt() - 0.5).abs() - thickness;
                    let s1 = smoothstep(0.02, 0.0, d1);
                    let s2 = smoothstep(0.02, 0.0, d2);
                    ((s1 * 200.0 + s2 * 50.0).clamp(0.0, 255.0) as u8,
                     (s2 * 180.0).clamp(0.0, 255.0) as u8,
                     (s1 * 80.0 + s2 * 230.0).clamp(0.0, 255.0) as u8)
                } else if has_circle {
                    let cx = 0.5 + u_time.sin() * 0.2;
                    let cy = 0.5 + u_time.cos() * 0.2;
                    let d = ((uv_x - cx).powi(2) + (uv_y - cy).powi(2)).sqrt();
                    let radius = u1.clamp(0.01, 2.0);
                    let circle = 1.0 - smoothstep(radius - 0.01, radius + 0.01, d);
                    ((circle * 50.0) as u8, (circle * 150.0) as u8, (circle * 255.0) as u8)
                } else {
                    let pulse = (u_time * 3.0).sin() * 0.5 + 0.5;
                    let d = ((uv_x - 0.5).powi(2) + (uv_y - 0.5).powi(2)).sqrt();
                    let ring = smoothstep(0.3 * pulse, 0.31 * pulse, d) - smoothstep(0.35 * pulse, 0.36 * pulse, d);
                    ((ring * pulse * 255.0) as u8, (ring * 128.0) as u8, (ring * (1.0 - pulse) * 255.0) as u8)
                };

                pixels.push(egui::Color32::from_rgb(r, g, b));
            }
        }

        // Create texture from pixels
        let image = egui::ColorImage {
            size: [res_x, res_y],
            pixels,
        };
        let texture = ui.ctx().load_texture(
            format!("wgsl_preview_{}", node_id),
            image,
            egui::TextureOptions {
                magnification: egui::TextureFilter::Linear,
                minification: egui::TextureFilter::Linear,
                ..Default::default()
            },
        );

        let (rect, _) = ui.allocate_exact_size(egui::vec2(display_w, display_h), egui::Sense::hover());
        ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);

        ui.painter().text(
            egui::pos2(rect.right() - 4.0, rect.top() + 4.0),
            egui::Align2::RIGHT_TOP,
            format!("{}x{}", res_x, res_y),
            egui::FontId::proportional(9.0),
            egui::Color32::from_rgba_unmultiplied(200, 200, 200, 120),
        );
    } else {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(display_w, display_h), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(40, 40, 40));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Connect WGSL source",
            egui::FontId::proportional(12.0), egui::Color32::from_rgb(120, 120, 120));
    }

    // ── Uniform declarations ──
    if !uniform_names.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("// Shader uniforms:").small().monospace().color(egui::Color32::GRAY));
        for (i, name) in uniform_names.iter().enumerate() {
            let utype = uniform_types.get(i).map(|s| s.as_str()).unwrap_or("float");
            let val = uniform_values.get(i).copied().unwrap_or(0.0);
            let decl = match utype {
                "color" => format!("var<uniform> {}: vec3<f32>;", name),
                _ => format!("var<uniform> {}: f32; // = {:.3}", name, val),
            };
            ui.label(egui::RichText::new(decl).small().monospace().color(egui::Color32::from_rgb(160, 180, 200)));
        }
    }

    // ── Code display ──
    if has_code {
        ui.separator();
        ui.label(egui::RichText::new(format!("{} chars", code.len())).small().color(egui::Color32::GRAY));
        let mut display = code;
        ui.add(egui::TextEdit::multiline(&mut display).font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY).desired_rows(6).interactive(false));
    }
}

// ── Math helpers ─────────────────────────────────────────────────────────

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn simple_noise(x: f32, y: f32) -> f32 {
    let ix = x.floor(); let iy = y.floor();
    let fx = x - ix; let fy = y - iy;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = simple_hash(ix, iy);
    let b = simple_hash(ix + 1.0, iy);
    let c = simple_hash(ix, iy + 1.0);
    let d = simple_hash(ix + 1.0, iy + 1.0);
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uy
}

fn simple_hash(x: f32, y: f32) -> f32 {
    let h = x * 127.1 + y * 311.7;
    let s = (h * 43758.5453123).sin();
    s - s.floor()
}

fn sd_box(px: f32, py: f32, bx: f32, by: f32) -> f32 {
    let dx = px.abs() - bx;
    let dy = py.abs() - by;
    (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0)
}
