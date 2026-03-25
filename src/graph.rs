use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_true() -> bool { true }
fn default_bg_color() -> [u8; 3] { [30, 30, 30] }
fn default_text_color() -> [u8; 3] { [220, 220, 220] }
fn default_window_bg() -> [u8; 3] { [40, 40, 40] }
fn default_window_alpha() -> u8 { 240 }
fn default_grid_color() -> [u8; 3] { [12, 12, 12] }
fn default_rounding() -> f32 { 4.0 }
fn default_spacing() -> f32 { 4.0 }
fn default_scope_history() -> Vec<f32> { Vec::new() }
fn default_scope_length() -> usize { 200 }
fn default_scope_min() -> f32 { 0.0 }
fn default_scope_max() -> f32 { 1.0 }
fn default_scope_height() -> f32 { 80.0 }
fn default_canvas_w() -> f32 { 800.0 }
fn default_canvas_h() -> f32 { 600.0 }
fn default_resolution() -> u32 { 120 }
fn default_max_tokens() -> u32 { 1024 }

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PortValue {
    Float(f32),
    Text(String),
    None,
}

impl std::fmt::Display for PortValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortValue::Float(v) => write!(f, "{:.3}", v),
            PortValue::Text(s) => {
                if s.len() > 24 { write!(f, "\"{}...\"", &s[..24]) }
                else { write!(f, "\"{}\"", s) }
            }
            PortValue::None => write!(f, "\u{2014}"),
        }
    }
}

impl PortValue {
    pub fn as_float(&self) -> f32 {
        match self { PortValue::Float(v) => *v, _ => 0.0 }
    }
}

pub struct PortDef { pub name: &'static str }

// ── MIDI mode ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiMode { Note, CC }

// ── Node types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Slider { value: f32, min: f32, max: f32 },
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
    },
    Add,
    Multiply,
    File { path: String, content: String },
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
        #[serde(default = "default_rounding")]
        rounding: f32,
        #[serde(default = "default_spacing")]
        spacing: f32,
        #[serde(default)]
        use_hsl: bool,
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
    Comment { text: String },
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
        #[serde(default)]
        log: Vec<String>,
        #[serde(default)]
        listening: bool,
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
        #[serde(default)]
        provider: String,      // "anthropic" / "openai" / "custom"
        #[serde(default)]
        model: String,
        #[serde(default)]
        response: String,
        #[serde(default)]
        status: String,
        #[serde(default = "default_max_tokens")]
        max_tokens: u32,
        #[serde(default)]
        temperature: f32,
        #[serde(default)]
        api_key_name: String,  // Key name in project api_keys.json
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
    },
    AudioPlayer {
        #[serde(default)]
        file_path: String,
        #[serde(default = "default_one")]
        volume: f32,
        #[serde(default)]
        looping: bool,
    },
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
}

fn default_device_id() -> u8 { 1 }
fn default_440() -> f32 { 440.0 }
fn default_half() -> f32 { 0.5 }
fn default_one() -> f32 { 1.0 }
fn default_point_eight() -> f32 { 0.8 }

impl NodeType {
    pub fn title(&self) -> &str {
        match self {
            NodeType::Slider { .. } => "Slider",
            NodeType::Display { .. } => "Display",
            NodeType::Add => "Add",
            NodeType::Multiply => "Multiply",
            NodeType::File { .. } => "File",
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
            NodeType::KeyInput { .. } => "Key Input",
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
            NodeType::AudioDevice { .. } => "Audio Device",
            NodeType::AudioFx { .. } => "Audio FX",
            NodeType::RustPlugin { .. } => "Rust Plugin",
        }
    }

