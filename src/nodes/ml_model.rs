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
    let (model_path, labels_path, confidence, result_text, status, last_input_hash) = match node_type {
        NodeType::MlModel { model_path, labels_path, confidence, result_text, status, last_input_hash } => {
            (model_path, labels_path, confidence, result_text, status, last_input_hash)
        }
        _ => return,
    };

    // Model file selector
    ui.horizontal(|ui| {
        ui.label("Model:");
        if ui.button("Load .onnx").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("ONNX", &["onnx"])
                .pick_file()
            {
                *model_path = p.display().to_string();
                *status = "Model loaded".into();
            }
        }
    });
    if !model_path.is_empty() {
        let short = if model_path.len() > 30 {
            format!("...{}", &model_path[model_path.len()-30..])
        } else {
            model_path.clone()
        };
        ui.label(egui::RichText::new(short).small().monospace());
    }

    // Labels file (optional)
    ui.horizontal(|ui| {
        ui.label("Labels:");
        if ui.button("Load .txt").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Text", &["txt", "csv"])
                .pick_file()
            {
                *labels_path = p.display().to_string();
            }
        }
    });
    if !labels_path.is_empty() {
        let short = if labels_path.len() > 30 {
            format!("...{}", &labels_path[labels_path.len()-30..])
        } else {
            labels_path.clone()
        };
        ui.label(egui::RichText::new(short).small().monospace());
    }

    // Confidence threshold
    ui.horizontal(|ui| {
        ui.label("Threshold:");
        ui.add(egui::Slider::new(confidence, 0.01..=1.0).step_by(0.01));
    });

    // Status
    if !status.is_empty() {
        let color = if status.starts_with("Error") || status.starts_with("error") {
            egui::Color32::from_rgb(255, 100, 100)
        } else if status.contains("Running") || status.contains("running") {
            egui::Color32::from_rgb(200, 200, 80)
        } else {
            egui::Color32::from_rgb(80, 200, 80)
        };
        ui.colored_label(color, egui::RichText::new(&*status).small());
    }

    // Show result
    if !result_text.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("Results:").small().strong());
        for line in result_text.lines().take(10) {
            ui.label(egui::RichText::new(line).small().monospace());
        }
    }

    // Check for input image and trigger inference
    let input_val = Graph::static_input_value(connections, values, node_id, 0);
    if let PortValue::Image(img) = &input_val {
        // Hash input to detect changes
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        img.width.hash(&mut h);
        img.height.hash(&mut h);
        if img.pixels.len() >= 32 {
            img.pixels[..16].hash(&mut h);
            img.pixels[img.pixels.len()-16..].hash(&mut h);
        }
        let hash = h.finish();

        if hash != *last_input_hash && !model_path.is_empty() {
            *last_input_hash = hash;
            *status = "Running inference...".into();

            // Schedule background inference
            let inference_id = egui::Id::new(("ml_inference", node_id));
            ui.ctx().data_mut(|d| d.insert_temp(inference_id, MlInferenceRequest {
                model_path: model_path.clone(),
                labels_path: labels_path.clone(),
                confidence: *confidence,
                image: img.clone(),
                node_id,
            }));
        }
    }
}

/// Request for background inference
#[derive(Clone)]
pub struct MlInferenceRequest {
    pub model_path: String,
    pub labels_path: String,
    pub confidence: f32,
    pub image: Arc<ImageData>,
    pub node_id: NodeId,
}

/// Result from background inference
pub struct MlInferenceResult {
    pub node_id: NodeId,
    pub result_text: String,
    pub status: String,
}

/// Run ONNX inference on an image (called from background thread)
pub fn run_inference(req: &MlInferenceRequest) -> MlInferenceResult {
    match run_inference_inner(req) {
        Ok(text) => MlInferenceResult {
            node_id: req.node_id,
            result_text: text,
            status: "Done".into(),
        },
        Err(e) => MlInferenceResult {
            node_id: req.node_id,
            result_text: String::new(),
            status: format!("Error: {}", e),
        },
    }
}

