use crate::graph::*;
use eframe::egui;
use std::collections::{BTreeSet, HashMap};

/// Detect single uppercase letters A-Z used as variables in the formula
fn detect_variables(formula: &str) -> Vec<char> {
    let mut vars = BTreeSet::new();
    let chars: Vec<char> = formula.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() {
            // Check it's not part of a function name (preceded/followed by lowercase)
            let prev_alpha = i > 0 && chars[i - 1].is_ascii_alphabetic();
            let next_alpha = i + 1 < chars.len() && chars[i + 1].is_ascii_alphabetic();
            if !prev_alpha && !next_alpha {
                vars.insert(ch);
            }
        }
    }
    vars.into_iter().collect()
}

/// Evaluate formula using Rhai with variable substitution
fn evaluate_formula(formula: &str, var_values: &HashMap<char, f64>) -> Result<f64, String> {
    let mut engine = rhai::Engine::new();

    // Add helper functions
    engine.register_fn("map", |val: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64| -> f64 {
        let t = (val - in_min) / (in_max - in_min);
        out_min + t * (out_max - out_min)
    });
    engine.register_fn("lerp", |a: f64, b: f64, t: f64| -> f64 { a + (b - a) * t });
    engine.register_fn("clamp", |val: f64, min: f64, max: f64| -> f64 { val.max(min).min(max) });
    engine.register_fn("sign", |val: f64| -> f64 { if val > 0.0 { 1.0 } else if val < 0.0 { -1.0 } else { 0.0 } });
    engine.register_fn("deg", |rad: f64| -> f64 { rad * 180.0 / std::f64::consts::PI });
    engine.register_fn("rad", |deg: f64| -> f64 { deg * std::f64::consts::PI / 180.0 });

    // Build Rhai expression: replace A → a_val, B → b_val, etc.
    let mut expr = formula.to_string();
    // Replace variable names with Rhai-safe identifiers (must be done carefully)
    // Sort by length desc to avoid partial replacements
    let mut var_names: Vec<char> = var_values.keys().copied().collect();
    var_names.sort();

    for &ch in var_names.iter().rev() {
        // Only replace standalone letters (not inside function names)
        let from = ch.to_string();
        let to = format!("_var_{}_", ch.to_ascii_lowercase());
        // Simple replacement — assumes single uppercase letters are variables
        let mut result = String::new();
        let chars_vec: Vec<char> = expr.chars().collect();
        let mut i = 0;
        while i < chars_vec.len() {
            if chars_vec[i] == ch {
                let prev_alpha = i > 0 && chars_vec[i - 1].is_ascii_alphabetic();
                let next_alpha = i + 1 < chars_vec.len() && chars_vec[i + 1].is_ascii_alphabetic();
                if !prev_alpha && !next_alpha {
                    result.push_str(&to);
                } else {
                    result.push(chars_vec[i]);
                }
            } else {
                result.push(chars_vec[i]);
            }
            i += 1;
        }
        expr = result;
    }

    // Replace × and ÷ with * and /
    expr = expr.replace('×', "*").replace('÷', "/");

    // Add PI, TAU, E constants
    let mut scope = rhai::Scope::new();
    scope.push_constant("PI", std::f64::consts::PI);
    scope.push_constant("TAU", std::f64::consts::TAU);
    scope.push_constant("E", std::f64::consts::E);

    // Add variable values
    for (&ch, &val) in var_values {
        let name = format!("_var_{}_", ch.to_ascii_lowercase());
        scope.push(&name, val);
    }

    match engine.eval_expression_with_scope::<rhai::Dynamic>(&mut scope, &expr) {
        Ok(val) => {
            if let Some(f) = val.as_float().ok() { Ok(f) }
            else if let Some(i) = val.as_int().ok() { Ok(i as f64) }
            else { Err("Non-numeric result".into()) }
        }
        Err(e) => Err(format!("{}", e)),
    }
}

const PRESETS: &[(&str, &str)] = &[
    ("+", "A + B"),
    ("−", "A - B"),
    ("×", "A * B"),
    ("÷", "A / B"),
    ("%", "A % B"),
    ("sin", "sin(A)"),
    ("cos", "cos(A)"),
    ("tan", "tan(A)"),
    ("pow", "A.pow(B)"),
    ("sqrt", "A.sqrt()"),
    ("abs", "A.abs()"),
    ("min", "if A < B { A } else { B }"),
    ("max", "if A > B { A } else { B }"),
    ("clamp", "clamp(A, 0.0, 1.0)"),
    ("map", "map(A, 0.0, 1.0, 0.0, 100.0)"),
    ("lerp", "lerp(A, B, C)"),
    ("round", "A.round()"),
    ("floor", "A.floor()"),
    ("ceil", "A.ceiling()"),
];

