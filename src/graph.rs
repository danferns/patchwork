use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

fn default_true() -> bool { true }
fn default_bg_color() -> [u8; 3] { [20, 20, 20] }
fn default_text_color() -> [u8; 3] { [220, 220, 220] }
fn default_window_bg() -> [u8; 3] { [24, 24, 24] }
fn default_window_alpha() -> u8 { 240 }
fn default_grid_color() -> [u8; 3] { [28, 28, 28] }
fn default_slider_step() -> f32 { 0.01 }
fn default_slider_color() -> [u8; 3] { [80, 160, 255] }
fn default_grid_style() -> u8 { 2 } // Default: Dotted
fn default_rounding() -> f32 { 16.0 }
fn default_spacing() -> f32 { 4.0 }
fn default_wire_thickness() -> f32 { 6.0 }
fn default_display_color() -> [u8; 3] { [80, 200, 120] }
fn default_comment_color() -> [u8; 3] { [45, 45, 50] }
fn default_scope_history() -> Vec<f32> { Vec::new() }
fn default_scope_length() -> usize { 200 }
fn default_scope_min() -> f32 { 0.0 }
fn default_scope_max() -> f32 { 1.0 }
fn default_scope_height() -> f32 { 80.0 }
fn default_canvas_w() -> f32 { 400.0 }
fn default_canvas_h() -> f32 { 300.0 }
fn default_wiggle_range() -> f32 { 1.0 }
fn default_wiggle_speed() -> f32 { 1.0 }
fn default_resolution() -> u32 { 120 }
fn default_max_tokens() -> u32 { 1024 }
fn default_provider() -> String { "anthropic".into() }
fn default_model() -> String { "claude-sonnet-4-20250514".into() }
fn default_temperature() -> f32 { 0.7 }

pub type NodeId = u64;

// ── ML Model Presets ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MlPreset {
    /// ImageNet-style classification (softmax over classes). Input: 224×224, NCHW, ImageNet norm.
    #[default]
    Classification,
    /// YOLO-style object detection. Outputs bounding boxes + class + confidence.
    /// Input: 640×640, NCHW, 0–1 norm. Output: [1, N, 5+classes] or [1, 5+classes, N].
    ObjectDetection,
    /// Pose estimation (e.g., MoveNet, MediaPipe Pose). Outputs keypoints.
    /// Input: 192×192 or 256×256, NHWC or NCHW, 0–1 norm.
    PoseEstimation,
    /// Custom model — user sets input size and normalization manually.
    Custom,
}

impl MlPreset {
    pub fn all() -> &'static [MlPreset] {
        &[MlPreset::Classification, MlPreset::ObjectDetection, MlPreset::PoseEstimation, MlPreset::Custom]
    }

    pub fn name(&self) -> &'static str {
        match self {
            MlPreset::Classification => "Classification",
            MlPreset::ObjectDetection => "Object Detection",
            MlPreset::PoseEstimation => "Pose Estimation",
            MlPreset::Custom => "Custom",
        }
    }

    /// Default input size for this preset
    pub fn input_size(&self) -> u32 {
        match self {
            MlPreset::Classification => 224,
            MlPreset::ObjectDetection => 640,
            MlPreset::PoseEstimation => 128,  // MediaPipe pose detector uses 128
            MlPreset::Custom => 224,
        }
    }

    /// Whether to use ImageNet normalization (mean/std) vs simple 0–1
    pub fn imagenet_norm(&self) -> bool {
        matches!(self, MlPreset::Classification)
    }

    /// Whether the model expects NHWC [1, H, W, 3] instead of NCHW [1, 3, H, W].
    /// Most ONNX models use NCHW. If wrong, the auto-detect retry will correct it.
    pub fn is_nhwc(&self) -> bool {
        false // Default to NCHW; auto-detect handles the rest
    }
}

// ── Image Data ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA8, 4 bytes per pixel
}

impl ImageData {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self { width, height, pixels }
    }
    #[allow(dead_code)]
    pub fn solid(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let pixels = vec![r, g, b, a].repeat((width * height) as usize);
        Self { width, height, pixels }
    }
}

// ── Port Value ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PortValue {
    Float(f32),
    Text(String),
    Image(Arc<ImageData>),
    None,
}

// Custom serde: Image serializes as None (pixel data is runtime-only)
impl Serialize for PortValue {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            PortValue::Float(v) => {
                use serde::ser::SerializeMap;
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("Float", v)?;
                m.end()
            }
            PortValue::Text(v) => {
                use serde::ser::SerializeMap;
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("Text", v)?;
                m.end()
            }
            PortValue::Image(_) | PortValue::None => {
                s.serialize_str("None")
            }
        }
    }
}

impl<'de> Deserialize<'de> for PortValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match &v {
            serde_json::Value::Object(m) => {
                if let Some(f) = m.get("Float") {
                    Ok(PortValue::Float(f.as_f64().unwrap_or(0.0) as f32))
                } else if let Some(t) = m.get("Text") {
                    Ok(PortValue::Text(t.as_str().unwrap_or("").to_string()))
                } else {
                    Ok(PortValue::None)
                }
            }
            _ => Ok(PortValue::None),
        }
    }
}

impl PartialEq for PortValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PortValue::Float(a), PortValue::Float(b)) => a == b,
            (PortValue::Text(a), PortValue::Text(b)) => a == b,
            (PortValue::Image(a), PortValue::Image(b)) => Arc::ptr_eq(a, b),
            (PortValue::None, PortValue::None) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for PortValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortValue::Float(v) => write!(f, "{:.3}", v),
            PortValue::Text(s) => {
                if s.len() > 24 { write!(f, "\"{}...\"", &s[..24]) }
                else { write!(f, "\"{}\"", s) }
            }
            PortValue::Image(img) => write!(f, "[Image {}x{}]", img.width, img.height),
            PortValue::None => write!(f, "\u{2014}"),
        }
    }
}

impl PortValue {
    pub fn as_float(&self) -> f32 {
        match self { PortValue::Float(v) => *v, _ => 0.0 }
    }
    pub fn as_image(&self) -> Option<&Arc<ImageData>> {
        match self { PortValue::Image(img) => Some(img), _ => None }
    }
}

/// Semantic type of a port — drives visual shape, color, brightness behavior, and type hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortKind {
    /// Generic continuous float value (slider, math result, frequency, etc.)
    Number,
    /// Float known to be 0.0–1.0 (mix, phase, opacity, progress)
    Normalized,
    /// Momentary pulse 0→1→0 (timer trigger, key press, gate pass)
    Trigger,
    /// Sustained on/off boolean (toggle, active, running)
    Gate,
    /// String data (text, file path, JSON, URL)
    Text,
    /// Pixel bitmap (RGBA image, video frame)
    Image,
    /// Audio signal routing (synth→fx→speaker)
    Audio,
    /// Individual color channel (R, G, or B, 0–255)
    Color,
    /// Unknown / any type (fallback)
    Generic,
}

impl PortKind {
    /// Base color for this port kind (used for port fill and wire color)
    pub fn base_color(&self) -> [u8; 3] {
        match self {
            Self::Number      => [80, 100, 230],    // blue
            Self::Normalized  => [60, 160, 230],    // cyan-blue
            Self::Trigger     => [255, 160, 40],    // orange
            Self::Gate        => [220, 180, 60],    // amber
            Self::Text        => [60, 220, 80],     // green
            Self::Image       => [200, 30, 255],    // purple
            Self::Audio       => [255, 220, 40],    // yellow
            Self::Color       => [220, 220, 220],   // white (tinted per channel at render)
            Self::Generic     => [140, 140, 140],   // gray
        }
    }

    /// Phosphor icon glyph for this port kind
    pub fn icon_glyph(&self) -> &'static str {
        match self {
            Self::Number      => crate::icons::MATH_OPERATIONS,
            Self::Normalized  => crate::icons::SLIDERS,
            Self::Trigger     => crate::icons::LIGHTNING,
            Self::Gate        => crate::icons::TOGGLE_RIGHT,
            Self::Text        => crate::icons::TEXT_T,
            Self::Image       => crate::icons::IMAGE,
            Self::Audio       => crate::icons::WAVEFORM,
            Self::Color       => crate::icons::PALETTE,
            Self::Generic     => "",
        }
    }

    /// Shape identifier for rendering
    /// 0=Circle, 1=RoundedSquare, 2=Triangle, 3=Diamond, 4=HalfMoon
    pub fn shape_id(&self) -> u8 {
        match self {
            Self::Number      => 0, // circle
            Self::Normalized  => 0, // circle (with ring indicator)
            Self::Trigger     => 2, // triangle
            Self::Gate        => 4, // half-moon
            Self::Text        => 1, // rounded square
            Self::Image       => 3, // diamond
            Self::Audio       => 0, // circle (with inner dot)
            Self::Color       => 0, // circle (tinted)
            Self::Generic     => 0, // circle
        }
    }

    /// Convert from PortValue (runtime inference, used as fallback)
    pub fn from_value(val: &PortValue) -> Self {
        match val {
            PortValue::Float(_) => Self::Number,
            PortValue::Text(_) => Self::Text,
            PortValue::Image(_) => Self::Image,
            PortValue::None => Self::Generic,
        }
    }

    /// Check if two port kinds are compatible for connection.
    /// Rules are intentionally permissive — numeric types (Number, Normalized, Trigger, Gate, Color)
    /// can interconnect since they all carry float values. Audio and Image are exclusive.
    /// Generic is compatible with everything. Text only connects to Text or Generic.
    pub fn compatible(from: PortKind, to: PortKind) -> bool {
        use PortKind::*;
        if from == Generic || to == Generic { return true; }
        match (from, to) {
            // Numeric family: all interchangeable (they're all f32 underneath)
            (Number | Normalized | Trigger | Gate | Color, Number | Normalized | Trigger | Gate | Color) => true,
            // Audio only connects to Audio
            (Audio, Audio) => true,
            // Image only connects to Image
            (Image, Image) => true,
            // Text only connects to Text
            (Text, Text) => true,
            // Everything else is incompatible
            _ => false,
        }
    }
}

pub struct PortDef {
    pub name: std::borrow::Cow<'static, str>,
    pub kind: PortKind,
}

impl PortDef {
    /// Create a PortDef with a static name (zero-cost, no allocation)
    pub fn new(name: &'static str, kind: PortKind) -> Self {
        Self { name: std::borrow::Cow::Borrowed(name), kind }
    }
    /// Create a PortDef with a dynamic name (owned String, freed on drop)
    pub fn dynamic(name: String, kind: PortKind) -> Self {
        Self { name: std::borrow::Cow::Owned(name), kind }
    }
}

