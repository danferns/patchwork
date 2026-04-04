// GPU-accelerated image processing using wgpu
// Upload images as textures → blend/effect via WGSL shader → display or readback

use crate::graph::{ImageData, NodeId};
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use std::collections::HashMap;
use std::sync::Arc;

// ── Constants ────────────────────────────────────────────────────────────────

const BLEND_SHADER: &str = r#"
struct Params {
    mode: u32,
    mix: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var tex_a: texture_2d<f32>;
@group(0) @binding(2) var tex_b: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

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

fn blend_colors(a: vec4<f32>, b: vec4<f32>, mode: u32) -> vec4<f32> {
    switch mode {
        case 0u: { return b; }                                          // Normal
        case 1u: { return a * b; }                                      // Multiply
        case 2u: { return vec4(1.0) - (vec4(1.0) - a) * (vec4(1.0) - b); } // Screen
        case 3u: {                                                       // Overlay
            let r = select(2.0 * a.r * b.r, 1.0 - 2.0 * (1.0-a.r) * (1.0-b.r), a.r > 0.5);
            let g = select(2.0 * a.g * b.g, 1.0 - 2.0 * (1.0-a.g) * (1.0-b.g), a.g > 0.5);
            let bb = select(2.0 * a.b * b.b, 1.0 - 2.0 * (1.0-a.b) * (1.0-b.b), a.b > 0.5);
            return vec4(r, g, bb, max(a.a, b.a));
        }
        case 4u: { return clamp(a + b, vec4(0.0), vec4(1.0)); }        // Add
        case 5u: { return abs(a - b); }                                 // Difference
        case 6u: {                                                       // Soft Light
            let r = select((2.0*b.r - 1.0) * (a.r - a.r*a.r) + a.r, (2.0*b.r) * a.r + a.r*a.r*(1.0-2.0*b.r), b.r <= 0.5);
            let g = select((2.0*b.g - 1.0) * (a.g - a.g*a.g) + a.g, (2.0*b.g) * a.g + a.g*a.g*(1.0-2.0*b.g), b.g <= 0.5);
            let bb = select((2.0*b.b - 1.0) * (a.b - a.b*a.b) + a.b, (2.0*b.b) * a.b + a.b*a.b*(1.0-2.0*b.b), b.b <= 0.5);
            return vec4(r, g, bb, max(a.a, b.a));
        }
        case 7u: {                                                       // Hard Light
            let r = select(1.0 - 2.0*(1.0-a.r)*(1.0-b.r), 2.0*a.r*b.r, b.r <= 0.5);
            let g = select(1.0 - 2.0*(1.0-a.g)*(1.0-b.g), 2.0*a.g*b.g, b.g <= 0.5);
            let bb = select(1.0 - 2.0*(1.0-a.b)*(1.0-b.b), 2.0*a.b*b.b, b.b <= 0.5);
            return vec4(r, g, bb, max(a.a, b.a));
        }
        default: { return b; }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let a = textureSample(tex_a, tex_sampler, in.uv);
    let b = textureSample(tex_b, tex_sampler, in.uv);
    let blended = blend_colors(a, b, p.mode);
    return mix(a, blended, p.mix);
}
"#;

// ── GPU Resources ────────────────────────────────────────────────────────────

struct GpuBlendResources {
    pipeline: wgpu::RenderPipeline,
    display_pipeline: wgpu::RenderPipeline,
    display_bind_group_layout: wgpu::BindGroupLayout,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
}

#[allow(dead_code)]
struct GpuBlendInstance {
    _bind_group: wgpu::BindGroup,
    output_texture: wgpu::Texture,
    _output_view: wgpu::TextureView,
    display_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// Stored in egui_wgpu callback resources
pub struct GpuBlendStore {
    resources: Option<GpuBlendResources>,
    instances: HashMap<NodeId, GpuBlendInstance>,
}

pub struct GpuBlendCallback {
    pub node_id: NodeId,
    pub mode: u32,
    pub mix: f32,
    pub img_a: Arc<ImageData>,
    pub img_b: Arc<ImageData>,
    pub target_format: wgpu::TextureFormat,
}

impl egui_wgpu::CallbackTrait for GpuBlendCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let store = callback_resources.entry::<GpuBlendStore>().or_insert_with(|| GpuBlendStore {
            resources: None,
            instances: HashMap::new(),
        });

        // Initialize pipeline on first use
        if store.resources.is_none() {
            store.resources = Some(create_blend_pipeline(device, self.target_format));
        }
        let res = match store.resources.as_ref() {
            Some(r) => r,
            None => return Vec::new(), // Pipeline creation failed — skip this frame
        };

        let w = self.img_a.width.max(self.img_b.width);
        let h = self.img_a.height.max(self.img_b.height);

        // Upload params
        let params = [self.mode, self.mix.to_bits(), 0u32, 0u32];
        queue.write_buffer(&res.uniform_buffer, 0, bytemuck::cast_slice(&params));

        // Create/update textures
        let tex_a = upload_texture(device, queue, &self.img_a, "blend_a");
        let tex_b = upload_texture(device, queue, &self.img_b, "blend_b");
        let view_a = tex_a.create_view(&Default::default());
        let view_b = tex_b.create_view(&Default::default());

        // Create output render target
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blend_output"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&Default::default());

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blend_bind_group"),
            layout: &res.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: res.uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view_a) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view_b) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&res.sampler) },
            ],
        });

        // Render blend to output texture
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blend_offscreen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&res.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // Create display bind group to sample the output texture
        let display_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blend_display_bg"),
            layout: &res.display_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&output_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&res.sampler) },
            ],
        });

        store.instances.insert(self.node_id, GpuBlendInstance {
            _bind_group: bind_group,
            output_texture,
            _output_view: output_view,
            display_bind_group,
            width: w,
            height: h,
        });

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(store) = callback_resources.get::<GpuBlendStore>() {
            if let (Some(res), Some(inst)) = (&store.resources, store.instances.get(&self.node_id)) {
                render_pass.set_pipeline(&res.display_pipeline);
                render_pass.set_bind_group(0, &inst.display_bind_group, &[]);
                render_pass.draw(0..3, 0..1);
            }
        }
    }
}