pub fn render(
    ui: &mut egui::Ui,
    formula: &mut String,
    variables: &mut Vec<char>,
    result: &mut f64,
    error: &mut String,
    node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    port_positions: &mut HashMap<(NodeId, usize, bool), egui::Pos2>,
    dragging_from: &mut Option<(NodeId, usize, bool)>,
) {
    // ── Input ports (one per detected variable) ─────────────
    let mut var_values: HashMap<char, f64> = HashMap::new();
    for (i, &ch) in variables.iter().enumerate() {
        let is_wired = connections.iter().any(|c| c.to_node == node_id && c.to_port == i);
        let val = if is_wired {
            Graph::static_input_value(connections, values, node_id, i).as_float() as f64
        } else {
            0.0
        };
        var_values.insert(ch, val);

        ui.horizontal(|ui| {
            // Port circle
            let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click_and_drag());
            let (fill, border) = if response.hovered() || response.dragged() {
                (egui::Color32::YELLOW, egui::Color32::WHITE)
            } else if is_wired {
                (egui::Color32::from_rgb(60, 140, 255), egui::Color32::from_rgb(120, 180, 255))
            } else {
                (egui::Color32::from_rgb(70, 75, 85), egui::Color32::from_rgb(120, 125, 135))
            };
            ui.painter().circle_filled(rect.center(), 5.0, fill);
            ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.5, border));
            port_positions.insert((node_id, i, true), rect.center());
            if response.drag_started() {
                if let Some(existing) = connections.iter().find(|c| c.to_node == node_id && c.to_port == i) {
                    *dragging_from = Some((existing.from_node, existing.from_port, true));
                } else {
                    *dragging_from = Some((node_id, i, false));
                }
            }

            ui.label(egui::RichText::new(format!("{}", ch)).strong());
            ui.label(egui::RichText::new(format!("{:.3}", val)).monospace().small().color(egui::Color32::GRAY));
        });
    }

    if !variables.is_empty() { ui.separator(); }

    // ── Formula display/edit ────────────────────────────────
    let old_formula = formula.clone();
    ui.horizontal(|ui| {
        ui.label("ƒ");
        ui.add(
            egui::TextEdit::singleline(formula)
                .desired_width(ui.available_width() - 4.0)
                .font(egui::FontId::monospace(13.0))
                .hint_text("A + B"),
        );
    });

    // Update variables if formula changed
    if *formula != old_formula {
        *variables = detect_variables(formula);
    }

    // ── Evaluate ────────────────────────────────────────────
    if !formula.is_empty() {
        match evaluate_formula(formula, &var_values) {
            Ok(val) => {
                *result = val;
                error.clear();
            }
            Err(e) => {
                *error = e;
            }
        }
    }

    // ── Result display ──────────────────────────────────────
    if error.is_empty() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("= {:.4}", result)).monospace().strong().size(15.0));

            // Output port
            let remaining = ui.available_width() - 14.0;
            if remaining > 0.0 { ui.add_space(remaining); }
            let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click_and_drag());
            let (fill, border) = if response.hovered() || response.dragged() {
                (egui::Color32::YELLOW, egui::Color32::WHITE)
            } else {
                (egui::Color32::from_rgb(60, 140, 255), egui::Color32::from_rgb(120, 180, 255))
            };
            ui.painter().circle_filled(rect.center(), 5.0, fill);
            ui.painter().circle_stroke(rect.center(), 5.0, egui::Stroke::new(1.5, border));
            port_positions.insert((node_id, 0, false), rect.center());
            if response.drag_started() {
                *dragging_from = Some((node_id, 0, true));
            }
        });
    } else {
        ui.colored_label(egui::Color32::from_rgb(255, 100, 100),
            egui::RichText::new(&*error).small());
    }

    // ── Quick presets (collapsible) ─────────────────────────
    ui.collapsing("Quick Formula", |ui| {
        let cols = 5;
        egui::Grid::new(egui::Id::new(("math_presets", node_id)))
            .num_columns(cols)
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for (i, (label, preset_formula)) in PRESETS.iter().enumerate() {
                    if ui.small_button(*label).on_hover_text(*preset_formula).clicked() {
                        *formula = preset_formula.to_string();
                        *variables = detect_variables(formula);
                    }
                    if (i + 1) % cols == 0 { ui.end_row(); }
                }
            });
    });
}
