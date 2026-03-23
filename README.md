# Patchwork

A node-based visual programming environment built with Rust and [egui](https://github.com/emilk/egui).

Connect nodes to build data pipelines — route numbers through math, load and edit files, preview shaders, send MIDI/OSC, communicate over serial, run custom scripts, and more. Everything is a window. Everything connects.

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
├── app.rs               Core loop: canvas, node rendering, connections, menus, clipboard, shortcuts
├── graph.rs             Data model: Graph, Node, Connection, PortValue, evaluation engine
├── midi.rs              MIDI device manager (input/output via midir)
├── serial.rs            Serial port manager (background reader threads)
├── osc.rs               OSC manager (UDP send/receive via rosc)
└── nodes/
    ├── mod.rs           Node catalog + render dispatch
    ├── slider.rs        Float slider with configurable range and input port
    ├── display.rs       Shows any input value (float or text)
    ├── math.rs          Add / Multiply
    ├── file.rs          Opens any file from disk, outputs content as text
    ├── text_editor.rs   Editable text area with input/output ports
    ├── wgsl_viewer.rs   Displays WGSL shader code with preview
    ├── mouse_tracker.rs Outputs live pointer X/Y
    ├── midi_out.rs      Send MIDI Note or CC messages to a device
    ├── midi_in.rs       Receive MIDI messages with live log
    ├── serial.rs        Read/write serial ports with baud rate selection
    ├── osc_out.rs       Send OSC messages over UDP
    ├── osc_in.rs        Receive OSC messages over UDP
    ├── script.rs        Custom Rhai scripting with user-defined I/O
    ├── theme.rs         Global dark/light mode, accent color, font size
    ├── monitor.rs       Live FPS, frame time, node/connection count with sparklines
    ├── console.rs       System message log with color-coded output
    └── comment.rs       Freeform text note
```

## Nodes

| Node | Category | In | Out | Description |
|------|----------|:--:|:---:|-------------|
| **Slider** | Input | `In` | `Value` | Draggable float with min/max range; input overrides manual value |
| **Mouse Tracker** | Input | — | `X` `Y` | Live pointer coordinates |
| **Add** | Math | `A` `B` | `Result` | Outputs A + B |
| **Multiply** | Math | `A` `B` | `Result` | Outputs A x B |
| **File** | IO | — | `Content` | Loads any text file, outputs its content |
| **Text Editor** | IO | `Text In` | `Text Out` | Editable area; read-only when input connected |
| **Display** | Output | `Value` | — | Renders float or text from upstream |
| **WGSL Viewer** | Shader | `WGSL` | — | Shows shader code with visual preview |
| **MIDI Out** | MIDI | `Channel` `Note/CC#` `Velocity/Value` | — | Send Note or CC messages; device selector, change detection |
| **MIDI In** | MIDI | — | `Channel` `Note` `Velocity` | Receive MIDI with scrolling message log |
| **Serial** | Serial | `Send` | `Received` | Read/write serial ports; baud rate selector, live log |
| **OSC Out** | OSC | `Arg 0..N` | — | Send OSC float messages over UDP; configurable host/port/address |
| **OSC In** | OSC | — | `Arg 0..N` | Receive OSC messages; address filter, listen toggle, scrolling log |
| **Script** | Custom | user-defined | user-defined | Rhai scripting engine; +/- buttons for I/O ports; continuous or manual execution |
| **Theme** | Utility | — | — | Controls dark/light mode, accent color, font size |
| **Monitor** | Utility | — | `FPS` `Frame ms` `Nodes` | Live performance data with sparkline graphs |
| **Console** | Utility | — | — | System message log with color-coded output |
| **Comment** | Utility | — | — | Sticky note for documentation |

### Data flow

Ports carry either **Float** or **Text** values. Connections are one-to-many from outputs, one-to-one on inputs (reconnecting replaces the previous link). The graph evaluates in 5 propagation passes per frame.

```
File ──> Text Editor ──> WGSL Viewer         (text pipeline)
Slider ──> Add ──> Multiply ──> Display      (math pipeline)
Mouse Tracker ──> Script ──> MIDI Out        (control pipeline)
Slider ──> OSC Out ──> [network] ──> OSC In  (OSC pipeline)
```

### Script node

Write custom logic in [Rhai](https://rhai.rs/). Input and output names become variables automatically:

```rhai
// Inputs: a, b    Outputs: sum, diff
sum = a + b;
diff = a - b;
```

Toggle **Continuous** mode for live evaluation, or use manual **Run** button / **Exec** input port for triggered execution.

## Interactions

| Action | What it does |
|--------|-------------|
| **Double-click** canvas | Open the Add Node menu (with search) |
| **Drag** from a port | Create a connection (output to input) |
| **Click** a node | Select it (blue highlight) |
| **Right-click** a node | Context menu: Copy, Paste, Duplicate, Delete |
| **Cmd+C / Cmd+V** | Copy / paste selected node |
| **Cmd+D** | Duplicate selected node |
| **Option+Drag** a node | Duplicate and drag the copy |
| **Delete / Backspace** | Delete selected node (when no text field focused) |
| **Close** a node (x) | Delete the node and its connections |
| **Drag & drop** a file onto the canvas | Creates a File node with that file loaded |
| **Escape** | Close menus |
| **File > Save/Open** | Persist/restore the full graph as JSON |

## Adding a new node

1. Create `src/nodes/my_node.rs` with a `pub fn render(ui, ...)` function
2. Add a variant to `NodeType` in `src/graph.rs` — implement `title()`, `inputs()`, `outputs()`, `color_hint()`
3. Add evaluation logic in `Graph::evaluate()` if the node produces output values
4. Register in `src/nodes/mod.rs`: add `pub mod my_node`, a catalog entry, and a match arm in `render_content()`

## Tech

- **[eframe](https://github.com/emilk/egui/tree/master/crates/eframe)** / **egui** — immediate-mode GUI
- **[midir](https://crates.io/crates/midir)** — cross-platform MIDI I/O
- **[serialport](https://crates.io/crates/serialport)** — serial communication
- **[rosc](https://crates.io/crates/rosc)** — OSC protocol encoding/decoding
- **[rhai](https://crates.io/crates/rhai)** — embedded scripting engine
- **serde** — project serialization (JSON)
- **rfd** — native file dialogs
- **cargo-packager** — macOS `.app` / `.dmg` bundling

## License

MIT
