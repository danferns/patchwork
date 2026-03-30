#![allow(dead_code)]
use eframe::egui;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// System metrics collected by background thread
#[derive(Clone, Default)]
pub struct SystemMetrics {
    pub cpu_usage: f32,          // Overall CPU % (0-100)
    pub cpu_per_core: Vec<f32>,  // Per-core CPU %
    pub mem_used_gb: f32,        // RAM used (GB)
    pub mem_total_gb: f32,       // RAM total (GB)
    pub mem_percent: f32,        // RAM usage %
    pub gpu_name: String,        // GPU device name
    pub process_mem_mb: f32,     // Patchwork process memory (MB)
    pub process_cpu: f32,        // Patchwork process CPU %
}

pub type SharedMetrics = Arc<Mutex<SystemMetrics>>;

/// Persistent state for the profiler node
pub struct ProfilerState {
    pub metrics: SharedMetrics,
    pub fps_history: VecDeque<f32>,
    pub cpu_history: VecDeque<f32>,
    pub mem_history: VecDeque<f32>,
    pub process_mem_history: VecDeque<f32>,
    pub last_frame: Instant,
    pub frame_times: VecDeque<f32>,
    max_history: usize,
    pub node_count: usize,
    pub connection_count: usize,
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl ProfilerState {
    pub fn new() -> Self {
        let metrics: SharedMetrics = Arc::new(Mutex::new(SystemMetrics::default()));
        let metrics_clone = metrics.clone();

        // Background thread collects system metrics every 1 second
        let thread = std::thread::spawn(move || {
            use sysinfo::{System, Pid};

            let mut sys = System::new_all();
            let pid = Pid::from_u32(std::process::id());

            // Get GPU name once
            {
                // Try to read GPU info (macOS: system_profiler, Linux: lspci)
                let gpu_name = get_gpu_name();
                if let Ok(mut m) = metrics_clone.lock() {
                    m.gpu_name = gpu_name;
                }
            }

            loop {
                sys.refresh_all();

                let cpu_usage = sys.global_cpu_usage();
                let cpu_per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
                let mem_used = sys.used_memory() as f64 / 1_073_741_824.0; // bytes to GB
                let mem_total = sys.total_memory() as f64 / 1_073_741_824.0;
                let mem_percent = if mem_total > 0.0 { (mem_used / mem_total * 100.0) as f32 } else { 0.0 };

                // Process-specific metrics
                let (proc_mem, proc_cpu) = if let Some(process) = sys.process(pid) {
                    let mem_mb = process.memory() as f64 / 1_048_576.0; // bytes to MB
                    (mem_mb as f32, process.cpu_usage())
                } else {
                    (0.0, 0.0)
                };

                if let Ok(mut m) = metrics_clone.lock() {
                    m.cpu_usage = cpu_usage;
                    m.cpu_per_core = cpu_per_core;
                    m.mem_used_gb = mem_used as f32;
                    m.mem_total_gb = mem_total as f32;
                    m.mem_percent = mem_percent;
                    m.process_mem_mb = proc_mem;
                    m.process_cpu = proc_cpu;
                }

                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        Self {
            metrics,
            fps_history: VecDeque::with_capacity(120),
            cpu_history: VecDeque::with_capacity(120),
            mem_history: VecDeque::with_capacity(120),
            process_mem_history: VecDeque::with_capacity(120),
            last_frame: Instant::now(),
            frame_times: VecDeque::with_capacity(120),
            max_history: 120,
            node_count: 0,
            connection_count: 0,
            _thread: Some(thread),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if dt > 0.0 {
            let fps = 1.0 / dt;
            self.fps_history.push_back(fps);
            self.frame_times.push_back(dt * 1000.0);
            if self.fps_history.len() > self.max_history { self.fps_history.pop_front(); }
            if self.frame_times.len() > self.max_history { self.frame_times.pop_front(); }
        }

        if let Ok(m) = self.metrics.lock() {
            self.cpu_history.push_back(m.cpu_usage);
            self.mem_history.push_back(m.mem_percent);
            self.process_mem_history.push_back(m.process_mem_mb);
            if self.cpu_history.len() > self.max_history { self.cpu_history.pop_front(); }
            if self.mem_history.len() > self.max_history { self.mem_history.pop_front(); }
            if self.process_mem_history.len() > self.max_history { self.process_mem_history.pop_front(); }
        }
    }
}

fn get_gpu_name() -> String {
    // Try macOS system_profiler
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

pub fn render(ui: &mut egui::Ui, state: &ProfilerState) {
    let metrics = state.metrics.lock().ok();
    let m = metrics.as_deref().cloned().unwrap_or_default();

    let fps = state.fps_history.back().copied().unwrap_or(0.0);
    let frame_ms = state.frame_times.back().copied().unwrap_or(0.0);

    // FPS & Frame Time
    ui.horizontal(|ui| {
        ui.label("FPS:");
        let fps_color = if fps >= 55.0 { egui::Color32::from_rgb(80, 200, 80) }
            else if fps >= 30.0 { egui::Color32::from_rgb(200, 200, 80) }
            else { egui::Color32::from_rgb(255, 80, 80) };
        ui.colored_label(fps_color, egui::RichText::new(format!("{:.0}", fps)).strong());
        ui.label(egui::RichText::new(format!("{:.1}ms", frame_ms)).small().color(egui::Color32::GRAY));
    });
    draw_sparkline(ui, &state.fps_history, egui::Color32::from_rgb(80, 200, 80), 0.0, 120.0);

    ui.separator();

    // CPU
    ui.horizontal(|ui| {
        ui.label("CPU:");
        let cpu_color = if m.cpu_usage < 50.0 { egui::Color32::from_rgb(80, 180, 255) }
            else if m.cpu_usage < 80.0 { egui::Color32::from_rgb(200, 200, 80) }
            else { egui::Color32::from_rgb(255, 80, 80) };
        ui.colored_label(cpu_color, egui::RichText::new(format!("{:.0}%", m.cpu_usage)).strong());
        ui.label(egui::RichText::new(format!("({} cores)", m.cpu_per_core.len())).small().color(egui::Color32::GRAY));
    });
    draw_sparkline(ui, &state.cpu_history, egui::Color32::from_rgb(80, 180, 255), 0.0, 100.0);

    // CPU per-core mini bars
    if !m.cpu_per_core.is_empty() {
        let bar_h = 4.0;
        let total_w = ui.available_width();
        let bar_w = (total_w / m.cpu_per_core.len() as f32).max(2.0) - 1.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(total_w, bar_h),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        for (i, &usage) in m.cpu_per_core.iter().enumerate() {
            let x = rect.left() + i as f32 * (bar_w + 1.0);
            let bg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(bar_w, bar_h));
            let fill_h = bar_h * (usage / 100.0).min(1.0);
            let fill = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - fill_h),
                egui::vec2(bar_w, fill_h),
            );
            painter.rect_filled(bg, 0.0, egui::Color32::from_rgb(30, 30, 40));
            let c = if usage < 50.0 { egui::Color32::from_rgb(60, 140, 200) }
                else if usage < 80.0 { egui::Color32::from_rgb(200, 180, 60) }
                else { egui::Color32::from_rgb(220, 60, 60) };
            painter.rect_filled(fill, 0.0, c);
        }
    }

    ui.separator();

    // Memory
    ui.horizontal(|ui| {
        ui.label("RAM:");
        ui.colored_label(
            egui::Color32::from_rgb(200, 120, 255),
            egui::RichText::new(format!("{:.1}/{:.0} GB ({:.0}%)", m.mem_used_gb, m.mem_total_gb, m.mem_percent)).strong(),
        );
    });
    draw_sparkline(ui, &state.mem_history, egui::Color32::from_rgb(200, 120, 255), 0.0, 100.0);

    ui.separator();

    // GPU
    if !m.gpu_name.is_empty() && m.gpu_name != "Unknown GPU" {
        ui.horizontal(|ui| {
            ui.label("GPU:");
            ui.label(egui::RichText::new(&m.gpu_name).small().color(egui::Color32::from_rgb(180, 220, 100)));
        });
    }

    ui.separator();

    // Patchwork Process
    ui.label(egui::RichText::new("Patchwork Process").strong().small());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Mem: {:.0} MB", m.process_mem_mb)).small());
        ui.label(egui::RichText::new(format!("CPU: {:.1}%", m.process_cpu)).small());
    });
    draw_sparkline(ui, &state.process_mem_history, egui::Color32::from_rgb(255, 180, 80), 0.0, (m.process_mem_mb * 2.0).max(100.0));

    ui.separator();

    // Graph stats
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Nodes: {}  Wires: {}", state.node_count, state.connection_count))
            .small().color(egui::Color32::GRAY));
    });
}

impl ProfilerState {
    pub fn set_graph_stats(&mut self, nodes: usize, connections: usize) {
        self.node_count = nodes;
        self.connection_count = connections;
    }
}

fn draw_sparkline(ui: &mut egui::Ui, data: &VecDeque<f32>, color: egui::Color32, min: f32, max: f32) {
    let h = 25.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 20));

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