// ── MIDI mode ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiMode { Note, CC }

// ── Node types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Slider {
        value: f32,
        min: f32,
        max: f32,
        #[serde(default = "default_slider_step")]
        step: f32,
        #[serde(default = "default_slider_color")]
        slider_color: [u8; 3],
        #[serde(default)]
        label: String,
    },
    Display {
        #[serde(default = "default_scope_history")]
        history: Vec<f32>,
        #[serde(default = "default_scope_length")]
        history_max: usize,
        #[serde(default = "default_scope_min")]
        scope_min: f32,
        #[serde(default = "default_scope_max")]
        scope_max: f32,
        #[serde(default = "default_scope_height")]
        scope_height: f32,
        #[serde(default)]
        paused: bool,
        #[serde(default = "default_display_color")]
        display_color: [u8; 3],
        #[serde(default)]
        label: String,
        #[serde(default)]
        auto_fit: bool,
    },
    VisualOutput {
        #[serde(default = "default_preview_size")]
        preview_size: f32,
    },
    Add,
    Multiply,
    /// Formula-based math node with auto-detected variable ports (A-Z)
    Math {
        formula: String,
        /// Detected variable names (sorted A-Z), drives input port count
        #[serde(default)]
        variables: Vec<char>,
        /// Last computed result
        #[serde(default)]
        result: f64,
        /// Error message from last evaluation
        #[serde(default)]
        error: String,
    },
    File { path: String, content: String },
    /// Folder browser: lists files in a directory, click to open
    FolderBrowser {
        #[serde(default)]
        path: String,
        #[serde(default)]
        selected_file: String,
        #[serde(default)]
        search: String,
    },
    TextEditor { content: String },
    WgslViewer {
        #[serde(default)]
        wgsl_code: String,
        #[serde(default)]
        uniform_names: Vec<String>,
        #[serde(default)]
        uniform_types: Vec<String>,
        #[serde(default)]
        uniform_values: Vec<f32>,
        #[serde(default)]
        uniform_min: Vec<f32>,
        #[serde(default)]
        uniform_max: Vec<f32>,
        #[serde(default = "default_canvas_w")]
        canvas_w: f32,
        #[serde(default = "default_canvas_h")]
        canvas_h: f32,
        #[serde(default = "default_resolution")]
        resolution: u32,
        #[serde(default)]
        expanded: bool,
    },
    MouseTracker { x: f32, y: f32 },
    Time {
        #[serde(default)]
        elapsed: f32,
        #[serde(default)]
        speed: f32,
        #[serde(default)]
        running: bool,
    },
    Color {
        r: u8, g: u8, b: u8,
    },
    MidiOut {
        port_name: String,
        mode: MidiMode,
        channel: u8,
        #[serde(default)]
        manual_d1: u8,
        #[serde(default)]
        manual_d2: u8,
    },
    MidiIn {
        port_name: String,
        channel: u8,
        note: u8,
        velocity: u8,
        #[serde(default)]
        log: Vec<String>,
    },
    Theme {
        dark_mode: bool,
        accent: [u8; 3],
        font_size: f32,
        #[serde(default = "default_bg_color")]
        bg_color: [u8; 3],
        #[serde(default = "default_text_color")]
        text_color: [u8; 3],
        #[serde(default = "default_window_bg")]
        window_bg: [u8; 3],
        #[serde(default = "default_window_alpha")]
        window_alpha: u8,
        #[serde(default = "default_grid_color")]
        grid_color: [u8; 3],
        /// Grid style: 0=Solid (no grid), 1=Square, 2=Dotted
        #[serde(default = "default_grid_style")]
        grid_style: u8,
        /// Wire style: 0=Bezier, 1=Straight, 2=Orthogonal, 3=Wiggly
        #[serde(default)]
        wire_style: u8,
        /// Wiggly wire: gravity sag (0=none, 1=heavy droop)
        #[serde(default)]
        wiggle_gravity: f32,
        /// Wiggly wire: amplitude range multiplier (0.1=tiny, 2.0=wild)
        #[serde(default = "default_wiggle_range")]
        wiggle_range: f32,
        /// Wiggly wire: speed multiplier (0.1=slow, 2.0=fast)
        #[serde(default = "default_wiggle_speed")]
        wiggle_speed: f32,
        #[serde(default = "default_rounding")]
        rounding: f32,
        #[serde(default = "default_spacing")]
        spacing: f32,
        #[serde(default)]
        use_hsl: bool,
        /// Wire thickness (default 2.0)
        #[serde(default = "default_wire_thickness")]
        wire_thickness: f32,
        /// Background image/video path (shown on canvas behind nodes)
        #[serde(default)]
        background_path: String,
    },
    Serial {
        port_name: String,
        baud_rate: u32,
        #[serde(default)]
        log: Vec<String>,
        #[serde(default)]
        last_line: String,
        #[serde(default)]
        send_buf: String,
    },
    Comment {
        text: String,
        #[serde(default = "default_comment_color")]
        bg_color: [u8; 3],
    },
    Script {
        name: String,
        input_names: Vec<String>,
        output_names: Vec<String>,
        code: String,
        #[serde(default)]
        last_values: Vec<f32>,
        #[serde(default)]
        error: String,
        #[serde(default = "default_true")]
        continuous: bool,
        #[serde(default)]
        trigger: bool,
    },
    Console {
        #[serde(default)]
        messages: Vec<String>,
    },
    Monitor,
    OscOut {
        host: String,
        port: u16,
        address: String,
        arg_count: usize,
    },
    OscIn {
        port: u16,
        address_filter: String,
        #[serde(default)]
        arg_count: usize,
        #[serde(default)]
        last_args: Vec<f32>,
        /// Text representation of last args (preserves strings, formatted numbers)
        #[serde(default)]
        last_args_text: Vec<String>,
        #[serde(default)]
        log: Vec<String>,
        #[serde(default)]
        listening: bool,
        /// Auto-discovered addresses: (address, arg_count, last_preview)
        #[serde(default)]
        discovered: Vec<(String, usize, String)>,
    },
    KeyInput {
        key_name: String,
        #[serde(default)]
        pressed: bool,
        #[serde(default)]
        toggle_mode: bool,
        #[serde(default)]
        toggled_on: bool,
    },
    Palette {
        #[serde(default)]
        search: String,
    },
    HttpRequest {
        #[serde(default)]
        url: String,
        #[serde(default)]
        method: String,        // GET or POST
        #[serde(default)]
        headers: String,       // Custom headers, key: value per line
        #[serde(default)]
        response: String,
        #[serde(default)]
        status: String,        // "idle" / "200 OK" / "error: ..."
        #[serde(default)]
        auto_send: bool,
        #[serde(default)]
        last_hash: u64,
    },
    AiRequest {
        #[serde(default = "default_provider")]
        provider: String,      // "anthropic" / "openai" / "google"
        #[serde(default = "default_model")]
        model: String,
        #[serde(default)]
        system_prompt: String,
        #[serde(default)]
        user_prompt: String,
        #[serde(default)]
        response: String,
        #[serde(default)]
        status: String,
        #[serde(default = "default_max_tokens")]
        max_tokens: u32,
        #[serde(default = "default_temperature")]
        temperature: f32,
        #[serde(default)]
        api_key: String,
        #[serde(default)]
        response_type: u8,     // 0=Text, 1=JSON, 2=Code, 3=WGSL, 4=HTML, 5=Image
        #[serde(default)]
        last_trigger: f32,     // for rising-edge detection on trigger port
        // Legacy fields kept for backward compatibility
        #[serde(default)]
        api_key_name: String,
        #[serde(default)]
        custom_url: String,
    },
    JsonExtract {
        #[serde(default)]
        path: String,
    },
    FileMenu,
    ZoomControl {
        #[serde(default)]
        zoom_value: f32,
    },
    ObHub {
        #[serde(default)]
        port_name: String,
        #[serde(default)]
        selected_port: String,
        /// (device_type, id) pairs discovered from the hub — updated each frame
        #[serde(default)]
        detected_devices: Vec<(String, u8)>,
    },
    ObJoystick {
        #[serde(default = "default_device_id")]
        device_id: u8,
        /// Which Hub node this device belongs to (set by spawn button, 0 = auto-find)
        #[serde(default)]
        hub_node_id: NodeId,
    },
    ObEncoder {
        #[serde(default = "default_device_id")]
        device_id: u8,
        #[serde(default)]
        hub_node_id: NodeId,
    },
    Synth {
        #[serde(default)]
        waveform: crate::audio::Waveform,
        #[serde(default = "default_440")]
        frequency: f32,
        #[serde(default = "default_half")]
        amplitude: f32,
        #[serde(default = "default_true")]
        active: bool,
        /// FM modulation depth in Hz
        #[serde(default)]
        fm_depth: f32,
    },
    AudioPlayer {
        #[serde(default)]
        file_path: String,
        #[serde(default = "default_one")]
        volume: f32,
        #[serde(default)]
        looping: bool,
        /// Duration of the loaded audio file in seconds (computed on load)
        #[serde(default)]
        duration_secs: f64,
    },
    AudioInput {
        #[serde(default)]
        selected_device: String,
        #[serde(default = "default_mic_gain")]
        gain: f32,
        #[serde(default)]
        active: bool,
    },
    /// Real-time audio analysis — outputs amplitude, bass, mid, treble from the master mix.
    AudioAnalyzer,
    AudioDevice {
        #[serde(default)]
        selected_output: String,
        #[serde(default)]
        selected_input: String,
        #[serde(default = "default_point_eight")]
        master_volume: f32,
    },
    AudioFx {
        #[serde(default)]
        effects: Vec<crate::audio::AudioEffect>,
    },
    // Individual audio effect nodes
    AudioDelay {
        #[serde(default = "default_delay_ms")]
        time_ms: f32,
        #[serde(default = "default_half")]
        feedback: f32,
    },
    AudioDistortion {
        #[serde(default = "default_distortion_drive")]
        drive: f32,
    },
    AudioLowPass {
        #[serde(default = "default_lpf_cutoff")]
        cutoff: f32,
    },
    AudioHighPass {
        #[serde(default = "default_hpf_cutoff")]
        cutoff: f32,
    },
    AudioGain {
        #[serde(default = "default_one")]
        level: f32,
    },
    /// Schroeder reverb — room size, damping, wet/dry mix.
    AudioReverb {
        #[serde(default = "default_half")]
        room_size: f32,
        #[serde(default = "default_half")]
        damping: f32,
        #[serde(default = "default_reverb_mix")]
        mix: f32,
    },
    /// Parametric EQ — interactive frequency response curve with biquad filter bank.
    AudioEq {
        #[serde(default = "default_eq_points")]
        points: Vec<[f32; 2]>,
    },
    Speaker {
        #[serde(default)]
        active: bool,
        #[serde(default = "default_point_eight")]
        volume: f32,
    },
    /// Audio Mixer: variable number of audio input channels, each with a gain fader.
    /// Per-channel: audio input + gain control input. One mixed audio output.
    AudioMixer {
        /// Number of input channels (min 2)
        #[serde(default = "default_mixer_channels")]
        channel_count: usize,
        /// Per-channel gain (0.0 – 1.0)
        #[serde(default = "default_mixer_gains")]
        gains: Vec<f32>,
    },
    RustPlugin {
        #[serde(default)]
        input_names: Vec<String>,
        #[serde(default)]
        output_names: Vec<String>,
        #[serde(default)]
        code: String,
        #[serde(default)]
        last_values: Vec<f64>,
        #[serde(default)]
        error: String,
    },
    McpServer,
    Profiler,
    HtmlViewer,
    // ── Image & Signal nodes ────────────────────────────────
    ImageNode {
        #[serde(default)]
        path: String,
        #[serde(default)]
        save_path: String,
        #[serde(skip)]
        image_data: Option<Arc<ImageData>>,
        #[serde(default = "default_preview_size")]
        preview_size: f32,
        /// Hash of last saved image to avoid re-saving unchanged data
        #[serde(skip)]
        last_save_hash: u64,
    },
    ImageEffects {
        #[serde(default = "default_one")]
        brightness: f32,
        #[serde(default = "default_one")]
        contrast: f32,
        #[serde(default = "default_one")]
        saturation: f32,
        #[serde(default)]
        hue: f32,
        #[serde(default)]
        exposure: f32,
        #[serde(default = "default_one")]
        gamma: f32,
    },
    Crop {
        /// Crop margins as fractions 0.0–1.0 of the image dimension
        #[serde(default)]
        top: f32,
        #[serde(default)]
        left: f32,
        #[serde(default)]
        bottom: f32,
        #[serde(default)]
        right: f32,
    },
    Blend {
        #[serde(default)]
        mode: u8,
        #[serde(default = "default_half")]
        mix: f32,
    },
    Curve {
        #[serde(default = "default_curve_points")]
        points: Vec<[f32; 2]>,
        /// 0=Manual (X input), 1=Envelope (trigger+speed), 2=LFO (looping)
        #[serde(default)]
        mode: u8,
        /// Playback speed: 1.0 = 1 second to traverse full curve
        #[serde(default = "default_one")]
        speed: f32,
        /// Whether envelope loops at the end
        #[serde(default)]
        looping: bool,
        /// Current playback position 0→1 (runtime only)
        #[serde(skip)]
        phase: f32,
        /// Is envelope currently playing (runtime only)
        #[serde(skip)]
        playing: bool,
        /// Last trigger input value for rising-edge detection (runtime only)
        #[serde(skip)]
        last_trigger: f32,
    },
    Draw {
        #[serde(default)]
        strokes: Vec<DrawStroke>,
        #[serde(default = "default_draw_size")]
        canvas_size: f32,
        #[serde(default)]
        color: [u8; 3],
        #[serde(default = "default_draw_width")]
        line_width: f32,
    },
    Noise {
        #[serde(default)]
        noise_type: u8,
        #[serde(default)]
        mode: u8,
        #[serde(default = "default_noise_scale")]
        scale: f32,
        #[serde(default)]
        seed: u32,
    },
    ColorCurves {
        #[serde(default = "default_curve_points")]
        master: Vec<[f32; 2]>,
        #[serde(default = "default_curve_points")]
        red: Vec<[f32; 2]>,
        #[serde(default = "default_curve_points")]
        green: Vec<[f32; 2]>,
        #[serde(default = "default_curve_points")]
        blue: Vec<[f32; 2]>,
        #[serde(default)]
        active_channel: u8,
    },
    VideoPlayer {
        #[serde(default)]
        path: String,
        #[serde(default)]
        playing: bool,
        #[serde(default)]
        looping: bool,
        #[serde(default = "default_video_w")]
        res_w: u32,
        #[serde(default = "default_video_h")]
        res_h: u32,
        #[serde(skip)]
        current_frame: Option<Arc<ImageData>>,
        #[serde(default)]
        duration: f32,
        #[serde(default = "default_speed")]
        speed: f32,
        #[serde(default)]
        status: String,
    },
    Camera {
        #[serde(default)]
        device_index: u32,
        #[serde(default = "default_video_w")]
        res_w: u32,
        #[serde(default = "default_video_h")]
        res_h: u32,
        #[serde(default)]
        active: bool,
        #[serde(skip)]
        current_frame: Option<Arc<ImageData>>,
        #[serde(default)]
        status: String,
    },
    MlModel {
        #[serde(default)]
        model_path: String,
        #[serde(default)]
        labels_path: String,
        #[serde(default = "default_confidence")]
        confidence: f32,
        #[serde(default)]
        preset: MlPreset,
        #[serde(default)]
        result_text: String,
        #[serde(default)]
        result_json: String,
        #[serde(skip)]
        annotated_frame: Option<std::sync::Arc<ImageData>>,
        #[serde(default)]
        status: String,
        #[serde(skip)]
        last_input_hash: u64,
    },
    /// Gate: Compare + pass/block in one node.
    /// When condition (Value <mode> Threshold) is true → output = Value. Else → output = else_value.
    Gate {
        /// 0: >, 1: <, 2: >=, 3: <=, 4: ==, 5: !=
        #[serde(default)]
        mode: u8,
        #[serde(default)]
        threshold: f32,
        #[serde(default)]
        else_value: f32,
    },
    /// Timer/Interval: periodic pulse every N seconds.
    /// Uses wall-clock reference time for drift-free tempo sync.
    Timer {
        #[serde(default = "default_one")]
        interval: f32,
        #[serde(default)]
        elapsed: f32,
        #[serde(default = "default_true")]
        running: bool,
        /// How long the trigger stays high (seconds)
        #[serde(default = "default_pulse_width")]
        pulse_width: f32,
        /// Wall-clock reference time (seconds since app start) when timer was started/resumed.
        /// elapsed is computed as `now - ref_time + paused_elapsed`.
        /// Skipped in serialization — re-initialized on load.
        #[serde(skip)]
        ref_time: f64,
        /// Accumulated elapsed time when paused (so we can resume seamlessly).
        #[serde(skip)]
        paused_elapsed: f64,
        /// Whether ref_time has been initialized this session.
        #[serde(skip)]
        time_initialized: bool,
    },
    /// Map/Range: linear mapping from one range to another
    MapRange {
        #[serde(default)]
        in_min: f32,
        #[serde(default = "default_one")]
        in_max: f32,
        #[serde(default)]
        out_min: f32,
        #[serde(default = "default_one")]
        out_max: f32,
        #[serde(default)]
        clamp: bool,
    },
    /// String Format: template with {0}, {1} placeholders
    StringFormat {
        #[serde(default)]
        template: String,
        #[serde(default = "default_string_format_args")]
        arg_count: usize,
    },
    /// Sample & Hold: capture value on trigger rising edge, hold until next
    SampleHold {
        #[serde(default)]
        held_float: f32,
        #[serde(default)]
        held_text: String,
        /// Whether the last held value was text (true) or float (false)
        #[serde(default)]
        is_text: bool,
        /// Last trigger value for rising-edge detection
        #[serde(default)]
        last_trigger: f32,
        /// History of held float values for staircase visualization
        #[serde(default)]
        history: Vec<f32>,
    },
    /// Select/Switch: route input A or B based on selector
    Select {
        /// 0 = hard switch, 1 = crossfade (float only)
        #[serde(default)]
        mode: u8,
    },
}