    pub fn inputs(&self) -> Vec<PortDef> {
        match self {
            NodeType::Slider { .. } => vec![PortDef { name: "In" }, PortDef { name: "Min" }, PortDef { name: "Max" }],
            NodeType::Display { .. } => vec![PortDef { name: "Value" }],
            NodeType::Add => vec![PortDef { name: "A" }, PortDef { name: "B" }],
            NodeType::Multiply => vec![PortDef { name: "A" }, PortDef { name: "B" }],
            NodeType::File { .. } => vec![],
            NodeType::TextEditor { .. } => vec![PortDef { name: "Text In" }],
            NodeType::WgslViewer { uniform_names, uniform_types, .. } => {
                let mut ports = vec![PortDef { name: "WGSL" }];
                for (i, n) in uniform_names.iter().enumerate() {
                    let t = uniform_types.get(i).map(|s| s.as_str()).unwrap_or("float");
                    if t == "color" {
                        // Color takes 3 ports: R, G, B
                        ports.push(PortDef { name: Box::leak(format!("{} R", n).into_boxed_str()) });
                        ports.push(PortDef { name: Box::leak(format!("{} G", n).into_boxed_str()) });
                        ports.push(PortDef { name: Box::leak(format!("{} B", n).into_boxed_str()) });
                    } else {
                        ports.push(PortDef { name: Box::leak(n.clone().into_boxed_str()) });
                    }
                }
                ports
            }
            NodeType::Time { .. } => vec![],
            NodeType::Color { .. } => vec![],
            NodeType::MouseTracker { .. } => vec![],
            NodeType::MidiOut { mode, .. } => match mode {
                MidiMode::Note => vec![
                    PortDef { name: "Channel" },
                    PortDef { name: "Note" },
                    PortDef { name: "Velocity" },
                ],
                MidiMode::CC => vec![
                    PortDef { name: "Channel" },
                    PortDef { name: "CC#" },
                    PortDef { name: "Value" },
                ],
            },
            NodeType::MidiIn { .. } => vec![],
            NodeType::Theme { .. } => vec![
                PortDef { name: "BG R" }, PortDef { name: "BG G" }, PortDef { name: "BG B" },
                PortDef { name: "Text R" }, PortDef { name: "Text G" }, PortDef { name: "Text B" },
                PortDef { name: "Accent R" }, PortDef { name: "Accent G" }, PortDef { name: "Accent B" },
                PortDef { name: "Win R" }, PortDef { name: "Win G" }, PortDef { name: "Win B" },
                PortDef { name: "Grid R" }, PortDef { name: "Grid G" }, PortDef { name: "Grid B" },
                PortDef { name: "Font Size" },
                PortDef { name: "Rounding" },
                PortDef { name: "Spacing" },
                PortDef { name: "Win Alpha" },
            ],
            NodeType::Serial { .. } => vec![PortDef { name: "Send" }],
            NodeType::Comment { .. } => vec![],
            NodeType::Console { .. } => vec![],
            NodeType::Monitor => vec![],
            NodeType::OscOut { arg_count, .. } => {
                (0..*arg_count).map(|i| PortDef { name: Box::leak(format!("Arg {}", i).into_boxed_str()) }).collect()
            }
            NodeType::OscIn { .. } => vec![],
            NodeType::KeyInput { .. } => vec![],
            NodeType::Palette { .. } => vec![],
            NodeType::HttpRequest { .. } => vec![
                PortDef { name: "URL" },
                PortDef { name: "Body" },
                PortDef { name: "Headers" },
            ],
            NodeType::AiRequest { .. } => vec![
                PortDef { name: "Config" },
                PortDef { name: "System" },
                PortDef { name: "Prompt" },
            ],
            NodeType::JsonExtract { .. } => vec![PortDef { name: "JSON" }],
            NodeType::FileMenu => vec![],
            NodeType::ZoomControl { .. } => vec![PortDef { name: "Zoom" }],
            NodeType::ObHub { .. } => vec![PortDef { name: "Command" }],
            NodeType::ObJoystick { .. } => vec![],
            NodeType::ObEncoder { .. } => vec![],
            NodeType::Synth { .. } => vec![
                PortDef { name: "Freq" },
                PortDef { name: "Amp" },
                PortDef { name: "Gate" },
            ],
            NodeType::AudioPlayer { .. } => vec![],
            NodeType::AudioDevice { .. } => vec![],
            NodeType::AudioFx { .. } => vec![PortDef { name: "Source" }],
            NodeType::RustPlugin { input_names, .. } => {
                input_names.iter().map(|n| PortDef { name: Box::leak(n.clone().into_boxed_str()) }).collect()
            }
            NodeType::Script { input_names, continuous, .. } => {
                let mut ports: Vec<PortDef> = Vec::new();
                if !continuous {
                    ports.push(PortDef { name: "Exec" });
                }
                ports.push(PortDef { name: "Code" }); // Code from input port
                for n in input_names {
                    ports.push(PortDef { name: Box::leak(n.clone().into_boxed_str()) });
                }
                ports
            }
        }
    }

