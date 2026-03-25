// MCP (Model Context Protocol) Server for Patchwork
// Enables AI assistants to programmatically create nodes, connect them, and build workflows.
// Runs as a background thread, communicates with GUI via mpsc channels.
// Protocol: JSON-RPC over stdin/stdout.

use crate::graph::*;
use crate::nodes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::mpsc;

// ── Commands & Results ───────────────────────────────────────────────────────

pub enum McpCommand {
    ListNodeTypes,
    CreateNode { type_name: String, position: [f32; 2], properties: Option<Value> },
    DeleteNode { node_id: NodeId },
    ListNodes,
    GetNode { node_id: NodeId },
    UpdateNode { node_id: NodeId, properties: Value },
    Connect { from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize },
    Disconnect { from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize },
    ListConnections,
    GetPortValues { node_id: Option<NodeId> },
    SaveProject { path: String },
    LoadProject { path: String },
    GetGraph,
    CreateWorkflow { nodes: Vec<WorkflowNode>, connections: Vec<WorkflowConn> },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum McpResult {
    Json(Value),
    Error { error: String },
}

pub struct McpRequest {
    pub command: McpCommand,
    pub response_tx: mpsc::Sender<McpResult>,
}

#[derive(Deserialize)]
pub struct WorkflowNode {
    #[serde(rename = "type")]
    pub node_type: String,
    pub position: Option<[f32; 2]>,
    pub properties: Option<Value>,
}

#[derive(Deserialize)]
pub struct WorkflowConn {
    pub from_index: usize,
    pub from_port: usize,
    pub to_index: usize,
    pub to_port: usize,
}

// ── Tool Schemas ─────────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_node_types",
            "description": "List all available node types with their categories, input ports, and output ports",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_node",
            "description": "Create a new node of the specified type at a canvas position",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "description": "Node type name (e.g., 'Slider', 'Synth', 'Add')" },
                    "position": { "type": "array", "items": { "type": "number" }, "description": "[x, y] canvas position, default [200, 200]" },
                    "properties": { "type": "object", "description": "Initial property values (e.g., {\"value\": 0.5} for Slider)" }
                },
                "required": ["type"]
            }
        },
        {
            "name": "delete_node",
            "description": "Delete a node by its ID (also removes all connections)",
            "inputSchema": {
                "type": "object",
                "properties": { "node_id": { "type": "integer", "description": "Node ID" } },
                "required": ["node_id"]
            }
        },
        {
            "name": "list_nodes",
            "description": "List all nodes in the current graph with their IDs, types, and positions",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_node",
            "description": "Get full state of a node by ID including all properties",
            "inputSchema": {
                "type": "object",
                "properties": { "node_id": { "type": "integer" } },
                "required": ["node_id"]
            }
        },
        {
            "name": "update_node",
            "description": "Update properties of an existing node (e.g., set slider value, change synth frequency)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "integer" },
                    "properties": { "type": "object", "description": "Properties to update" }
                },
                "required": ["node_id", "properties"]
            }
        },
        {
            "name": "connect",
            "description": "Connect an output port of one node to an input port of another",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from_node": { "type": "integer", "description": "Source node ID" },
                    "from_port": { "type": "integer", "description": "Source output port index (0-based)" },
                    "to_node": { "type": "integer", "description": "Target node ID" },
                    "to_port": { "type": "integer", "description": "Target input port index (0-based)" }
                },
                "required": ["from_node", "from_port", "to_node", "to_port"]
            }
        },
        {
            "name": "disconnect",
            "description": "Remove a connection between two ports",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from_node": { "type": "integer" },
                    "from_port": { "type": "integer" },
                    "to_node": { "type": "integer" },
                    "to_port": { "type": "integer" }
                },
                "required": ["from_node", "from_port", "to_node", "to_port"]
            }
        },
        {
            "name": "list_connections",
            "description": "List all connections in the graph",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_port_values",
            "description": "Get current evaluated output values. Optionally filter by node_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "integer", "description": "Optional: filter to a specific node" }
                }
            }
        },
        {
            "name": "save_project",
            "description": "Save the current graph to a project folder",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Folder path to save project" } },
                "required": ["path"]
            }
        },
        {
            "name": "load_project",
            "description": "Load a graph from a project folder",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Path to project.json file" } },
                "required": ["path"]
            }
        },
        {
            "name": "create_workflow",
            "description": "Create multiple nodes and connections in one atomic operation. Connections use 0-based indices into the nodes array.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string" },
                                "position": { "type": "array", "items": { "type": "number" } },
                                "properties": { "type": "object" }
                            },
                            "required": ["type"]
                        }
                    },
                    "connections": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "from_index": { "type": "integer", "description": "Index into nodes array" },
                                "from_port": { "type": "integer" },
                                "to_index": { "type": "integer", "description": "Index into nodes array" },
                                "to_port": { "type": "integer" }
                            },
                            "required": ["from_index", "from_port", "to_index", "to_port"]
                        }
                    }
                },
                "required": ["nodes"]
            }
        }
    ])
}

