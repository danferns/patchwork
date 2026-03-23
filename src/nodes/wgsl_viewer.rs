use crate::graph::*;
use eframe::egui;
use eframe::egui_wgpu;
use std::collections::HashMap;

/// Built-in vertex shader — only used if user doesn't provide their own
const BUILTIN_VERTEX_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    return out;
}
"#;

pub fn render(
    ui: &mut egui::Ui,
    wgsl_code: &mut String,
    _node_id: NodeId,
    values: &HashMap<(NodeId, usize), PortValue>,
    connections: &[Connection],
    wgpu_render_state: &Option<egui_wgpu::RenderState>,
) {
    let input_code = Graph::static_input_value(connections, values, _node_id, 0);
    let code = match &input_code {
        PortValue::Text(s) => s.clone(),
        _ => String::new(),
    };

    *wgsl_code = code.clone();

    if code.is_empty() {
        ui.colored_label(egui::Color32::GRAY, "Connect WGSL code to render");
        return;
    }

    // Show code preview (collapsible)
    ui.collapsing("Shader Code", |ui| {
        let preview = if code.len() > 500 {
            format!("{}...", &code[..500])
        } else {
            code.clone()
        };
        let mut p = preview;
        ui.code_editor(&mut p);
    });

    // Detect if user provides their own vertex shader
    let has_vertex_shader = code.contains("@vertex");

    // Build final shader code
    let final_code = if has_vertex_shader {
        // User provides full shader (vertex + fragment) — use as-is
        code.clone()
    } else {
        // User provides fragment only — prepend built-in vertex shader
        format!("{}\n{}", BUILTIN_VERTEX_SHADER, code)
    };

    // Detect vertex count: if user has array<vec2<f32>, 6> or similar quad, use 6; otherwise 3
    let vertex_count = if has_vertex_shader && code.contains("array<") {
        // Try to detect vertex count from array declaration
        if code.contains(", 6>") || code.contains(",6>") { 6u32 } else { 3u32 }
    } else {
        3 // built-in fullscreen triangle
    };

    // Validate with naga
    match validate_wgsl_naga(&final_code) {
        Err(e) => {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), format!("❌ {}", e));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(300.0, 200.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(40, 15, 15));
        }
        Ok(()) => {
            if let Some(render_state) = wgpu_render_state {
                render_with_wgpu(ui, &final_code, render_state, vertex_count);
            } else {
                ui.colored_label(egui::Color32::YELLOW, "⚠ No WGPU render state");
            }
        }
    }
}

fn render_with_wgpu(
    ui: &mut egui::Ui,
    shader_code: &str,
    render_state: &egui_wgpu::RenderState,
    vertex_count: u32,
) {
    use egui_wgpu::wgpu;

    let device = &render_state.device;
    let target_format = render_state.target_format;

    // Use error scopes to catch GPU errors without panicking
    device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgsl_user_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_code.into()),
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgsl_user_pipeline"),
        layout: None, // auto layout from shader
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(target_format.into())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    // Check for GPU compilation errors
    let error = pollster::block_on(device.pop_error_scope());

    if let Some(err) = error {
        ui.colored_label(
            egui::Color32::from_rgb(255, 100, 100),
            format!("❌ GPU: {}", err),
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(300.0, 200.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(40, 15, 15));
        return;
    }

    // Store pipeline in callback resources
    {
        let mut renderer = render_state.renderer.write();
        renderer.callback_resources.insert(WgslPaintResources {
            pipeline,
            vertex_count,
        });
    }

    ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "✅ Rendering");

    let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 300.0), egui::Sense::hover());

    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        WgslPaintCallback,
    ));
}

struct WgslPaintResources {
    pipeline: egui_wgpu::wgpu::RenderPipeline,
    vertex_count: u32,
}

struct WgslPaintCallback;

impl egui_wgpu::CallbackTrait for WgslPaintCallback {
    fn prepare(
        &self,
        _device: &egui_wgpu::wgpu::Device,
        _queue: &egui_wgpu::wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut egui_wgpu::wgpu::CommandEncoder,
        _resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<egui_wgpu::wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(res) = resources.get::<WgslPaintResources>() {
            render_pass.set_pipeline(&res.pipeline);
            render_pass.draw(0..res.vertex_count, 0..1);
        }
    }
}

fn validate_wgsl_naga(code: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(code)
        .map_err(|e| format!("Parse: {}", e))?;

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .map_err(|e| format!("Validation: {}", e))?;

    Ok(())
}
