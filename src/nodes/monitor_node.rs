//! MonitorNode — standalone struct implementing NodeBehavior.
//!
//! System profiler: FPS, CPU, RAM, GPU, process memory, node/connection count.
//! Background thread collects sysinfo metrics every 1 second.

use crate::graph::{PortDef, PortKind, PortValue};
use crate::node_trait::NodeBehavior;
use serde::{Serialize, Deserialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// System metrics collected by background thread
#[derive(Clone, Default)]
struct SystemMetrics {
    cpu_usage: f32,
    cpu_per_core: Vec<f32>,
    mem_used_gb: f32,
    mem_total_gb: f32,
    mem_percent: f32,
    gpu_name: String,
    process_mem_mb: f32,
    process_cpu: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MonitorNode {
    // Serialized fields (minimal — just need to reconstruct)
    #[serde(skip, default = "default_metrics")]
    metrics: Arc<Mutex<SystemMetrics>>,
    #[serde(skip)]
    fps_history: VecDeque<f32>,
    #[serde(skip)]
    cpu_history: VecDeque<f32>,
    #[serde(skip)]
    mem_history: VecDeque<f32>,
    #[serde(skip)]
    process_mem_history: VecDeque<f32>,
    #[serde(skip)]
    frame_times: VecDeque<f32>,
    #[serde(skip, default = "Instant::now")]
    last_frame: Instant,
    #[serde(skip)]
    node_count: usize,
    #[serde(skip)]
    connection_count: usize,
    #[serde(skip)]
    thread_started: bool,
}

fn default_metrics() -> Arc<Mutex<SystemMetrics>> {
    Arc::new(Mutex::new(SystemMetrics::default()))
}

impl std::fmt::Debug for MonitorNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorNode").finish()
    }
}

impl Default for MonitorNode {
    fn default() -> Self {
        Self {
            metrics: default_metrics(),
            fps_history: VecDeque::with_capacity(120),
            cpu_history: VecDeque::with_capacity(120),
            mem_history: VecDeque::with_capacity(120),
            process_mem_history: VecDeque::with_capacity(120),
            frame_times: VecDeque::with_capacity(120),
            last_frame: Instant::now(),
            node_count: 0,
            connection_count: 0,
            thread_started: false,
        }
    }
}