fn default_pulse_width() -> f32 { 0.1 }
fn default_string_format_args() -> usize { 2 }
fn default_confidence() -> f32 { 0.05 }
fn default_video_w() -> u32 { 640 }
fn default_video_h() -> u32 { 480 }
fn default_speed() -> f32 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawStroke {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 3],
    pub width: f32,
}

fn default_device_id() -> u8 { 1 }
fn default_preview_size() -> f32 { 150.0 }
fn default_draw_size() -> f32 { 200.0 }
fn default_draw_width() -> f32 { 2.0 }
fn default_noise_scale() -> f32 { 5.0 }
fn default_curve_points() -> Vec<[f32; 2]> { vec![[0.0, 0.0], [1.0, 1.0]] }
/// Flat EQ: 5 points at 0dB (y=0.5) across the frequency range
fn default_eq_points() -> Vec<[f32; 2]> { vec![[0.0, 0.5], [0.25, 0.5], [0.5, 0.5], [0.75, 0.5], [1.0, 0.5]] }
fn default_mixer_channels() -> usize { 2 }
fn default_mixer_gains() -> Vec<f32> { vec![0.8, 0.8] }
fn default_440() -> f32 { 440.0 }
fn default_half() -> f32 { 0.5 }
fn default_reverb_mix() -> f32 { 0.3 }
fn default_one() -> f32 { 1.0 }
fn default_mic_gain() -> f32 { 1.0 }
fn default_point_eight() -> f32 { 0.8 }
fn default_delay_ms() -> f32 { 250.0 }
fn default_distortion_drive() -> f32 { 4.0 }
fn default_lpf_cutoff() -> f32 { 1000.0 }
fn default_hpf_cutoff() -> f32 { 200.0 }