    pub fn outputs(&self) -> Vec<PortDef> {
        match self {
            NodeType::Slider { .. } => vec![PortDef { name: "Value" }],
            NodeType::Display { .. } => vec![],
            NodeType::Add => vec![PortDef { name: "Result" }],
            NodeType::Multiply => vec![PortDef { name: "Result" }],
            NodeType::File { .. } => vec![PortDef { name: "Content" }],
            NodeType::TextEditor { .. } => vec![PortDef { name: "Text Out" }],
            NodeType::WgslViewer { .. } => vec![],
            NodeType::Time { .. } => vec![PortDef { name: "Seconds" }, PortDef { name: "Beat" }],
            NodeType::Color { .. } => vec![PortDef { name: "R" }, PortDef { name: "G" }, PortDef { name: "B" }],
            NodeType::MouseTracker { .. } => vec![PortDef { name: "X" }, PortDef { name: "Y" }],
            NodeType::MidiOut { .. } => vec![],
            NodeType::MidiIn { .. } => vec![
                PortDef { name: "Channel" },
                PortDef { name: "Note" },
                PortDef { name: "Velocity" },
            ],
            NodeType::Theme { .. } => vec![
                PortDef { name: "BG R" }, PortDef { name: "BG G" }, PortDef { name: "BG B" },
                PortDef { name: "Text R" }, PortDef { name: "Text G" }, PortDef { name: "Text B" },
                PortDef { name: "Accent R" }, PortDef { name: "Accent G" }, PortDef { name: "Accent B" },
            ],
            NodeType::Serial { .. } => vec![PortDef { name: "Send" }],
            NodeType::Comment { .. } => vec![],
            NodeType::Console { .. } => vec![],
            NodeType::Monitor => vec![
                PortDef { name: "FPS" },
                PortDef { name: "Frame ms" },
                PortDef { name: "Nodes" },
            ],
            NodeType::OscOut { .. } => vec![],
            NodeType::OscIn { arg_count, .. } => {
                (0..*arg_count).map(|i| PortDef { name: Box::leak(format!("Arg {}", i).into_boxed_str()) }).collect()
            }
            NodeType::KeyInput { .. } => vec![
                PortDef { name: "Trigger" },
                PortDef { name: "Held" },
                PortDef { name: "Toggle" },
            ],
            NodeType::Script { output_names, .. } => {
                output_names.iter().map(|n| PortDef { name: Box::leak(n.clone().into_boxed_str()) }).collect()
            }
            NodeType::Palette { .. } => vec![],
            NodeType::HttpRequest { .. } => vec![
                PortDef { name: "Response" },
                PortDef { name: "Status" },
            ],
            NodeType::AiRequest { .. } => vec![
                PortDef { name: "Response" },
                PortDef { name: "Status" },
            ],
            NodeType::JsonExtract { .. } => vec![PortDef { name: "Value" }],
            NodeType::FileMenu => vec![],
            NodeType::ZoomControl { .. } => vec![PortDef { name: "Zoom" }],
            NodeType::ObHub { detected_devices, .. } => {
                let mut ports = Vec::new();
                let mut sorted = detected_devices.clone();
                sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                for (dtype, id) in &sorted {
                    match dtype.as_str() {
                        "joystick" => {
                            ports.push(PortDef { name: Box::leak(format!("j{}_x", id).into_boxed_str()) });
                            ports.push(PortDef { name: Box::leak(format!("j{}_y", id).into_boxed_str()) });
                            ports.push(PortDef { name: Box::leak(format!("j{}_btn", id).into_boxed_str()) });
                        }
                        "encoder" => {
                            ports.push(PortDef { name: Box::leak(format!("e{}_turn", id).into_boxed_str()) });
                            ports.push(PortDef { name: Box::leak(format!("e{}_click", id).into_boxed_str()) });
                            ports.push(PortDef { name: Box::leak(format!("e{}_pos", id).into_boxed_str()) });
                        }
                        other => {
                            ports.push(PortDef { name: Box::leak(format!("{}{}_{}", &other[..1], id, "val").into_boxed_str()) });
                        }
                    }
                }
                if ports.is_empty() {
                    ports.push(PortDef { name: "(no devices)" });
                }
                ports
            },
            NodeType::ObJoystick { .. } => vec![
                PortDef { name: "X" },
                PortDef { name: "Y" },
                PortDef { name: "Button" },
            ],
            NodeType::ObEncoder { .. } => vec![
                PortDef { name: "Turn" },
                PortDef { name: "Click" },
                PortDef { name: "Position" },
            ],
            NodeType::Synth { .. } => vec![PortDef { name: "Audio" }],
            NodeType::AudioPlayer { .. } => vec![],
            NodeType::AudioDevice { .. } => vec![],
            NodeType::AudioFx { .. } => vec![PortDef { name: "Audio" }],
            NodeType::RustPlugin { output_names, .. } => {
                output_names.iter().map(|n| PortDef { name: Box::leak(n.clone().into_boxed_str()) }).collect()
            }
        }
    }