// ── Helper Functions ─────────────────────────────────────────────────────────

const DISPLAY_SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(tex, tex_sampler, in.uv);
}
"#;

fn create_blend_pipeline(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> GpuBlendResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blend_shader"),
        source: wgpu::ShaderSource::Wgsl(BLEND_SHADER.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blend_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blend_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blend_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blend_params"),
        size: 16, // 4 × u32
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("blend_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // Display pipeline: samples a single texture and renders to screen
    let display_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("display_shader"),
        source: wgpu::ShaderSource::Wgsl(DISPLAY_SHADER.into()),
    });

    let display_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("display_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let display_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("display_pipeline_layout"),
        bind_group_layouts: &[&display_bind_group_layout],
        push_constant_ranges: &[],
    });

    let display_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("display_pipeline"),
        layout: Some(&display_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &display_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &display_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    GpuBlendResources { pipeline, display_pipeline, display_bind_group_layout, bind_group_layout, sampler, uniform_buffer }
}

pub fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, img: &ImageData, label: &str) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &img.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(img.width * 4),
            rows_per_image: Some(img.height),
        },
        wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
    );

    texture
}

/// Readback GPU texture to CPU ImageData (blocking)
#[allow(dead_code)]
pub fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Arc<ImageData> {
    let bytes_per_row = (width * 4 + 255) & !255; // Align to 256
    let buf_size = (bytes_per_row * height) as u64;

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback_staging"),
        size: buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
    device.poll(wgpu::Maintain::Wait);
    let _ = rx.recv();

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&data[start..end]);
    }

    Arc::new(ImageData { width, height, pixels })
}