// ── Property Merge ───────────────────────────────────────────────────────────

/// Merge JSON properties into a NodeType using serde serialization round-trip
pub fn apply_properties(node_type: &mut NodeType, properties: Value) {
    if let Value::Object(props) = properties {
        if let Ok(mut current) = serde_json::to_value(&*node_type) {
            // NodeType serializes as {"Slider": {"value": 0.5, ...}}
            if let Value::Object(outer) = &mut current {
                if let Some((_, inner)) = outer.iter_mut().next() {
                    if let Value::Object(inner_map) = inner {
                        for (k, v) in props {
                            inner_map.insert(k, v);
                        }
                    }
                }
            }
            if let Ok(updated) = serde_json::from_value::<NodeType>(current) {
                *node_type = updated;
            }
        }
    }
}

// ── Command Execution (called by app.rs) ─────────────────────────────────────

pub fn execute_command(
    cmd: McpCommand,
    graph: &mut Graph,
    values: &HashMap<(NodeId, usize), PortValue>,
) -> McpResult {
    match cmd {
        McpCommand::ListNodeTypes => {
            let catalog = nodes::catalog();
            let types: Vec<Value> = catalog.iter().map(|e| {
                let nt = (e.factory)();
                json!({
                    "name": e.label,
                    "category": e.category,
                    "inputs": nt.inputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                    "outputs": nt.outputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                })
            }).collect();
            McpResult::Json(json!(types))
        }

        McpCommand::CreateNode { type_name, position, properties } => {
            let catalog = nodes::catalog();
            if let Some(entry) = catalog.iter().find(|e| e.label.eq_ignore_ascii_case(&type_name)) {
                let mut nt = (entry.factory)();
                if let Some(props) = properties {
                    apply_properties(&mut nt, props);
                }
                let id = graph.add_node(nt, position);
                McpResult::Json(json!({ "node_id": id }))
            } else {
                McpResult::Error { error: format!("Unknown node type: '{}'. Use list_node_types to see available types.", type_name) }
            }
        }

        McpCommand::DeleteNode { node_id } => {
            if graph.nodes.contains_key(&node_id) {
                graph.remove_node(node_id);
                McpResult::Json(json!({ "success": true }))
            } else {
                McpResult::Error { error: format!("Node {} not found", node_id) }
            }
        }

        McpCommand::ListNodes => {
            let nodes: Vec<Value> = graph.nodes.iter().map(|(&id, node)| {
                json!({
                    "id": id,
                    "type": node.node_type.title(),
                    "position": node.pos,
                    "inputs": node.node_type.inputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                    "outputs": node.node_type.outputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                })
            }).collect();
            McpResult::Json(json!(nodes))
        }

        McpCommand::GetNode { node_id } => {
            if let Some(node) = graph.nodes.get(&node_id) {
                let node_json = serde_json::to_value(&node.node_type).unwrap_or(json!(null));
                McpResult::Json(json!({
                    "id": node_id,
                    "type": node.node_type.title(),
                    "position": node.pos,
                    "state": node_json,
                    "inputs": node.node_type.inputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                    "outputs": node.node_type.outputs().iter().map(|p| p.name).collect::<Vec<_>>(),
                }))
            } else {
                McpResult::Error { error: format!("Node {} not found", node_id) }
            }
        }

        McpCommand::UpdateNode { node_id, properties } => {
            if let Some(node) = graph.nodes.get_mut(&node_id) {
                apply_properties(&mut node.node_type, properties);
                McpResult::Json(json!({ "success": true }))
            } else {
                McpResult::Error { error: format!("Node {} not found", node_id) }
            }
        }

        McpCommand::Connect { from_node, from_port, to_node, to_port } => {
            if !graph.nodes.contains_key(&from_node) {
                return McpResult::Error { error: format!("Source node {} not found", from_node) };
            }
            if !graph.nodes.contains_key(&to_node) {
                return McpResult::Error { error: format!("Target node {} not found", to_node) };
            }
            graph.add_connection(from_node, from_port, to_node, to_port);
            McpResult::Json(json!({ "success": true }))
        }

        McpCommand::Disconnect { from_node, from_port, to_node, to_port } => {
            let before = graph.connections.len();
            graph.connections.retain(|c| {
                !(c.from_node == from_node && c.from_port == from_port &&
                  c.to_node == to_node && c.to_port == to_port)
            });
            let removed = before - graph.connections.len();
            McpResult::Json(json!({ "success": true, "removed": removed }))
        }

        McpCommand::ListConnections => {
            let conns: Vec<Value> = graph.connections.iter().map(|c| {
                let from_name = graph.nodes.get(&c.from_node)
                    .map(|n| n.node_type.outputs().get(c.from_port).map(|p| p.name).unwrap_or("?"))
                    .unwrap_or("?");
                let to_name = graph.nodes.get(&c.to_node)
                    .map(|n| n.node_type.inputs().get(c.to_port).map(|p| p.name).unwrap_or("?"))
                    .unwrap_or("?");
                json!({
                    "from_node": c.from_node, "from_port": c.from_port, "from_port_name": from_name,
                    "to_node": c.to_node, "to_port": c.to_port, "to_port_name": to_name,
                })
            }).collect();
            McpResult::Json(json!(conns))
        }

        McpCommand::GetPortValues { node_id } => {
            let mut result: HashMap<String, Value> = HashMap::new();
            for (&(nid, port), val) in values {
                if node_id.is_none() || node_id == Some(nid) {
                    let key = format!("{}:{}", nid, port);
                    let v = match val {
                        PortValue::Float(f) => json!(f),
                        PortValue::Text(s) => json!(s),
                        PortValue::None => json!(null),
                    };
                    result.insert(key, v);
                }
            }
            McpResult::Json(json!(result))
        }

        McpCommand::SaveProject { path } => {
            let dir = std::path::Path::new(&path);
            if let Err(e) = std::fs::create_dir_all(dir) {
                return McpResult::Error { error: format!("mkdir: {}", e) };
            }
            let json = serde_json::to_string_pretty(graph).unwrap_or_default();
            match std::fs::write(dir.join("project.json"), json) {
                Ok(()) => McpResult::Json(json!({ "success": true, "path": path })),
                Err(e) => McpResult::Error { error: format!("write: {}", e) },
            }
        }

        McpCommand::LoadProject { path } => {
            let p = std::path::Path::new(&path);
            let json_path = if p.is_file() { p.to_path_buf() } else { p.join("project.json") };
            match std::fs::read_to_string(&json_path) {
                Ok(content) => match serde_json::from_str::<Graph>(&content) {
                    Ok(loaded) => {
                        *graph = loaded;
                        McpResult::Json(json!({ "success": true, "nodes": graph.nodes.len() }))
                    }
                    Err(e) => McpResult::Error { error: format!("parse: {}", e) },
                },
                Err(e) => McpResult::Error { error: format!("read: {}", e) },
            }
        }

        McpCommand::GetGraph => {
            let json = serde_json::to_value(graph).unwrap_or(json!(null));
            McpResult::Json(json)
        }

        McpCommand::CreateWorkflow { nodes: wf_nodes, connections: wf_conns } => {
            let catalog = nodes::catalog();
            let mut created_ids: Vec<NodeId> = Vec::new();

            for (i, wf_node) in wf_nodes.iter().enumerate() {
                let entry = catalog.iter().find(|e| e.label.eq_ignore_ascii_case(&wf_node.node_type));
                if let Some(entry) = entry {
                    let mut nt = (entry.factory)();
                    if let Some(ref props) = wf_node.properties {
                        apply_properties(&mut nt, props.clone());
                    }
                    let pos = wf_node.position.unwrap_or([200.0 + i as f32 * 250.0, 200.0]);
                    let id = graph.add_node(nt, pos);
                    created_ids.push(id);
                } else {
                    return McpResult::Error {
                        error: format!("Unknown node type at index {}: '{}'", i, wf_node.node_type),
                    };
                }
            }

            // Create connections using the resolved IDs
            for wf_conn in &wf_conns {
                if wf_conn.from_index >= created_ids.len() || wf_conn.to_index >= created_ids.len() {
                    continue; // Skip invalid indices
                }
                let from_id = created_ids[wf_conn.from_index];
                let to_id = created_ids[wf_conn.to_index];
                graph.add_connection(from_id, wf_conn.from_port, to_id, wf_conn.to_port);
            }

            McpResult::Json(json!({ "node_ids": created_ids }))
        }
    }
}