impl MonitorNode {
    fn ensure_thread(&mut self) {
        if self.thread_started { return; }
        self.thread_started = true;

        let metrics = self.metrics.clone();
        std::thread::Builder::new()
            .name("monitor-sysinfo".to_string())
            .spawn(move || {
                use sysinfo::{System, Pid};
                let mut sys = System::new_all();
                let pid = Pid::from_u32(std::process::id());

                // GPU name (once)
                {
                    let name = get_gpu_name();
                    if let Ok(mut m) = metrics.lock() { m.gpu_name = name; }
                }

                loop {
                    sys.refresh_all();
                    let cpu = sys.global_cpu_usage();
                    let cores: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
                    let mem_used = sys.used_memory() as f64 / 1_073_741_824.0;
                    let mem_total = sys.total_memory() as f64 / 1_073_741_824.0;
                    let mem_pct = if mem_total > 0.0 { (mem_used / mem_total * 100.0) as f32 } else { 0.0 };
                    let (proc_mem, proc_cpu) = sys.process(pid)
                        .map(|p| (p.memory() as f32 / 1_048_576.0, p.cpu_usage()))
                        .unwrap_or((0.0, 0.0));

                    if let Ok(mut m) = metrics.lock() {
                        m.cpu_usage = cpu;
                        m.cpu_per_core = cores;
                        m.mem_used_gb = mem_used as f32;
                        m.mem_total_gb = mem_total as f32;
                        m.mem_percent = mem_pct;
                        m.process_mem_mb = proc_mem;
                        m.process_cpu = proc_cpu;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }).ok();
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if dt > 0.0 {
            self.fps_history.push_back(1.0 / dt);
            self.frame_times.push_back(dt * 1000.0);
            if self.fps_history.len() > 120 { self.fps_history.pop_front(); }
            if self.frame_times.len() > 120 { self.frame_times.pop_front(); }
        }

        if let Ok(m) = self.metrics.lock() {
            self.cpu_history.push_back(m.cpu_usage);
            self.mem_history.push_back(m.mem_percent);
            self.process_mem_history.push_back(m.process_mem_mb);
            if self.cpu_history.len() > 120 { self.cpu_history.pop_front(); }
            if self.mem_history.len() > 120 { self.mem_history.pop_front(); }
            if self.process_mem_history.len() > 120 { self.process_mem_history.pop_front(); }
        }
    }
}

impl NodeBehavior for MonitorNode {
    fn title(&self) -> &str { "Monitor" }

    fn inputs(&self) -> Vec<PortDef> { vec![] }

    fn outputs(&self) -> Vec<PortDef> {
        vec![
            PortDef::new("FPS", PortKind::Number),
            PortDef::new("Frame ms", PortKind::Number),
            PortDef::new("CPU %", PortKind::Number),
            PortDef::new("RAM %", PortKind::Number),
            PortDef::new("Proc MB", PortKind::Number),
            PortDef::new("Nodes", PortKind::Number),
        ]
    }

    fn color_hint(&self) -> [u8; 3] { [255, 160, 60] }

    fn evaluate(&mut self, _inputs: &[PortValue]) -> Vec<(usize, PortValue)> {
        self.ensure_thread();
        self.tick();

        let fps = self.fps_history.back().copied().unwrap_or(0.0);
        let frame_ms = self.frame_times.back().copied().unwrap_or(0.0);
        let (cpu, ram, proc_mb) = self.metrics.lock()
            .map(|m| (m.cpu_usage, m.mem_percent, m.process_mem_mb))
            .unwrap_or((0.0, 0.0, 0.0));

        vec![
            (0, PortValue::Float(fps)),
            (1, PortValue::Float(frame_ms)),
            (2, PortValue::Float(cpu)),
            (3, PortValue::Float(ram)),
            (4, PortValue::Float(proc_mb)),
            (5, PortValue::Float(self.node_count as f32)),
        ]
    }

    fn type_tag(&self) -> &str { "monitor" }

    fn save_state(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn render_ui(&mut self, ui: &mut eframe::egui::Ui) {
        use eframe::egui;

        self.ensure_thread();

        let m = self.metrics.lock().ok().map(|m| m.clone()).unwrap_or_default();
        let fps = self.fps_history.back().copied().unwrap_or(0.0);
        let frame_ms = self.frame_times.back().copied().unwrap_or(0.0);

        // FPS
        let fps_color = if fps >= 55.0 { egui::Color32::from_rgb(80, 200, 80) }
            else if fps >= 30.0 { egui::Color32::from_rgb(200, 200, 80) }
            else { egui::Color32::from_rgb(255, 80, 80) };

        ui.horizontal(|ui| {
            ui.label("FPS:");
            ui.colored_label(fps_color, egui::RichText::new(format!("{:.0}", fps)).strong());
            ui.label(egui::RichText::new(format!("{:.1}ms", frame_ms)).small()
                .color(ui.visuals().widgets.noninteractive.fg_stroke.color));
        });
        draw_sparkline(ui, &self.fps_history, fps_color, 0.0, 120.0);

        ui.separator();

        // CPU
        let cpu_color = if m.cpu_usage < 50.0 { egui::Color32::from_rgb(80, 180, 255) }
            else if m.cpu_usage < 80.0 { egui::Color32::from_rgb(200, 200, 80) }
            else { egui::Color32::from_rgb(255, 80, 80) };

        ui.horizontal(|ui| {
            ui.label("CPU:");
            ui.colored_label(cpu_color, egui::RichText::new(format!("{:.0}%", m.cpu_usage)).strong());
            let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
            ui.label(egui::RichText::new(format!("({} cores)", m.cpu_per_core.len())).small().color(dim));
        });
        draw_sparkline(ui, &self.cpu_history, cpu_color, 0.0, 100.0);

        // Per-core bars
        if !m.cpu_per_core.is_empty() {
            let bar_h = 4.0;
            let total_w = ui.available_width();
            let bar_w = (total_w / m.cpu_per_core.len() as f32).max(2.0) - 1.0;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, bar_h), egui::Sense::hover());
            let painter = ui.painter();
            for (i, &usage) in m.cpu_per_core.iter().enumerate() {
                let x = rect.left() + i as f32 * (bar_w + 1.0);
                let bg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(bar_w, bar_h));
                let fill_h = bar_h * (usage / 100.0).min(1.0);
                let fill = egui::Rect::from_min_size(
                    egui::pos2(x, rect.bottom() - fill_h), egui::vec2(bar_w, fill_h));
                painter.rect_filled(bg, 0.0, ui.visuals().extreme_bg_color);
                let c = if usage < 50.0 { egui::Color32::from_rgb(60, 140, 200) }
                    else if usage < 80.0 { egui::Color32::from_rgb(200, 180, 60) }
                    else { egui::Color32::from_rgb(220, 60, 60) };
                painter.rect_filled(fill, 0.0, c);
            }
        }

        ui.separator();

        // RAM
        ui.horizontal(|ui| {
            ui.label("RAM:");
            ui.colored_label(egui::Color32::from_rgb(200, 120, 255),
                egui::RichText::new(format!("{:.1}/{:.0}GB ({:.0}%)", m.mem_used_gb, m.mem_total_gb, m.mem_percent)).strong());
        });
        draw_sparkline(ui, &self.mem_history, egui::Color32::from_rgb(200, 120, 255), 0.0, 100.0);

        ui.separator();

        // GPU
        if !m.gpu_name.is_empty() && m.gpu_name != "Unknown GPU" {
            ui.horizontal(|ui| {
                ui.label("GPU:");
                ui.label(egui::RichText::new(&m.gpu_name).small()
                    .color(egui::Color32::from_rgb(180, 220, 100)));
            });
        }

        ui.separator();

        // Process
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Mem: {:.0}MB  CPU: {:.1}%", m.process_mem_mb, m.process_cpu)).small());
        });
        draw_sparkline(ui, &self.process_mem_history, egui::Color32::from_rgb(255, 180, 80),
            0.0, (m.process_mem_mb * 2.0).max(100.0));

        ui.separator();

        // Graph stats
        let dim = ui.visuals().widgets.noninteractive.fg_stroke.color;
        ui.label(egui::RichText::new(format!("Nodes: {}  Wires: {}", self.node_count, self.connection_count))
            .small().color(dim));
    }
}