impl NodeType {
    pub fn title(&self) -> &str {
        match self {
            NodeType::Slider { .. } => "Slider",
            NodeType::Display { .. } => "Display",
            NodeType::VisualOutput { .. } => "Visual Output",
            NodeType::Add => "Add",
            NodeType::Multiply => "Multiply",
            NodeType::Math { .. } => "Math",
            NodeType::File { .. } => "File",
            NodeType::FolderBrowser { .. } => "Folder",
            NodeType::TextEditor { .. } => "Text Editor",
            NodeType::WgslViewer { .. } => "WGSL Viewer",
            NodeType::Time { .. } => "Time",
            NodeType::Color { .. } => "Color",
            NodeType::MouseTracker { .. } => "Mouse Tracker",
            NodeType::MidiOut { .. } => "MIDI Out",
            NodeType::MidiIn { .. } => "MIDI In",
            NodeType::Theme { .. } => "Theme",
            NodeType::Serial { .. } => "Serial",
            NodeType::Comment { .. } => "Comment",
            NodeType::Script { .. } => "Script",
            NodeType::Console { .. } => "Console",
            NodeType::Monitor => "Monitor",
            NodeType::OscOut { .. } => "OSC Out",
            NodeType::OscIn { .. } => "OSC In",
            NodeType::KeyInput { .. } => "Keyboard Input",
            NodeType::Palette { .. } => "Node Palette",
            NodeType::HttpRequest { .. } => "HTTP Request",
            NodeType::AiRequest { .. } => "AI Request",
            NodeType::JsonExtract { .. } => "JSON Extract",
            NodeType::FileMenu => "File",
            NodeType::ZoomControl { .. } => "Zoom",
            NodeType::ObHub { .. } => "OB Hub",
            NodeType::ObJoystick { .. } => "OB Joystick",
            NodeType::ObEncoder { .. } => "OB Encoder",
            NodeType::Synth { .. } => "Synth",
            NodeType::AudioPlayer { .. } => "Audio Player",
            NodeType::AudioInput { .. } => "Audio Input",
            NodeType::AudioAnalyzer => "Audio Analyzer",
            NodeType::AudioDevice { .. } => "Audio Device",
            NodeType::AudioFx { .. } => "Audio FX",
            NodeType::AudioDelay { .. } => "Delay",
            NodeType::AudioDistortion { .. } => "Distortion",
            NodeType::AudioReverb { .. } => "Reverb",
            NodeType::AudioLowPass { .. } => "Low Pass",
            NodeType::AudioHighPass { .. } => "High Pass",
            NodeType::AudioGain { .. } => "Gain",
            NodeType::AudioEq { .. } => "EQ",
            NodeType::Speaker { .. } => "Speaker",
            NodeType::AudioMixer { .. } => "Mixer",
            NodeType::RustPlugin { .. } => "Rust Plugin",
            NodeType::McpServer => "MCP Server",
            NodeType::Profiler => "System Profiler",
            NodeType::HtmlViewer => "HTML Viewer",
            NodeType::ImageNode { .. } => "Image",
            NodeType::Crop { .. } => "Crop",
            NodeType::ImageEffects { .. } => "Image Effects",
            NodeType::Blend { .. } => "Blend",
            NodeType::Curve { .. } => "Curve",
            NodeType::Draw { .. } => "Draw",
            NodeType::Noise { .. } => "Noise",
            NodeType::ColorCurves { .. } => "Color Curves",
            NodeType::VideoPlayer { .. } => "Video Player",
            NodeType::Camera { .. } => "Camera",
            NodeType::MlModel { .. } => "ML Model",
            NodeType::Gate { .. } => "Gate",
            NodeType::Timer { .. } => "Timer",
            NodeType::MapRange { .. } => "Map/Range",
            NodeType::StringFormat { .. } => "String Format",
            NodeType::SampleHold { .. } => "Sample & Hold",
            NodeType::Select { .. } => "Select",
        }
    }

