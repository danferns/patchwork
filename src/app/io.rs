use super::*;

impl super::PatchworkApp {
    pub(super) fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() { self.push_undo(); }
        let image_exts = ["png", "jpg", "jpeg", "gif", "bmp", "webp"];
        let video_exts = ["mp4", "mov", "avi", "webm", "mkv"];
        for path in dropped {
            let pos = ctx.pointer_latest_pos().unwrap_or(egui::pos2(200.0, 200.0));
            let off_e = self.canvas_offset / self.canvas_zoom;
            let canvas_x = pos.x - off_e.x;
            let canvas_y = pos.y - off_e.y;

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if image_exts.contains(&ext.as_str()) {
                let image_data = crate::nodes::image_node::load_image_from_path(&path.display().to_string());
                self.graph.add_node(NodeType::ImageNode {
                    path: path.display().to_string(),
                    save_path: String::new(),
                    image_data,
                    preview_size: 150.0,
                    last_save_hash: 0,
                }, [canvas_x, canvas_y]);
            } else if video_exts.contains(&ext.as_str()) {
                self.graph.add_node(NodeType::VideoPlayer {
                    path: path.display().to_string(),
                    playing: false, looping: false,
                    res_w: 640, res_h: 480,
                    current_frame: None,
                    duration: 0.0, speed: 1.0,
                    status: "Loaded".into(),
                }, [canvas_x, canvas_y]);
            } else {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                self.graph.add_node(NodeType::File { path: path.display().to_string(), content }, [canvas_x, canvas_y]);
            }
        }
    }

    pub(super) fn poll_midi_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            if let Some(msg) = self.midi.poll_input(nid) {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::MidiIn { channel, note, velocity, log, .. } = &mut node.node_type {
                        if msg.len() >= 3 {
                            *channel = msg[0] & 0x0F;
                            let status = msg[0] & 0xF0;
                            match status {
                                0x80 | 0x90 | 0xA0 | 0xB0 => { *note = msg[1]; *velocity = msg[2]; }
                                _ => {}
                            }
                        }
                        log.push(nodes::midi_in::format_midi_message(&msg));
                    }
                }
            }
        }
    }

    pub(super) fn poll_serial_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let lines = self.serial.poll(nid);
            if !lines.is_empty() {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::Serial { log, last_line, .. } = &mut node.node_type {
                        for line in lines {
                            *last_line = line.clone();
                            log.push(line);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn poll_osc_inputs(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let messages = self.osc.poll(nid);
            if !messages.is_empty() {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    if let NodeType::OscIn { address_filter, arg_count, last_args, log, .. } = &mut node.node_type {
                        for (addr, args) in messages {
                            if !address_filter.is_empty() && !addr.contains(address_filter.as_str()) {
                                continue;
                            }
                            // Update last_args from received message
                            for (i, &val) in args.iter().enumerate() {
                                if i < *arg_count {
                                    if i >= last_args.len() { last_args.push(0.0); }
                                    last_args[i] = val;
                                }
                            }
                            let args_str = args.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>().join(", ");
                            log.push(format!("{} [{}]", addr, args_str));
                            if log.len() > 200 { log.remove(0); }
                        }
                    }
                }
            }
        }
    }

    pub(super) fn poll_http_responses(&mut self) {
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            if let Some(resp) = self.http.poll(nid) {
                if let Some(node) = self.graph.nodes.get_mut(&nid) {
                    match &mut node.node_type {
                        NodeType::HttpRequest { response, status, .. } => {
                            *status = format!("{}", resp.status);
                            *response = resp.body;
                        }
                        NodeType::AiRequest { provider, response, status, .. } => {
                            if resp.status >= 200 && resp.status < 300 {
                                // Try to detect provider from response shape if not set
                                let prov = if provider.is_empty() {
                                    // Auto-detect: Anthropic has "content" array, OpenAI has "choices"
                                    if resp.body.contains("\"content\":[{\"type\"") { "anthropic" }
                                    else { "openai" }
                                } else {
                                    provider.as_str()
                                };
                                *response = crate::nodes::ai_request::extract_ai_response(prov, &resp.body);
                                *status = "done".into();
                            } else {
                                *response = resp.body;
                                *status = format!("error: {}", resp.status);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Poll ML inference results and dispatch new requests
    pub(super) fn poll_ml_inference(&mut self, ctx: &egui::Context) {
        // Receive completed results
        while let Ok(result) = self.ml_rx.try_recv() {
            if let Some(node) = self.graph.nodes.get_mut(&result.node_id) {
                if let NodeType::MlModel { result_text, status, .. } = &mut node.node_type {
                    *result_text = result.result_text;
                    *status = result.status;
                }
            }
        }

        // Check for new inference requests (stored in egui temp data by ml_model::render)
        let node_ids: Vec<NodeId> = self.graph.nodes.keys().copied().collect();
        for nid in node_ids {
            let inference_id = egui::Id::new(("ml_inference", nid));
            if let Some(req) = ctx.data_mut(|d| d.get_temp::<crate::nodes::ml_model::MlInferenceRequest>(inference_id)) {
                ctx.data_mut(|d| d.remove::<crate::nodes::ml_model::MlInferenceRequest>(inference_id));
                let tx = self.ml_tx.clone();
                std::thread::spawn(move || {
                    let result = crate::nodes::ml_model::run_inference(&req);
                    let _ = tx.send(result);
                });
            }
        }
    }

    /// Process pending MCP commands from the MCP server thread
    pub(super) fn process_mcp_commands(&mut self, values: &HashMap<(NodeId, usize), PortValue>) {
        let rx = match &self.mcp_rx {
            Some(rx) => rx,
            None => return,
        };
        // Drain all pending requests (non-blocking)
        while let Ok(request) = rx.try_recv() {
            let result = crate::mcp::execute_command(request.command, &mut self.graph, values);
            let _ = request.response_tx.send(result);
        }
    }

    pub(super) fn save_project(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Save Project Folder").pick_folder() {
            let project_file = dir.join("project.json");
            let json = serde_json::to_string_pretty(&self.graph).unwrap_or_default();
            let _ = std::fs::write(&project_file, json);
            // Also save api_keys if any exist
            if !self.api_keys.is_empty() {
                let keys_file = dir.join("api_keys.json");
                let keys_json = serde_json::to_string_pretty(&self.api_keys).unwrap_or_default();
                let _ = std::fs::write(&keys_file, keys_json);
            }
            self.project_path = Some(dir.display().to_string());
        }
    }

    pub(super) fn load_project(&mut self) {
        // Try picking a folder first, then fall back to file
        if let Some(path) = rfd::FileDialog::new().add_filter("Patchwork", &["json"]).pick_file() {
            let dir = if path.file_name().map(|f| f == "project.json").unwrap_or(false) {
                path.parent().map(|p| p.to_path_buf())
            } else {
                None
            };
            // Load graph from the file
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(graph) = serde_json::from_str::<Graph>(&json) {
                    self.graph = graph;
                    self.port_positions.clear();
                    self.node_rects.clear();
                    self.undo_history.clear();
                }
            }
            // Load api_keys from the same folder
            if let Some(dir) = &dir {
                let keys_file = dir.join("api_keys.json");
                if let Ok(json) = std::fs::read_to_string(&keys_file) {
                    if let Ok(keys) = serde_json::from_str::<HashMap<String, String>>(&json) {
                        self.api_keys = keys;
                    }
                }
                self.project_path = Some(dir.display().to_string());
            } else {
                self.project_path = Some(path.display().to_string());
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn project_dir(&self) -> Option<std::path::PathBuf> {
        self.project_path.as_ref().map(|p| std::path::PathBuf::from(p))
    }

    #[allow(dead_code)]
    pub(super) fn load_api_keys(&mut self) {
        if let Some(dir) = self.project_dir() {
            let keys_file = dir.join("api_keys.json");
            if let Ok(json) = std::fs::read_to_string(&keys_file) {
                if let Ok(keys) = serde_json::from_str::<HashMap<String, String>>(&json) {
                    self.api_keys = keys;
                }
            }
        }
    }

    pub(super) fn sync_console_messages(&mut self) {
        for node in self.graph.nodes.values_mut() {
            if let NodeType::Console { messages } = &mut node.node_type {
                *messages = self.console_messages.clone();
            }
        }
    }

    pub(super) fn apply_theme(&self, ctx: &egui::Context) {
        for node in self.graph.nodes.values() {
            if let NodeType::Theme { dark_mode, accent, font_size, bg_color, text_color, window_bg, window_alpha, grid_color: _, rounding, spacing, .. } = &node.node_type {
                nodes::theme::apply(ctx, *dark_mode, *accent, *font_size, *bg_color, *text_color, *window_bg, *window_alpha, *rounding, *spacing);
                return;
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn log_message(&mut self, msg: String) {
        self.console_messages.push(msg);
        if self.console_messages.len() > 200 {
            self.console_messages.remove(0);
        }
    }

    pub(super) fn update_mouse_trackers(&mut self, ctx: &egui::Context) {
        if let Some(pos) = ctx.pointer_latest_pos() {
            for node in self.graph.nodes.values_mut() {
                if let NodeType::MouseTracker { x, y } = &mut node.node_type { *x = pos.x; *y = pos.y; }
            }
        }
    }

    pub(super) fn update_key_inputs(&mut self, ctx: &egui::Context) {
        // Don't capture keys when a text field is focused
        if ctx.wants_keyboard_input() { return; }

        for node in self.graph.nodes.values_mut() {
            if let NodeType::KeyInput { key_name, pressed, toggle_mode, toggled_on } = &mut node.node_type {
                if let Some(key) = nodes::key_input::parse_key(key_name) {
                    let is_down = ctx.input(|i| i.key_down(key));
                    let just_pressed = ctx.input(|i| i.key_pressed(key));

                    if *toggle_mode {
                        if just_pressed {
                            *toggled_on = !*toggled_on;
                        }
                        *pressed = just_pressed;
                    } else {
                        *pressed = is_down;
                    }
                } else {
                    *pressed = false;
                }
            }
        }
    }
}
