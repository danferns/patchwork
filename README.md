# Patchwork

A node-based visual programming environment built with Rust and [egui](https://github.com/emilk/egui).

Connect nodes to build data pipelines — route numbers through math, load and edit files, preview shaders, send MIDI, and more. Everything is a window. Everything connects.

## Quick start

```bash
cargo run
```

Double-click the canvas to add nodes. Drag from output ports (blue) to input ports (gray) to connect them.

### Build distributable (macOS)

```bash
cargo install cargo-packager
cargo packager --release    # produces .app and .dmg
```

## Project structure

```
src/
├── main.rs              Entry point, window setup, icon
├── app.rs               Core loop: canvas, node rendering, connections, menus, theme, file drop
├── graph.rs             Data model: Graph, Node, Connection, PortValue, evaluation engine
└── nodes/
    ├── mod.rs           Node catalog + render dispatch
    ├── slider.rs        Float slider with configurable range
    ├── display.rs       Shows any input value (float or text)
    ├── math.rs          Add / Multiply
    ├── file.rs          Opens any file from disk, outputs content as text
    ├── text_editor.rs   Editable text area with input/output ports
    ├── wgsl_viewer.rs   Displays WGSL shader code with preview
    ├── mouse_tracker.rs Outputs live pointer X/Y
    ├── midi_output.rs   MIDI note/velocity/channel (placeholder)
    ├── theme.rs         Global dark/light mode, accent color, font size
    └── comment.rs       Freeform text note
```

## Nodes

| Node | In | Out | Description |
|------|:--:|:---:|-------------|
| **Slider** | — | `Value` | Draggable float with configurable min/max range |
| **Display** | `Value` | — | Renders float or text from upstream |
| **Add** | `A` `B` | `Result` | Outputs A + B |
| **Multiply** | `A` `B` | `Result` | Outputs A × B |
| **File** | — | `Content` | Loads any text file, outputs its content |
| **Text Editor** | `Text In` | `Text Out` | Editable area; read-only when input is connected |
| **WGSL Viewer** | `WGSL` | — | Shows shader code and a visual preview |
| **Mouse Tracker** | — | `X` `Y` | Live pointer coordinates |
| **MIDI Output** | `Note` `Vel` | — | Maps float inputs to MIDI parameters |
| **Theme** | — | — | Controls dark/light mode, accent color, font size |
| **Comment** | — | — | Sticky note for documentation |

### Data flow

Ports carry either **Float** or **Text** values. Connections are one-to-many from outputs, one-to-one on inputs (reconnecting replaces the previous link).

```
File ──→ Text Editor ──→ WGSL Viewer      (text pipeline)
Slider ──→ Add ──→ Multiply ──→ Display   (math pipeline)
Mouse Tracker ──→ Multiply ──→ MIDI Out   (control pipeline)
```

## Interactions

| Action | What it does |
|--------|-------------|
| **Double-click** canvas | Open the Add Node menu |
| **Drag** from a port | Create a connection (output → input) |
| **Close** a node (✕) | Delete the node and its connections |
| **Right-click** | Dismiss the add-node menu |
| **Drag & drop** a file onto the canvas | Creates a File node with that file loaded |
| **File → Save/Open** | Persist/restore the full graph as JSON |

## Examples

Open these via **File → Open Project**:

| File | What it shows |
|------|---------------|
| `examples/demo_math.json` | `(A + B) × C` with sliders feeding Add → Multiply → Display |
| `examples/demo_file_to_wgsl.json` | File → Text Editor → WGSL Viewer pipeline |
| `examples/demo_mouse_midi.json` | Mouse position → scale → MIDI output |

## Adding a new node

1. Create `src/nodes/my_node.rs` with a `pub fn render(ui, ...)` function
2. Add a variant to `NodeType` in `src/graph.rs` — implement `title()`, `inputs()`, `outputs()`, `color_hint()`
3. Add evaluation logic in `Graph::evaluate()` if the node produces output values
4. Register in `src/nodes/mod.rs`: add `pub mod my_node`, a catalog entry, and a match arm in `render_content()`

## Tech

- **[eframe](https://github.com/emilk/egui/tree/master/crates/eframe)** / **egui** — immediate-mode GUI
- **serde** — project serialization (JSON)
- **rfd** — native file dialogs
- **cargo-packager** — macOS `.app` / `.dmg` bundling

## License

MIT