    pub fn inputs(&self) -> Vec<PortDef> {
        use PortKind::*;
        match self {
            NodeType::Slider { .. } => vec![PortDef::new("In", Number), PortDef::new("Min", Number), PortDef::new("Max", Number)],
            NodeType::Display { .. } => vec![PortDef::new("Value", Generic)],
            NodeType::VisualOutput { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Add => vec![PortDef::new("A", Number), PortDef::new("B", Number)],
            NodeType::Multiply => vec![PortDef::new("A", Number), PortDef::new("B", Number)],
            NodeType::Math { variables, .. } => {
                variables.iter().map(|c| {
                    PortDef::dynamic(format!("{}", c), Number)
                }).collect()
            }
            NodeType::File { .. } => vec![],
            NodeType::FolderBrowser { .. } => vec![],
            NodeType::TextEditor { .. } => vec![PortDef::new("Text In", Text)],
            NodeType::WgslViewer { uniform_names, uniform_types, .. } => {
                let mut ports = vec![PortDef::new("WGSL", Text)];
                for (i, n) in uniform_names.iter().enumerate() {
                    let t = uniform_types.get(i).map(|s| s.as_str()).unwrap_or("float");
                    if t == "color" {
                        ports.push(PortDef::dynamic(format!("{} R", n), Color));
                        ports.push(PortDef::dynamic(format!("{} G", n), Color));
                        ports.push(PortDef::dynamic(format!("{} B", n), Color));
                    } else {
                        ports.push(PortDef::dynamic(n.clone(), Number));
                    }
                }
                ports
            }
            NodeType::Time { .. } => vec![],
            NodeType::Color { .. } => vec![PortDef::new("R", Color), PortDef::new("G", Color), PortDef::new("B", Color)],
            NodeType::MouseTracker { .. } => vec![],
            NodeType::MidiOut { mode, .. } => match mode {
                MidiMode::Note => vec![PortDef::new("Channel", Number), PortDef::new("Note", Number), PortDef::new("Velocity", Number)],
                MidiMode::CC => vec![PortDef::new("Channel", Number), PortDef::new("CC#", Number), PortDef::new("Value", Number)],
            },
            NodeType::MidiIn { .. } => vec![],
            NodeType::Theme { .. } => vec![
                PortDef::new("BG R", Color), PortDef::new("BG G", Color), PortDef::new("BG B", Color),
                PortDef::new("Text R", Color), PortDef::new("Text G", Color), PortDef::new("Text B", Color),
                PortDef::new("Accent R", Color), PortDef::new("Accent G", Color), PortDef::new("Accent B", Color),
                PortDef::new("Win R", Color), PortDef::new("Win G", Color), PortDef::new("Win B", Color),
                PortDef::new("Grid R", Color), PortDef::new("Grid G", Color), PortDef::new("Grid B", Color),
                PortDef::new("Font Size", Number),
                PortDef::new("Rounding", Number),
                PortDef::new("Spacing", Number),
                PortDef::new("Win Alpha", Normalized),
                PortDef::new("Background", Text),
                PortDef::new("BG Image", Image),
            ],
            NodeType::Serial { .. } => vec![PortDef::new("Send", Text)],
            NodeType::Comment { .. } => vec![],
            NodeType::Console { .. } => vec![],
            NodeType::Monitor => vec![],
            NodeType::OscOut { arg_count, .. } => {
                (0..*arg_count).map(|i| PortDef::dynamic(format!("Arg {}", i), Generic)).collect()
            }
            NodeType::OscIn { .. } => vec![],
            NodeType::KeyInput { .. } => vec![],
            NodeType::Palette { .. } => vec![],
            NodeType::HttpRequest { .. } => vec![PortDef::new("URL", Text), PortDef::new("Body", Text), PortDef::new("Headers", Text)],
            NodeType::AiRequest { .. } => vec![PortDef::new("System", Text), PortDef::new("Prompt", Text), PortDef::new("Send", Trigger)],
            NodeType::JsonExtract { .. } => vec![PortDef::new("JSON", Text)],
            NodeType::FileMenu => vec![],
            NodeType::ZoomControl { .. } => vec![PortDef::new("Zoom", Number)],
            NodeType::ObHub { .. } => vec![PortDef::new("Command", Text)],
            NodeType::ObJoystick { .. } => vec![],
            NodeType::ObEncoder { .. } => vec![],
            NodeType::Synth { .. } => vec![PortDef::new("Freq", Number), PortDef::new("Amp", Normalized), PortDef::new("Gate", Gate), PortDef::new("FM Wt", Normalized)],
            NodeType::AudioPlayer { .. } => vec![PortDef::new("Play", Trigger), PortDef::new("Volume", Normalized), PortDef::new("Seek", Normalized), PortDef::new("Speed", Number)],
            NodeType::AudioInput { .. } => vec![PortDef::new("Gain", Normalized)],
            NodeType::AudioAnalyzer => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioDevice { .. } => vec![],
            NodeType::AudioFx { .. } => vec![PortDef::new("Source", Audio)],
            NodeType::AudioDelay { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Time", Number), PortDef::new("Feedback", Normalized)],
            NodeType::AudioDistortion { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Drive", Number)],
            NodeType::AudioReverb { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Room", Normalized), PortDef::new("Damp", Normalized), PortDef::new("Mix", Normalized)],
            NodeType::AudioLowPass { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Cutoff", Number)],
            NodeType::AudioHighPass { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Cutoff", Number)],
            NodeType::AudioGain { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Level", Number)],
            NodeType::AudioEq { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::Speaker { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Volume", Normalized)],
            NodeType::AudioMixer { channel_count, .. } => {
                // Per channel: Audio input + Gain control input
                let mut ports = Vec::new();
                for i in 0..*channel_count {
                    ports.push(PortDef::dynamic(format!("Ch{}", i + 1), Audio));
                    ports.push(PortDef::dynamic(format!("Gain{}", i + 1), Normalized));
                }
                ports
            }
            NodeType::RustPlugin { input_names, .. } => {
                input_names.iter().map(|n| PortDef::dynamic(n.clone(), Generic)).collect()
            }
            NodeType::McpServer => vec![],
            NodeType::Profiler => vec![],
            NodeType::HtmlViewer => vec![PortDef::new("HTML", Text)],
            NodeType::ImageNode { .. } => vec![PortDef::new("Image In", Image)],
            NodeType::Crop { .. } => vec![
                PortDef::new("Image", Image),
                PortDef::new("Top", Normalized), PortDef::new("Left", Normalized),
                PortDef::new("Bottom", Normalized), PortDef::new("Right", Normalized),
            ],
            NodeType::ImageEffects { .. } => vec![
                PortDef::new("Image", Image), PortDef::new("Brightness", Normalized), PortDef::new("Contrast", Normalized),
                PortDef::new("Saturation", Normalized), PortDef::new("Hue", Number), PortDef::new("Exposure", Number), PortDef::new("Gamma", Number),
            ],
            NodeType::Blend { .. } => vec![PortDef::new("A", Image), PortDef::new("B", Image), PortDef::new("Mix", Normalized)],
            NodeType::Curve { .. } => vec![
                PortDef::new("X", Normalized), PortDef::new("Trigger", Trigger),
                PortDef::new("Speed", Number), PortDef::new("Gate", Gate),
            ],
            NodeType::Draw { .. } => vec![],
            NodeType::Noise { .. } => vec![PortDef::new("Seed", Number), PortDef::new("Scale", Number), PortDef::new("X", Number), PortDef::new("Y", Number)],
            NodeType::ColorCurves { .. } => vec![PortDef::new("Image", Image)],
            NodeType::VideoPlayer { .. } => vec![],
            NodeType::Camera { .. } => vec![],
            NodeType::MlModel { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Gate { .. } => vec![PortDef::new("Value", Number), PortDef::new("Threshold", Number)],
            NodeType::Timer { .. } => vec![PortDef::new("Interval", Number), PortDef::new("BPM", Number)],
            NodeType::MapRange { .. } => vec![
                PortDef::new("Value", Number), PortDef::new("In Min", Number), PortDef::new("In Max", Number),
                PortDef::new("Out Min", Number), PortDef::new("Out Max", Number),
            ],
            NodeType::StringFormat { arg_count, .. } => {
                let mut ports = vec![PortDef::new("Template", Text)];
                for i in 0..*arg_count {
                    ports.push(PortDef::dynamic(format!("Arg {}", i), Generic));
                }
                ports
            }
            NodeType::SampleHold { .. } => vec![PortDef::new("Value", Generic), PortDef::new("Trigger", Trigger)],
            NodeType::Select { .. } => vec![PortDef::new("A", Generic), PortDef::new("B", Generic), PortDef::new("Selector", Normalized)],
            NodeType::Script { input_names, continuous, .. } => {
                let mut ports: Vec<PortDef> = Vec::new();
                if !continuous { ports.push(PortDef::new("Exec", Trigger)); }
                ports.push(PortDef::new("Code", Text));
                for n in input_names {
                    ports.push(PortDef::dynamic(n.clone(), Generic));
                }
                ports
            }
        }
    }

    pub fn outputs(&self) -> Vec<PortDef> {
        use PortKind::*;
        match self {
            NodeType::Slider { .. } => vec![PortDef::new("Value", Number)],
            NodeType::Display { .. } => vec![],
            NodeType::VisualOutput { .. } => vec![],
            NodeType::Add => vec![PortDef::new("Result", Number)],
            NodeType::Multiply => vec![PortDef::new("Result", Number)],
            NodeType::Math { .. } => vec![PortDef::new("Result", Number)],
            NodeType::File { .. } => vec![PortDef::new("Content", Text)],
            NodeType::FolderBrowser { .. } => vec![PortDef::new("Path", Text), PortDef::new("Name", Text), PortDef::new("Content", Text)],
            NodeType::TextEditor { .. } => vec![PortDef::new("Text Out", Text)],
            NodeType::WgslViewer { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Time { .. } => vec![PortDef::new("Seconds", Number), PortDef::new("Beat", Normalized)],
            NodeType::Color { .. } => vec![PortDef::new("R", Color), PortDef::new("G", Color), PortDef::new("B", Color)],
            NodeType::MouseTracker { .. } => vec![PortDef::new("X", Number), PortDef::new("Y", Number)],
            NodeType::MidiOut { .. } => vec![],
            NodeType::MidiIn { .. } => vec![PortDef::new("Channel", Number), PortDef::new("Note", Number), PortDef::new("Velocity", Number)],
            NodeType::Theme { .. } => vec![
                PortDef::new("BG R", Color), PortDef::new("BG G", Color), PortDef::new("BG B", Color),
                PortDef::new("Text R", Color), PortDef::new("Text G", Color), PortDef::new("Text B", Color),
                PortDef::new("Accent R", Color), PortDef::new("Accent G", Color), PortDef::new("Accent B", Color),
            ],
            NodeType::Serial { .. } => vec![PortDef::new("Send", Text)],
            NodeType::Comment { .. } => vec![],
            NodeType::Console { .. } => vec![],
            NodeType::Monitor => vec![PortDef::new("FPS", Number), PortDef::new("Frame ms", Number), PortDef::new("Nodes", Number)],
            NodeType::OscOut { .. } => vec![],
            NodeType::OscIn { arg_count, .. } => {
                let mut ports: Vec<PortDef> = (0..*arg_count).map(|i| PortDef::dynamic(format!("Arg {}", i), Generic)).collect();
                ports.push(PortDef::new("Raw", Text));
                ports.push(PortDef::new("Address", Text));
                ports
            }
            NodeType::KeyInput { .. } => vec![PortDef::new("Trigger", Trigger), PortDef::new("Held", Gate), PortDef::new("Toggle", Gate)],
            NodeType::Script { output_names, .. } => {
                output_names.iter().map(|n| PortDef::dynamic(n.clone(), Generic)).collect()
            }
            NodeType::Palette { .. } => vec![],
            NodeType::HttpRequest { .. } => vec![PortDef::new("Response", Text), PortDef::new("Status", Text)],
            NodeType::AiRequest { .. } => vec![PortDef::new("Response", Text), PortDef::new("Status", Text)],
            NodeType::JsonExtract { .. } => vec![PortDef::new("Value", Generic)],
            NodeType::FileMenu => vec![],
            NodeType::ZoomControl { .. } => vec![PortDef::new("Zoom", Number)],
            NodeType::ObHub { detected_devices, .. } => {
                let mut ports = Vec::new();
                let mut sorted = detected_devices.clone();
                sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                for (dtype, id) in &sorted {
                    match dtype.as_str() {
                        "joystick" => {
                            ports.push(PortDef::dynamic(format!("j{}_x", id), Normalized));
                            ports.push(PortDef::dynamic(format!("j{}_y", id), Normalized));
                            ports.push(PortDef::dynamic(format!("j{}_btn", id), Gate));
                        }
                        "encoder" => {
                            ports.push(PortDef::dynamic(format!("e{}_turn", id), Number));
                            ports.push(PortDef::dynamic(format!("e{}_click", id), Gate));
                            ports.push(PortDef::dynamic(format!("e{}_pos", id), Number));
                        }
                        other => {
                            ports.push(PortDef::dynamic(format!("{}{}_{}", &other[..1], id, "val"), Number));
                        }
                    }
                }
                if ports.is_empty() {
                    ports.push(PortDef::new("(no devices)", Generic));
                }
                ports
            },
            NodeType::ObJoystick { .. } => vec![PortDef::new("X", Normalized), PortDef::new("Y", Normalized), PortDef::new("Button", Gate)],
            NodeType::ObEncoder { .. } => vec![PortDef::new("Turn", Number), PortDef::new("Click", Gate), PortDef::new("Position", Number)],
            NodeType::Synth { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioPlayer { .. } => vec![PortDef::new("Audio", Audio), PortDef::new("Progress", Normalized)],
            NodeType::AudioInput { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioAnalyzer => vec![
                PortDef::new("Amp", Normalized), PortDef::new("Peak", Normalized),
                PortDef::new("Bass", Normalized), PortDef::new("Mid", Normalized),
                PortDef::new("Treble", Normalized),
            ],
            NodeType::AudioDevice { .. } => vec![],
            NodeType::AudioFx { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioDelay { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioDistortion { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioReverb { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioLowPass { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioHighPass { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioGain { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::AudioEq { .. } => vec![PortDef::new("Audio", Audio)],
            NodeType::Speaker { .. } => vec![],
            NodeType::AudioMixer { .. } => vec![PortDef::new("Mix", Audio)],
            NodeType::RustPlugin { output_names, .. } => {
                output_names.iter().map(|n| PortDef::dynamic(n.clone(), Generic)).collect()
            }
            NodeType::McpServer => vec![],
            NodeType::HtmlViewer => vec![PortDef::new("URL", Text)],
            NodeType::Profiler => vec![PortDef::new("FPS", Number), PortDef::new("CPU %", Number), PortDef::new("RAM %", Number), PortDef::new("Proc MB", Number)],
            NodeType::ImageNode { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Crop { .. } => vec![PortDef::new("Cropped", Image)],
            NodeType::ImageEffects { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Blend { .. } => vec![PortDef::new("Image", Image)],
            NodeType::Curve { .. } => vec![
                PortDef::new("Y", Normalized), PortDef::new("Phase", Normalized),
                PortDef::new("End", Trigger), PortDef::new("Image", Image),
            ],
            NodeType::Draw { .. } => vec![PortDef::new("Image", Image), PortDef::new("Points", Text)],
            NodeType::Noise { .. } => vec![PortDef::new("Value", Number), PortDef::new("Image", Image)],
            NodeType::ColorCurves { .. } => vec![PortDef::new("Image", Image)],
            NodeType::VideoPlayer { .. } => vec![PortDef::new("Frame", Image), PortDef::new("Progress", Normalized)],
            NodeType::Camera { .. } => vec![PortDef::new("Frame", Image)],
            NodeType::MlModel { .. } => vec![PortDef::new("Annotated", Image), PortDef::new("Result", Text), PortDef::new("JSON", Text)],
            NodeType::Gate { .. } => vec![PortDef::new("Out", Number), PortDef::new("Pass", Gate)],
            NodeType::Timer { .. } => vec![PortDef::new("Trigger", Trigger), PortDef::new("Phase", Normalized), PortDef::new("BPM", Number)],
            NodeType::MapRange { .. } => vec![PortDef::new("Value", Number)],
            NodeType::StringFormat { .. } => vec![PortDef::new("Text", Text)],
            NodeType::SampleHold { .. } => vec![PortDef::new("Out", Generic), PortDef::new("Trigger", Trigger)],
            NodeType::Select { .. } => vec![PortDef::new("Out", Generic), PortDef::new("Active", Gate)],
        }
    }

    pub fn color_hint(&self) -> [u8; 3] {
        match self {
            NodeType::Slider { .. } => [80, 160, 255],
            NodeType::Display { .. } => [100, 200, 100],
            NodeType::VisualOutput { .. } => [200, 100, 255],
            NodeType::Add | NodeType::Multiply | NodeType::Math { .. } => [200, 160, 80],
            NodeType::File { .. } => [180, 120, 200],
            NodeType::FolderBrowser { .. } => [140, 160, 200],
            NodeType::TextEditor { .. } => [160, 140, 220],
            NodeType::WgslViewer { .. } => [220, 140, 60],
            NodeType::Time { .. } => [180, 220, 100],
            NodeType::Color { .. } => [255, 120, 180],
            NodeType::MouseTracker { .. } => [200, 100, 100],
            NodeType::MidiOut { .. } => [60, 180, 180],
            NodeType::MidiIn { .. } => [80, 200, 160],
            NodeType::Theme { .. } => [255, 180, 80],
            NodeType::Serial { .. } => [200, 180, 60],
            NodeType::Comment { .. } => [140, 140, 140],
            NodeType::Script { .. } => [150, 100, 200],
            NodeType::Console { .. } => [100, 150, 100],
            NodeType::Monitor => [80, 200, 200],
            NodeType::OscOut { .. } => [220, 120, 60],
            NodeType::OscIn { .. } => [60, 160, 220],
            NodeType::KeyInput { .. } => [220, 180, 60],
            NodeType::Palette { .. } => [120, 120, 180],
            NodeType::HttpRequest { .. } => [60, 180, 120],
            NodeType::AiRequest { .. } => [180, 100, 255],
            NodeType::JsonExtract { .. } => [200, 160, 60],
            NodeType::FileMenu => [200, 200, 200],
            NodeType::ZoomControl { .. } => [160, 160, 160],
            NodeType::ObHub { .. } => [40, 180, 120],
            NodeType::ObJoystick { .. } => [80, 160, 255],
            NodeType::ObEncoder { .. } => [200, 140, 80],
            NodeType::Synth { .. } => [100, 220, 180],
            NodeType::AudioPlayer { .. } => [180, 100, 220],
            NodeType::AudioInput { .. } => [220, 80, 120],
            NodeType::AudioAnalyzer => [255, 180, 60],
            NodeType::AudioDevice { .. } => [220, 180, 100],
            NodeType::AudioFx { .. } => [200, 100, 160],
            NodeType::AudioDelay { .. } => [180, 120, 200],
            NodeType::AudioDistortion { .. } => [220, 80, 80],
            NodeType::AudioReverb { .. } => [120, 140, 220],
            NodeType::AudioLowPass { .. } => [100, 160, 200],
            NodeType::AudioHighPass { .. } => [200, 160, 100],
            NodeType::AudioGain { .. } => [160, 200, 100],
            NodeType::AudioEq { .. } => [200, 160, 255],
            NodeType::Speaker { .. } => [80, 200, 80],
            NodeType::AudioMixer { .. } => [160, 120, 220],
            NodeType::RustPlugin { .. } => [255, 120, 50],
            NodeType::McpServer => [120, 200, 255],
            NodeType::Profiler => [255, 160, 60],
            NodeType::HtmlViewer => [60, 180, 220],
            NodeType::ImageNode { .. } => [200, 140, 220],
            NodeType::Crop { .. } => [160, 140, 200],
            NodeType::ImageEffects { .. } => [180, 120, 200],
            NodeType::Blend { .. } => [160, 100, 180],
            NodeType::Curve { .. } => [100, 200, 160],
            NodeType::Draw { .. } => [200, 180, 100],
            NodeType::Noise { .. } => [140, 180, 140],
            NodeType::ColorCurves { .. } => [220, 140, 160],
            NodeType::VideoPlayer { .. } => [220, 80, 140],
            NodeType::Camera { .. } => [80, 200, 140],
            NodeType::MlModel { .. } => [200, 80, 255],
            NodeType::Gate { .. } => [220, 180, 60],
            NodeType::Timer { .. } => [100, 200, 180],
            NodeType::MapRange { .. } => [180, 140, 220],
            NodeType::StringFormat { .. } => [220, 160, 100],
            NodeType::SampleHold { .. } => [120, 200, 160],
            NodeType::Select { .. } => [200, 160, 120],
        }
    }

    /// Whether this node renders its ports inline within the content
    /// instead of as separate lists at top/bottom.
    pub fn inline_ports(&self) -> bool {
        matches!(self, NodeType::Theme { .. } | NodeType::MidiOut { .. } | NodeType::Synth { .. } | NodeType::WgslViewer { .. } | NodeType::Color { .. } | NodeType::ImageEffects { .. } | NodeType::Slider { .. } | NodeType::Display { .. } | NodeType::VisualOutput { .. } | NodeType::Blend { .. } | NodeType::HttpRequest { .. } | NodeType::AiRequest { .. } | NodeType::Math { .. } | NodeType::AudioDelay { .. } | NodeType::AudioDistortion { .. } | NodeType::AudioLowPass { .. } | NodeType::AudioHighPass { .. } | NodeType::AudioGain { .. } | NodeType::AudioReverb { .. } | NodeType::AudioEq { .. } | NodeType::AudioPlayer { .. } | NodeType::Timer { .. } | NodeType::MapRange { .. } | NodeType::StringFormat { .. } | NodeType::SampleHold { .. } | NodeType::Select { .. } | NodeType::Curve { .. } | NodeType::AudioMixer { .. } | NodeType::Gate { .. } | NodeType::Speaker { .. } | NodeType::AudioInput { .. } | NodeType::Crop { .. })
    }

    /// Whether this node skips the standard egui::Window and renders itself completely custom.
    pub fn custom_render(&self) -> bool {
        matches!(self, NodeType::Slider { .. } | NodeType::Comment { .. } | NodeType::Display { .. })
    }
}

// ── Node & Connection ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node { pub id: NodeId, pub node_type: NodeType, pub pos: [f32; 2] }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub from_node: NodeId, pub from_port: usize,
    pub to_node: NodeId, pub to_port: usize,
    /// Optional user label displayed on the wire
    #[serde(default)]
    pub label: String,
}

// ── Graph ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: HashMap<NodeId, Node>,
    pub connections: Vec<Connection>,
    next_id: u64,
    /// Last evaluate timestamp for computing real dt (not serialized)
    #[serde(skip)]
    last_eval_time: f64,
    /// Topologically sorted node evaluation order (rebuilt when graph topology changes)
    #[serde(skip)]
    topo_order: Vec<NodeId>,
    /// Nodes detected to be in dependency cycles (appended after acyclic nodes)
    #[serde(skip)]
    cyclic_nodes: Vec<NodeId>,
    /// True when nodes/connections changed and topo_order must be rebuilt before next eval
    /// Uses skip_serializing + default_true so it starts as true after deserialization
    #[serde(skip_serializing, default = "default_true")]
    topo_dirty: bool,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            connections: Vec::new(),
            next_id: 1,
            last_eval_time: 0.0,
            topo_order: Vec::new(),
            cyclic_nodes: Vec::new(),
            topo_dirty: true,
        }
    }
    /// Get the PortKind for a specific port on a node.
    /// `is_output`: true for output ports, false for input ports.
    pub fn port_kind(&self, node_id: NodeId, port_idx: usize, is_output: bool) -> Option<PortKind> {
        let node = self.nodes.get(&node_id)?;
        let ports = if is_output { node.node_type.outputs() } else { node.node_type.inputs() };
        ports.get(port_idx).map(|p| p.kind)
    }

    pub fn add_node(&mut self, node_type: NodeType, pos: [f32; 2]) -> NodeId {
        let id = self.next_id; self.next_id += 1;
        self.nodes.insert(id, Node { id, node_type, pos });
        self.topo_dirty = true;
        id
    }
    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        self.connections.retain(|c| c.from_node != id && c.to_node != id);
        self.topo_dirty = true;
    }
    pub fn remove_connections_to_port(&mut self, node_id: NodeId, port: usize) {
        self.connections.retain(|c| !(c.to_node == node_id && c.to_port == port));
        self.topo_dirty = true;
    }
    pub fn add_connection(&mut self, from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize) {
        self.connections.retain(|c| !(c.to_node == to_node && c.to_port == to_port));
        self.connections.push(Connection { from_node, from_port, to_node, to_port, label: String::new() });
        self.topo_dirty = true;
    }

    /// Rebuild topological evaluation order using Kahn's algorithm.
    /// Called automatically before evaluate() when topo_dirty is set.
    ///
    /// Result: acyclic nodes in dependency order, then any cyclic nodes appended.
    /// For a stable graph this runs at most once between topology changes (add/remove node/wire).
    fn rebuild_topo_order(&mut self) {
        use std::collections::{VecDeque, HashSet};

        let node_ids: Vec<NodeId> = self.nodes.keys().copied().collect();

        // Build a unique set of dependency edges: (producer, consumer).
        // Multiple connections A→B on different ports count as a single edge.
        let mut edge_set: HashSet<(NodeId, NodeId)> = HashSet::new();
        for conn in &self.connections {
            if self.nodes.contains_key(&conn.from_node)
                && self.nodes.contains_key(&conn.to_node)
                && conn.from_node != conn.to_node   // ignore self-loops
            {
                edge_set.insert((conn.from_node, conn.to_node));
            }
        }

        // Build per-node in_degree and successor list from the unique edges.
        let mut in_degree: HashMap<NodeId, usize> = node_ids.iter().map(|&id| (id, 0)).collect();
        let mut successors: HashMap<NodeId, Vec<NodeId>> = node_ids.iter().map(|&id| (id, Vec::new())).collect();
        for &(from, to) in &edge_set {
            *in_degree.get_mut(&to).unwrap() += 1;
            successors.get_mut(&from).unwrap().push(to);
        }

        // Seed with all zero-in-degree nodes, sorted for deterministic ordering.
        let mut zero: Vec<NodeId> = node_ids.iter().filter(|&&id| in_degree[&id] == 0).copied().collect();
        zero.sort_unstable();
        let mut queue: VecDeque<NodeId> = zero.into_iter().collect();

        let mut sorted: Vec<NodeId> = Vec::with_capacity(node_ids.len());
        let mut in_sorted: HashSet<NodeId> = HashSet::new();

        while let Some(nid) = queue.pop_front() {
            sorted.push(nid);
            in_sorted.insert(nid);
            // Clone successor list so we can mutably borrow in_degree simultaneously.
            let succs: Vec<NodeId> = successors[&nid].clone();
            let mut newly_zero: Vec<NodeId> = Vec::new();
            for succ in succs {
                let deg = in_degree.get_mut(&succ).unwrap();
                *deg -= 1;
                if *deg == 0 { newly_zero.push(succ); }
            }
            newly_zero.sort_unstable();   // keep deterministic
            queue.extend(newly_zero);
        }

        // Any node not emitted by Kahn is in a cycle — append in sorted order.
        let mut cyclic: Vec<NodeId> = node_ids.iter()
            .filter(|&&id| !in_sorted.contains(&id))
            .copied()
            .collect();
        cyclic.sort_unstable();
        sorted.extend_from_slice(&cyclic);

        self.topo_order = sorted;
        self.cyclic_nodes = cyclic;
        self.topo_dirty = false;
    }

    /// Re-evaluate with pre-existing values (for injected hardware data).
    /// Evaluates in topological order so downstream nodes see fresh values in one pass.
    pub fn evaluate_with_existing(&mut self, values: &mut HashMap<(NodeId, usize), PortValue>, _now_secs: f64) {
        if self.topo_dirty || self.topo_order.is_empty() {
            self.rebuild_topo_order();
        }
        let eval_order = self.topo_order.clone();
        // Two passes: first follows topo order for acyclic chains; second catches any
        // remaining propagation for nodes that receive injected values indirectly.
        for _ in 0..2 {
            for &id in &eval_order {
                let inputs = self.collect_inputs(id, values);
                let node = match self.nodes.get_mut(&id) { Some(n) => n, None => continue };
                match &mut node.node_type {
                    NodeType::Slider { value, min, max, .. } => {
                        if let Some(PortValue::Float(v)) = inputs.get(1) { *min = *v; }
                        if let Some(PortValue::Float(v)) = inputs.get(2) { *max = *v; }
                        if let Some(PortValue::Float(v)) = inputs.first() { *value = *v; }
                        values.insert((id, 0), PortValue::Float(*value));
                    }
                    NodeType::Add => {
                        let a = inputs.first().map(|v| v.as_float()).unwrap_or(0.0);
                        let b = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                        values.insert((id, 0), PortValue::Float(a + b));
                    }
                    NodeType::Multiply => {
                        let a = inputs.first().map(|v| v.as_float()).unwrap_or(0.0);
                        let b = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                        values.insert((id, 0), PortValue::Float(a * b));
                    }
                    NodeType::Math { result, .. } => {
                        values.insert((id, 0), PortValue::Float(*result as f32));
                    }
                    NodeType::Display { .. } => {
                        if let Some(v) = inputs.first() {
                            values.insert((id, 0), v.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Per-node evaluation kernel — contains all node logic.
    /// Extracted so that topo-ordered and cyclic-fallback passes share the same code.
    /// `continue` in the original match block is replaced by `return` here.
    fn evaluate_node(
        id: NodeId,
        node_type: &mut NodeType,
        inputs: &[PortValue],
        values: &mut HashMap<(NodeId, usize), PortValue>,
        real_dt: f32,
        now_secs: f64,
    ) {
        match node_type {
                    NodeType::Slider { value, min, max, .. } => {
                        // Override min/max from inputs if connected
                        if let Some(PortValue::Float(v)) = inputs.get(1) { *min = *v; }
                        if let Some(PortValue::Float(v)) = inputs.get(2) { *max = *v; }
                        // Override value from input — also update the stored value so slider UI moves
                        if let Some(PortValue::Float(v)) = inputs.first() {
                            *value = *v;
                        }
                        values.insert((id, 0), PortValue::Float(*value));
                    }
                    NodeType::Add => {
                        let a = inputs.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                        let b = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                        values.insert((id, 0), PortValue::Float(a + b));
                    }
                    NodeType::Multiply => {
                        let a = inputs.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                        let b = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                        values.insert((id, 0), PortValue::Float(a * b));
                    }
                    NodeType::Math { result, .. } => {
                        // Result is computed in render (uses Rhai for formula eval).
                        // Just propagate the stored result.
                        values.insert((id, 0), PortValue::Float(*result as f32));
                    }
                    NodeType::Gate { mode, threshold, else_value } => {
                        let val = inputs.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                        let thresh = inputs.get(1).map(|v| v.as_float()).unwrap_or(*threshold);
                        // Also update threshold from input so UI stays in sync
                        if inputs.get(1).is_some() { *threshold = thresh; }
                        let pass = match mode {
                            0 => val > thresh,
                            1 => val < thresh,
                            2 => val >= thresh,
                            3 => val <= thresh,
                            4 => (val - thresh).abs() < f32::EPSILON,
                            5 => (val - thresh).abs() >= f32::EPSILON,
                            _ => val > thresh,
                        };
                        let out = if pass { val } else { *else_value };
                        values.insert((id, 0), PortValue::Float(out));
                        values.insert((id, 1), PortValue::Float(if pass { 1.0 } else { 0.0 }));
                    }
                    NodeType::Timer { interval, elapsed, running, pulse_width,
                                       ref_time, paused_elapsed, time_initialized } => {
                        // Override interval from input port 0
                        if let Some(pv) = inputs.first() {
                            let v = pv.as_float();
                            if v > 0.0 { *interval = v; }
                        }
                        // Override interval from BPM input port 1
                        if let Some(pv) = inputs.get(1) {
                            let bpm_in = pv.as_float();
                            if bpm_in > 0.0 {
                                *interval = 60.0 / bpm_in;
                            }
                        }

                        // ── Wall-clock timing ──────────────────────────────
                        // On first frame or after deserialization, initialize ref_time
                        if !*time_initialized {
                            if *running {
                                *ref_time = now_secs;
                                *paused_elapsed = *elapsed as f64;
                            }
                            *time_initialized = true;
                        }

                        if *running {
                            // Compute elapsed from wall clock — no accumulation drift
                            *elapsed = ((now_secs - *ref_time) + *paused_elapsed) as f32;
                        }

                        let safe_interval = interval.max(0.01);
                        let phase = (*elapsed % safe_interval) / safe_interval;
                        let is_pulse = phase < (*pulse_width / safe_interval);
                        let trigger = if is_pulse && *running { 1.0 } else { 0.0 };
                        let bpm = 60.0 / safe_interval;
                        values.insert((id, 0), PortValue::Float(trigger));
                        values.insert((id, 1), PortValue::Float(phase));
                        values.insert((id, 2), PortValue::Float(bpm));
                    }
                    NodeType::MapRange { in_min, in_max, out_min, out_max, clamp } => {
                        let val = inputs.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                        // Override ranges from inputs
                        if let Some(v) = inputs.get(1) { *in_min = v.as_float(); }
                        if let Some(v) = inputs.get(2) { *in_max = v.as_float(); }
                        if let Some(v) = inputs.get(3) { *out_min = v.as_float(); }
                        if let Some(v) = inputs.get(4) { *out_max = v.as_float(); }

                        let t = (val - *in_min) / (*in_max - *in_min).max(0.001);
                        let t_final = if *clamp { t.clamp(0.0, 1.0) } else { t };
                        let mapped = *out_min + t_final * (*out_max - *out_min);
                        values.insert((id, 0), PortValue::Float(mapped));
                    }
                    NodeType::StringFormat { template, arg_count } => {
                        // Port 0 = template text (optional), ports 1..=arg_count = args
                        let effective_template = match inputs.first() {
                            Some(PortValue::Text(s)) if !s.is_empty() => s.clone(),
                            _ => template.clone(),
                        };
                        let mut result = effective_template;
                        for i in 0..*arg_count {
                            let port = i + 1;
                            let replacement = match inputs.get(port) {
                                Some(PortValue::Float(f)) => {
                                    let s = format!("{:.6}", f);
                                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                                }
                                Some(PortValue::Text(s)) => s.clone(),
                                _ => String::new(),
                            };
                            let placeholder = format!("{{{}}}", i);
                            result = result.replace(&placeholder, &replacement);
                        }
                        values.insert((id, 0), PortValue::Text(result));
                    }
                    NodeType::SampleHold { held_float, held_text, is_text, last_trigger, history } => {
                        // Input 0 = Value, Input 1 = Trigger
                        let trigger_val = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                        let rising_edge = trigger_val > 0.5 && *last_trigger <= 0.5;
                        *last_trigger = trigger_val;

                        if rising_edge {
                            if let Some(val) = inputs.first() {
                                match val {
                                    PortValue::Float(f) => {
                                        *held_float = *f;
                                        *is_text = false;
                                        history.push(*f);
                                        while history.len() > 40 { history.remove(0); }
                                    }
                                    PortValue::Text(t) => {
                                        *held_text = t.clone();
                                        *is_text = true;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Output held value
                        if *is_text {
                            values.insert((id, 0), PortValue::Text(held_text.clone()));
                        } else {
                            values.insert((id, 0), PortValue::Float(*held_float));
                        }
                        // Trigger echo: 1.0 on rising edge frame, else 0.0
                        values.insert((id, 1), PortValue::Float(if rising_edge { 1.0 } else { 0.0 }));
                    }
                    NodeType::Select { mode } => {
                        // Input 0 = A, Input 1 = B, Input 2 = Selector
                        let val_a = inputs.get(0).cloned().unwrap_or(PortValue::Float(0.0));
                        let val_b = inputs.get(1).cloned().unwrap_or(PortValue::Float(0.0));
                        let selector = inputs.get(2).map(|v| v.as_float()).unwrap_or(0.0).clamp(0.0, 1.0);
                        let b_active = selector >= 0.5;

                        let output = if *mode == 1 {
                            // Crossfade (float only)
                            match (&val_a, &val_b) {
                                (PortValue::Float(fa), PortValue::Float(fb)) => {
                                    PortValue::Float(fa * (1.0 - selector) + fb * selector)
                                }
                                _ => if b_active { val_b.clone() } else { val_a.clone() },
                            }
                        } else {
                            // Hard switch
                            if b_active { val_b.clone() } else { val_a.clone() }
                        };

                        values.insert((id, 0), output);
                        values.insert((id, 1), PortValue::Float(if b_active { 1.0 } else { 0.0 }));
                    }
                    NodeType::Curve { points, mode, speed, looping, phase, playing, last_trigger, .. } => {
                        // Override speed from input port 2
                        if let Some(pv) = inputs.get(2) {
                            let v = pv.as_float();
                            if v > 0.0 { *speed = v; }
                        }
                        // Gate input (port 3) — freeze phase while high
                        let gate_high = inputs.get(3).map(|v| v.as_float() > 0.5).unwrap_or(false);

                        let x_pos = match *mode {
                            0 => {
                                // Manual: use X input directly
                                inputs.first().map(|v| v.as_float()).unwrap_or(0.0).clamp(0.0, 1.0)
                            }
                            1 | 2 => {
                                // Envelope / LFO: trigger-driven playback
                                let trig_val = inputs.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                                let rising = trig_val > 0.5 && *last_trigger <= 0.5;
                                *last_trigger = trig_val;

                                if rising {
                                    *phase = 0.0;
                                    *playing = true;
                                }

                                if *playing && !gate_high {
                                    *phase += real_dt * *speed;
                                    if *phase >= 1.0 {
                                        if *mode == 2 || *looping {
                                            *phase = *phase % 1.0; // loop
                                        } else {
                                            *phase = 1.0;
                                            *playing = false;
                                        }
                                    }
                                }
                                *phase
                            }
                            _ => 0.0,
                        };

                        let y_val = crate::nodes::curve::evaluate_curve(points, x_pos);
                        let at_end = !*playing && *phase >= 1.0 && *mode >= 1;

                        values.insert((id, 0), PortValue::Float(y_val));
                        values.insert((id, 1), PortValue::Float(x_pos));
                        values.insert((id, 2), PortValue::Float(if at_end { 1.0 } else { 0.0 }));
                        // Image (port 3) handled in app.rs image pass
                    }
                    NodeType::MouseTracker { x, y } => {
                        values.insert((id, 0), PortValue::Float(*x));
                        values.insert((id, 1), PortValue::Float(*y));
                    }
                    NodeType::Time { elapsed, speed, running } => {
                        if *running {
                            *elapsed += real_dt * *speed;
                        }
                        values.insert((id, 0), PortValue::Float(*elapsed));
                        values.insert((id, 1), PortValue::Float(*elapsed % 1.0));
                    }
                    NodeType::Color { r, g, b } => {
                        values.insert((id, 0), PortValue::Float(*r as f32));
                        values.insert((id, 1), PortValue::Float(*g as f32));
                        values.insert((id, 2), PortValue::Float(*b as f32));
                    }
                    NodeType::File { content, .. } => {
                        values.insert((id, 0), PortValue::Text(content.clone()));
                    }
                    NodeType::FolderBrowser { selected_file, .. } => {
                        // Output the selected file path and name
                        values.insert((id, 0), PortValue::Text(selected_file.clone()));
                        let name = std::path::Path::new(selected_file.as_str())
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        values.insert((id, 1), PortValue::Text(name));
                        // Read content (lazy, only when file selected)
                        if !selected_file.is_empty() {
                            let content = std::fs::read_to_string(selected_file).unwrap_or_default();
                            values.insert((id, 2), PortValue::Text(content));
                        }
                    }
                    NodeType::TextEditor { content } => {
                        if matches!(inputs.first(), Some(PortValue::Text(_))) {
                            values.insert((id, 0), inputs[0].clone());
                        } else {
                            values.insert((id, 0), PortValue::Text(content.clone()));
                        }
                    }
                    NodeType::MidiIn { channel, note, velocity, .. } => {
                        values.insert((id, 0), PortValue::Float(*channel as f32));
                        values.insert((id, 1), PortValue::Float(*note as f32));
                        values.insert((id, 2), PortValue::Float(*velocity as f32));
                    }
                    NodeType::Theme { bg_color, text_color, accent, .. } => {
                        values.insert((id, 0), PortValue::Float(bg_color[0] as f32));
                        values.insert((id, 1), PortValue::Float(bg_color[1] as f32));
                        values.insert((id, 2), PortValue::Float(bg_color[2] as f32));
                        values.insert((id, 3), PortValue::Float(text_color[0] as f32));
                        values.insert((id, 4), PortValue::Float(text_color[1] as f32));
                        values.insert((id, 5), PortValue::Float(text_color[2] as f32));
                        values.insert((id, 6), PortValue::Float(accent[0] as f32));
                        values.insert((id, 7), PortValue::Float(accent[1] as f32));
                        values.insert((id, 8), PortValue::Float(accent[2] as f32));
                    }
                    NodeType::Serial { last_line, .. } => {
                        values.insert((id, 0), PortValue::Text(last_line.clone()));
                    }
                    NodeType::Script { input_names, output_names, code, last_values, error, continuous, trigger, .. } => {
                        // Code port: if Code input is connected, use that; else use inline code
                        // Port layout: [Exec? (if manual)] [Code] [user inputs...]
                        let code_port_idx: usize = if *continuous { 0 } else { 1 };
                        let code_from_port = match inputs.get(code_port_idx) {
                            Some(PortValue::Text(s)) if !s.is_empty() => Some(s.clone()),
                            _ => None,
                        };
                        let effective_code = code_from_port.as_deref().unwrap_or(code.as_str());

                        if effective_code.is_empty() || output_names.is_empty() {
                            for (i, v) in last_values.iter().enumerate() {
                                values.insert((id, i), PortValue::Float(*v));
                            }
                            return; // emit last values and skip eval for this node
                        }

                        // In manual mode, only run if triggered
                        let should_run = if *continuous {
                            true
                        } else {
                            let exec_val = inputs.first().map(|v| v.as_float()).unwrap_or(0.0);
                            let fired = exec_val > 0.5 || *trigger;
                            *trigger = false;
                            fired
                        };

                        if !should_run {
                            for (i, v) in last_values.iter().enumerate() {
                                values.insert((id, i), PortValue::Float(*v));
                            }
                            return; // emit last values and skip eval for this node
                        }

                        let engine = rhai::Engine::new();
                        // User inputs start after [Exec?] [Code] ports
                        let input_offset: usize = code_port_idx + 1;
                        let in_vars: Vec<String> = input_names.iter().enumerate().map(|(i, name)| {
                            let val = inputs.get(i + input_offset).map(|v| v.as_float()).unwrap_or(0.0);
                            format!("let {} = {};", name, val)
                        }).collect();
                        // Declare output variables initialized to 0.0
                        let out_vars: Vec<String> = output_names.iter()
                            .map(|name| format!("let {} = 0.0;", name))
                            .collect();
                        // After user code, collect output variables into array
                        let collect_outputs = format!("[{}]",
                            output_names.join(", ")
                        );
                        let full_script = format!(
                            "{}\n{}\n{}\n{}",
                            in_vars.join("\n"),
                            out_vars.join("\n"),
                            effective_code,
                            collect_outputs
                        );
                        match engine.eval::<rhai::Array>(&full_script) {
                            Ok(arr) => {
                                error.clear();
                                last_values.clear();
                                for (i, val) in arr.iter().enumerate() {
                                    if i < output_names.len() {
                                        let f = val.as_float().unwrap_or(0.0) as f32;
                                        values.insert((id, i), PortValue::Float(f));
                                        last_values.push(f);
                                    }
                                }
                            }
                            Err(e) => {
                                *error = e.to_string();
                            }
                        }
                    }
                    NodeType::HttpRequest { response, status, .. } => {
                        values.insert((id, 0), PortValue::Text(response.clone()));
                        // Parse status code from status string (e.g., "200 OK" → 200.0)
                        let code = status.split_whitespace().next()
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        values.insert((id, 1), PortValue::Float(code));
                    }
                    NodeType::AiRequest { response, status, .. } => {
                        values.insert((id, 0), PortValue::Text(response.clone()));
                        let code = if status.contains("done") { 1.0 }
                            else if status.contains("error") { -1.0 }
                            else if status.contains("thinking") { 0.5 }
                            else { 0.0 };
                        values.insert((id, 1), PortValue::Float(code));
                    }
                    NodeType::JsonExtract { path, .. } => {
                        let json_text = match inputs.first() {
                            Some(PortValue::Text(s)) => s.clone(),
                            _ => String::new(),
                        };
                        let extracted = if !json_text.is_empty() && !path.is_empty() {
                            extract_json_path(&json_text, path)
                        } else {
                            String::new()
                        };
                        values.insert((id, 0), PortValue::Text(extracted));
                    }
                    NodeType::Console { .. } => {}
                    NodeType::Monitor => {}
                    NodeType::OscOut { .. } => {}
                    NodeType::OscIn { last_args, last_args_text, arg_count, address_filter, .. } => {
                        for i in 0..*arg_count {
                            let v = last_args.get(i).copied().unwrap_or(0.0);
                            values.insert((id, i), PortValue::Float(v));
                        }
                        // Raw text output: all args joined
                        let raw = last_args_text.join(", ");
                        values.insert((id, *arg_count), PortValue::Text(raw));
                        // Address output
                        values.insert((id, *arg_count + 1), PortValue::Text(address_filter.clone()));
                    }
                    NodeType::KeyInput { pressed, toggled_on, .. } => {
                        values.insert((id, 0), PortValue::Float(if *pressed { 1.0 } else { 0.0 }));
                        values.insert((id, 1), PortValue::Float(if *pressed { 1.0 } else { 0.0 }));
                        values.insert((id, 2), PortValue::Float(if *toggled_on { 1.0 } else { 0.0 }));
                    }
                    _ => {}
                }
    }

    /// Evaluate the graph. `now_secs` is the monotonic wall-clock time in seconds
    /// (from `std::time::Instant` elapsed since app start). Used by Timer for drift-free tempo.
    pub fn evaluate(&mut self, now_secs: f64) -> HashMap<(NodeId, usize), PortValue> {
        // Compute real frame dt from wall clock (clamped to avoid huge jumps).
        let real_dt = if self.last_eval_time > 0.0 {
            ((now_secs - self.last_eval_time) as f32).clamp(0.0001, 0.25)
        } else {
            1.0 / 60.0  // first-frame fallback
        };
        self.last_eval_time = now_secs;

        // Rebuild topo order only when graph topology has changed.
        if self.topo_dirty || self.topo_order.is_empty() {
            self.rebuild_topo_order();
        }
        // Clone ID vecs into locals so we can mutably borrow self.nodes inside the loop.
        let eval_order = self.topo_order.clone();
        let cyclic_ids = self.cyclic_nodes.clone();

        let mut values: HashMap<(NodeId, usize), PortValue> = HashMap::new();

        // ── Single topo-ordered pass ─────────────────────────────────────────
        // All predecessors of any node are evaluated before it, so one pass is
        // correct and O(n) for any acyclic graph.
        for &id in &eval_order {
            let inputs = self.collect_inputs(id, &values);
            if let Some(node) = self.nodes.get_mut(&id) {
                Self::evaluate_node(id, &mut node.node_type, &inputs, &mut values, real_dt, now_secs);
            }
        }

        // ── Extra passes for cyclic nodes (feedback loops) ───────────────────
        // Kahn's algorithm could not linearize these nodes. Two extra passes let
        // values propagate around short cycles without running the full graph again.
        if !cyclic_ids.is_empty() {
            for _ in 0..2 {
                for &id in &cyclic_ids {
                    let inputs = self.collect_inputs(id, &values);
                    if let Some(node) = self.nodes.get_mut(&id) {
                        Self::evaluate_node(id, &mut node.node_type, &inputs, &mut values, real_dt, now_secs);
                    }
                }
            }
        }

        values
    }

    pub fn collect_inputs(&self, node_id: NodeId, values: &HashMap<(NodeId, usize), PortValue>) -> Vec<PortValue> {
        let num = self.nodes.get(&node_id).map(|n| n.node_type.inputs().len()).unwrap_or(0);
        let mut inputs = vec![PortValue::None; num];
        for conn in &self.connections {
            if conn.to_node == node_id && conn.to_port < num {
                if let Some(val) = values.get(&(conn.from_node, conn.from_port)) {
                    inputs[conn.to_port] = val.clone();
                }
            }
        }
        inputs
    }

    pub fn static_input_value(connections: &[Connection], values: &HashMap<(NodeId, usize), PortValue>, node_id: NodeId, port_idx: usize) -> PortValue {
        for c in connections {
            if c.to_node == node_id && c.to_port == port_idx {
                return values.get(&(c.from_node, c.from_port)).cloned().unwrap_or(PortValue::None);
            }
        }
        PortValue::None
    }
}

/// Walk a JSON value using dot-separated path (e.g., "choices.0.message.content")
fn extract_json_path(json_str: &str, path: &str) -> String {
    let Ok(mut val) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return format!("(parse error)");
    };
    for segment in path.split('.') {
        val = if let Ok(idx) = segment.parse::<usize>() {
            val.get(idx).cloned().unwrap_or(serde_json::Value::Null)
        } else {
            val.get(segment).cloned().unwrap_or(serde_json::Value::Null)
        };
    }
    match &val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