// ── MCP Thread (JSON-RPC over stdio) ─────────────────────────────────────────

pub fn run_mcp_thread(command_tx: mpsc::Sender<McpRequest>) {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let reader = std::io::BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                write_jsonrpc_error(&mut stdout, Value::Null, -32700, "Parse error");
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        match method {
            "initialize" => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "patchwork", "version": "0.0.1" }
                    }
                });
                write_json(&mut stdout, &response);
            }

            "notifications/initialized" => {
                // Client acknowledged initialization — no response needed
            }

            "tools/list" => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": tool_definitions() }
                });
                write_json(&mut stdout, &response);
            }

            "tools/call" => {
                let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                let cmd = match parse_tool_call(tool_name, &arguments) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        let response = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                                "isError": true
                            }
                        });
                        write_json(&mut stdout, &response);
                        continue;
                    }
                };

                // Send command to GUI thread and wait for result
                let (resp_tx, resp_rx) = mpsc::channel();
                let req = McpRequest { command: cmd, response_tx: resp_tx };
                if command_tx.send(req).is_err() {
                    write_jsonrpc_error(&mut stdout, id, -32603, "App disconnected");
                    break;
                }

                // Wait for GUI to process (blocks until next frame)
                match resp_rx.recv() {
                    Ok(result) => {
                        let text = match &result {
                            McpResult::Json(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
                            McpResult::Error { error } => format!("Error: {}", error),
                        };
                        let is_error = matches!(result, McpResult::Error { .. });
                        let response = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": text }],
                                "isError": is_error
                            }
                        });
                        write_json(&mut stdout, &response);
                    }
                    Err(_) => {
                        write_jsonrpc_error(&mut stdout, id, -32603, "Response timeout");
                    }
                }
            }

            _ => {
                write_jsonrpc_error(&mut stdout, id, -32601, &format!("Unknown method: {}", method));
            }
        }
    }
}