fn draw_sparkline(ui: &mut eframe::egui::Ui, data: &VecDeque<f32>, color: eframe::egui::Color32, min: f32, max: f32) {
    use eframe::egui;

    let h = 25.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);

    if data.len() < 2 { return; }

    let range = (max - min).max(0.001);
    let points: Vec<egui::Pos2> = data.iter().enumerate().map(|(i, &v)| {
        let x = rect.left() + (i as f32 / (data.len() - 1) as f32) * w;
        let y = rect.bottom() - ((v - min) / range).clamp(0.0, 1.0) * h;
        egui::pos2(x, y)
    }).collect();

    // Fill
    let mut fill_pts = points.clone();
    fill_pts.push(egui::pos2(rect.right(), rect.bottom()));
    fill_pts.push(egui::pos2(rect.left(), rect.bottom()));
    let fill_color = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30);
    painter.add(egui::Shape::convex_polygon(fill_pts, fill_color, egui::Stroke::NONE));

    // Line
    for window in points.windows(2) {
        painter.line_segment([window[0], window[1]], egui::Stroke::new(1.0, color));
    }
}

fn get_gpu_name() -> String {
    if let Ok(output) = std::process::Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-detailLevel", "mini"])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Chipset Model:") || trimmed.starts_with("Chip:") {
                return trimmed.split(':').nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
            }
        }
    }
    "Unknown GPU".to_string()
}

/// Register MonitorNode in the node registry for deserialization.
#[allow(dead_code)]
pub fn register(registry: &mut crate::node_trait::NodeRegistryInner) {
    registry.register("monitor", |_state| Box::new(MonitorNode::default()));
}
