# Patchwork Example Projects

This folder contains complete, ready-to-use example projects demonstrating Patchwork's capabilities.

## Projects

### 1. OpenAI WGSL Generation

**Location:** `openai-wgsl-generation/`

**What it does:** Generates WGSL shaders using OpenAI's GPT-4 API and renders them in real-time.

**Workflow:**
```
System Prompt →
User Prompt   → AI Request → JSON Extract → WGSL Viewer
Config/API Key→
```

**Files:**
- `project.json` — Complete node graph (load this in Patchwork)
- `api_keys.json.example` — Template for API credentials (copy to `api_keys.json`, add your key)
- `SETUP.md` — Step-by-step setup instructions
- `EXAMPLE_PROMPTS.md` — 10+ example prompts for different shader effects

**How to Use:**
1. Copy `api_keys.json.example` to `api_keys.json`
2. Add your OpenAI API key to `api_keys.json`
3. Open `project.json` in Patchwork
4. Click "Send" on the AI Request node
5. Watch the generated shader render in real-time

**Requirements:**
- OpenAI API key (get from https://platform.openai.com/account/api-keys)
- Internet connection for API calls

**Example Effects:**
- Animated plasma
- Mandelbrot fractals
- Cloud patterns
- Rotating shapes
- Wave interference
- Tunnel effects
- Fire patterns
- And more...

---

## Directory Structure

```
example-projects/
├── README.md (this file)
├── openai-wgsl-generation/
│   ├── project.json
│   ├── api_keys.json.example
│   ├── SETUP.md
│   └── EXAMPLE_PROMPTS.md
└── (more projects coming soon)
```

---

## Creating Your Own Example Project

To create a new example project:

1. **Create a folder** with a descriptive name:
   ```bash
   mkdir my-cool-project
   cd my-cool-project
   ```

2. **Save a Patchwork project** into it:
   - File → Save Project → select the folder
   - Patchwork creates `project.json` automatically

3. **Add supporting files:**
   - `SETUP.md` — How to set up and run it
   - `README.md` — What it does and expected results
   - `api_keys.json.example` — If it uses external APIs
   - Any other config files needed

4. **Document the workflow:**
   - Explain what each node does
   - Show the node connections visually
   - Give expected outputs

5. **Add example prompts/data:**
   - If applicable, include example text inputs
   - Show what the output should look like

---

## Tips for Example Projects

- **Keep it simple:** Focus on demonstrating one main concept
- **Make it usable:** Provide all necessary config templates
- **Document thoroughly:** New users should understand everything
- **Show results:** Include screenshots or descriptions of expected outputs
- **Test before sharing:** Make sure the project works end-to-end

---

## Coming Soon

Future example projects planned:
- **Anthropic API WGSL Generation** — Using Claude instead of OpenAI
- **Serial Port Logger** — Reading and logging serial data
- **MIDI Monitor** — Capturing and displaying MIDI messages
- **OSC Receiver** — Processing OSC network messages
- **Live Audio Visualization** — Real-time audio waveforms
- **Multi-AI Comparison** — Chain multiple AI providers

---

## Contributing Examples

To contribute your own example project:

1. Create a well-documented project folder
2. Include all setup instructions
3. Test it thoroughly
4. Submit as a pull request on GitHub
5. Include a screenshot or GIF showing it in action

---

## Troubleshooting

**Project won't load:**
- Make sure `project.json` is in the project root
- Check that the JSON is valid (use a JSON validator)
- Try opening from File menu instead of double-clicking

**"API key not found" error:**
- Create `api_keys.json` from the `.example` file
- Add your actual API key (don't use placeholder text)
- Make sure JSON syntax is correct (quotes, commas, etc.)

**WGSL Viewer shows errors:**
- Check generated code for syntax issues
- Try a simpler prompt next time
- Verify shader inputs/outputs match your configuration

---

**Happy experimenting!** 🎨✨