fn run_inference_inner(req: &MlInferenceRequest) -> Result<String, String> {
    use ort::session::Session;

    // Load model
    let session = Session::builder()
        .map_err(|e| format!("Session builder: {}", e))?
        .commit_from_file(&req.model_path)
        .map_err(|e| format!("Load model: {}", e))?;

    // Get input name
    let input_name = session.inputs().first().ok_or("No inputs in model")?.name().to_string();

    // Most classification models expect [1, 3, 224, 224] (NCHW) or [1, 224, 224, 3] (NHWC)
    // We'll resize to 224x224 and try NCHW format
    let target_size = 224usize;

    // Resize image to target_size x target_size using simple bilinear
    let resized = resize_image(&req.image, target_size as u32, target_size as u32);

    // Convert to float [0, 1] and normalize with ImageNet mean/std
    let mean = [0.485f32, 0.456, 0.406];
    let std_dev = [0.229f32, 0.224, 0.225];

    let mut input_data = vec![0.0f32; 3 * target_size * target_size];
    for y in 0..target_size {
        for x in 0..target_size {
            let idx = (y * target_size + x) * 4;
            let r = resized[idx] as f32 / 255.0;
            let g = resized[idx + 1] as f32 / 255.0;
            let b = resized[idx + 2] as f32 / 255.0;
            // NCHW format
            input_data[0 * target_size * target_size + y * target_size + x] = (r - mean[0]) / std_dev[0];
            input_data[1 * target_size * target_size + y * target_size + x] = (g - mean[1]) / std_dev[1];
            input_data[2 * target_size * target_size + y * target_size + x] = (b - mean[2]) / std_dev[2];
        }
    }

    // Create input tensor [1, 3, 224, 224]
    let shape = vec![1usize, 3, target_size, target_size];
    let input_tensor = ort::value::Tensor::from_array((shape, input_data.into_boxed_slice()))
        .map_err(|e| format!("Create tensor: {}", e))?;

    // Run inference
    let mut session = session;
    let outputs = session.run(ort::inputs![&input_name => input_tensor])
        .map_err(|e| format!("Run: {}", e))?;

    // Get output tensor - extract raw f32 data
    let output = &outputs[0];
    let (_shape, data) = output.try_extract_tensor::<f32>()
        .map_err(|e| format!("Extract: {}", e))?;
    let scores: Vec<f32> = data.to_vec();

    // Apply softmax
    let max_score = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_scores: Vec<f32> = scores.iter().map(|s| (s - max_score).exp()).collect();
    let sum: f32 = exp_scores.iter().sum();
    let probs: Vec<f32> = exp_scores.iter().map(|s| s / sum).collect();

    // Load labels if available
    let labels: Vec<String> = if !req.labels_path.is_empty() {
        std::fs::read_to_string(&req.labels_path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .collect()
    } else {
        (0..probs.len()).map(|i| format!("class_{}", i)).collect()
    };

    // Get top results above confidence threshold
    let mut indexed: Vec<(usize, f32)> = probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut result = String::new();
    for (i, prob) in indexed.iter().take(5) {
        if *prob < req.confidence { break; }
        let label: &str = labels.get(*i).map(|s: &String| s.as_str()).unwrap_or("unknown");
        result.push_str(&format!("{}: {:.1}%\n", label, prob * 100.0));
    }

    if result.is_empty() {
        result = format!("No results above {:.0}% threshold", req.confidence * 100.0);
    }

    Ok(result)
}

/// Simple bilinear resize of RGBA image
fn resize_image(img: &ImageData, new_w: u32, new_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (new_w * new_h * 4) as usize];
    let x_ratio = img.width as f32 / new_w as f32;
    let y_ratio = img.height as f32 / new_h as f32;

    for y in 0..new_h {
        for x in 0..new_w {
            let src_x = (x as f32 * x_ratio).min(img.width as f32 - 1.0);
            let src_y = (y as f32 * y_ratio).min(img.height as f32 - 1.0);
            let sx = src_x as u32;
            let sy = src_y as u32;
            let src_idx = ((sy * img.width + sx) * 4) as usize;
            let dst_idx = ((y * new_w + x) * 4) as usize;
            if src_idx + 3 < img.pixels.len() && dst_idx + 3 < out.len() {
                out[dst_idx] = img.pixels[src_idx];
                out[dst_idx + 1] = img.pixels[src_idx + 1];
                out[dst_idx + 2] = img.pixels[src_idx + 2];
                out[dst_idx + 3] = img.pixels[src_idx + 3];
            }
        }
    }
    out
}