    pub fn color_hint(&self) -> [u8; 3] {
        match self {
            NodeType::Slider { .. } => [80, 160, 255],
            NodeType::Display { .. } => [100, 200, 100],
            NodeType::Add | NodeType::Multiply => [200, 160, 80],
            NodeType::File { .. } => [180, 120, 200],
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
            NodeType::AudioDevice { .. } => [220, 180, 100],
            NodeType::AudioFx { .. } => [200, 100, 160],
            NodeType::RustPlugin { .. } => [255, 120, 50],
        }
    }

    /// Whether this node renders its ports inline within the content
    /// instead of as separate lists at top/bottom.
    pub fn inline_ports(&self) -> bool {
        matches!(self, NodeType::Theme { .. })
    }
}

// ── Node & Connection ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node { pub id: NodeId, pub node_type: NodeType, pub pos: [f32; 2] }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub from_node: NodeId, pub from_port: usize,
    pub to_node: NodeId, pub to_port: usize,
}

// ── Graph ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: HashMap<NodeId, Node>,
    pub connections: Vec<Connection>,
    next_id: u64,
}

impl Graph {
    pub fn new() -> Self {
        Self { nodes: HashMap::new(), connections: Vec::new(), next_id: 1 }
    }
    pub fn add_node(&mut self, node_type: NodeType, pos: [f32; 2]) -> NodeId {
        let id = self.next_id; self.next_id += 1;
        self.nodes.insert(id, Node { id, node_type, pos }); id
    }
    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        self.connections.retain(|c| c.from_node != id && c.to_node != id);
    }
    pub fn remove_connections_to_port(&mut self, node_id: NodeId, port: usize) {
        self.connections.retain(|c| !(c.to_node == node_id && c.to_port == port));
    }
    pub fn add_connection(&mut self, from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize) {
        self.connections.retain(|c| !(c.to_node == to_node && c.to_port == to_port));
        self.connections.push(Connection { from_node, from_port, to_node, to_port });
    }

    /// Re-evaluate with pre-existing values (for injected hardware data).
    /// Runs 2 passes to propagate injected values through downstream nodes.
    pub fn evaluate_with_existing(&mut self, values: &mut HashMap<(NodeId, usize), PortValue>) {
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for _ in 0..2 {
            for &id in &ids {
                let inputs = self.collect_inputs(id, values);
                let node = match self.nodes.get_mut(&id) { Some(n) => n, None => continue };
                match &mut node.node_type {
                    NodeType::Slider { value, min, max } => {
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

    pub fn evaluate(&mut self) -> HashMap<(NodeId, usize), PortValue> {
        let mut values: HashMap<(NodeId, usize), PortValue> = HashMap::new();
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();

        for _ in 0..5 {
            for &id in &ids {
                let inputs = self.collect_inputs(id, &values);
                let node = match self.nodes.get_mut(&id) { Some(n) => n, None => continue };
                match &mut node.node_type {
                    NodeType::Slider { value, min, max } => {
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
                    NodeType::MouseTracker { x, y } => {
                        values.insert((id, 0), PortValue::Float(*x));
                        values.insert((id, 1), PortValue::Float(*y));
                    }
                    NodeType::Time { elapsed, speed, running } => {
                        if *running {
                            *elapsed += (1.0 / 60.0) * *speed;
                        }
                        values.insert((id, 0), PortValue::Float(*elapsed));
                        // Beat output: fractional part for looping animations
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
                            continue;
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
                            continue;
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
                    NodeType::OscIn { last_args, arg_count, .. } => {
                        for i in 0..*arg_count {
                            let v = last_args.get(i).copied().unwrap_or(0.0);
                            values.insert((id, i), PortValue::Float(v));
                        }
                    }
                    NodeType::KeyInput { pressed, toggled_on, .. } => {
                        values.insert((id, 0), PortValue::Float(if *pressed { 1.0 } else { 0.0 }));
                        values.insert((id, 1), PortValue::Float(if *pressed { 1.0 } else { 0.0 }));
                        values.insert((id, 2), PortValue::Float(if *toggled_on { 1.0 } else { 0.0 }));
                    }
                    _ => {}
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