fn parse_tool_call(name: &str, args: &Value) -> Result<McpCommand, String> {
    match name {
        "list_node_types" => Ok(McpCommand::ListNodeTypes),

        "create_node" => {
            let type_name = args.get("type").and_then(|v| v.as_str())
                .ok_or("Missing 'type' parameter")?.to_string();
            let position = args.get("position")
                .and_then(|v| v.as_array())
                .map(|a| [
                    a.first().and_then(|v| v.as_f64()).unwrap_or(200.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(200.0) as f32,
                ])
                .unwrap_or([200.0, 200.0]);
            let properties = args.get("properties").cloned();
            Ok(McpCommand::CreateNode { type_name, position, properties })
        }

        "delete_node" => {
            let node_id = args.get("node_id").and_then(|v| v.as_u64())
                .ok_or("Missing 'node_id'")?;
            Ok(McpCommand::DeleteNode { node_id })
        }

        "list_nodes" => Ok(McpCommand::ListNodes),

        "get_node" => {
            let node_id = args.get("node_id").and_then(|v| v.as_u64())
                .ok_or("Missing 'node_id'")?;
            Ok(McpCommand::GetNode { node_id })
        }

        "update_node" => {
            let node_id = args.get("node_id").and_then(|v| v.as_u64())
                .ok_or("Missing 'node_id'")?;
            let properties = args.get("properties").cloned()
                .ok_or("Missing 'properties'")?;
            Ok(McpCommand::UpdateNode { node_id, properties })
        }

        "connect" => {
            let from_node = args.get("from_node").and_then(|v| v.as_u64()).ok_or("Missing 'from_node'")?;
            let from_port = args.get("from_port").and_then(|v| v.as_u64()).ok_or("Missing 'from_port'")? as usize;
            let to_node = args.get("to_node").and_then(|v| v.as_u64()).ok_or("Missing 'to_node'")?;
            let to_port = args.get("to_port").and_then(|v| v.as_u64()).ok_or("Missing 'to_port'")? as usize;
            Ok(McpCommand::Connect { from_node, from_port, to_node, to_port })
        }

        "disconnect" => {
            let from_node = args.get("from_node").and_then(|v| v.as_u64()).ok_or("Missing 'from_node'")?;
            let from_port = args.get("from_port").and_then(|v| v.as_u64()).ok_or("Missing 'from_port'")? as usize;
            let to_node = args.get("to_node").and_then(|v| v.as_u64()).ok_or("Missing 'to_node'")?;
            let to_port = args.get("to_port").and_then(|v| v.as_u64()).ok_or("Missing 'to_port'")? as usize;
            Ok(McpCommand::Disconnect { from_node, from_port, to_node, to_port })
        }

        "list_connections" => Ok(McpCommand::ListConnections),

        "get_port_values" => {
            let node_id = args.get("node_id").and_then(|v| v.as_u64());
            Ok(McpCommand::GetPortValues { node_id })
        }

        "save_project" => {
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?.to_string();
            Ok(McpCommand::SaveProject { path })
        }

        "load_project" => {
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?.to_string();
            Ok(McpCommand::LoadProject { path })
        }

        "create_workflow" => {
            let nodes: Vec<WorkflowNode> = args.get("nodes")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or("Missing or invalid 'nodes' array")?;
            let connections: Vec<WorkflowConn> = args.get("connections")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            Ok(McpCommand::CreateWorkflow { nodes, connections })
        }

        _ => Err(format!("Unknown tool: {}", name)),
    }
}

// ── JSON-RPC Helpers ─────────────────────────────────────────────────────────

fn write_json(stdout: &mut std::io::Stdout, value: &Value) {
    let s = serde_json::to_string(value).unwrap_or_default();
    let _ = writeln!(stdout, "{}", s);
    let _ = stdout.flush();
}

fn write_jsonrpc_error(stdout: &mut std::io::Stdout, id: Value, code: i32, message: &str) {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    });
    write_json(stdout, &response);
}
