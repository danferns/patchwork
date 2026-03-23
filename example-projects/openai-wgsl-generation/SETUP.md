# OpenAI WGSL Generation — Quick Start Guide

This example project demonstrates how to use Patchwork to generate WGSL shaders using OpenAI's API and render them in real-time.

## 1. Get Your OpenAI API Key

1. Go to [OpenAI Platform](https://platform.openai.com/account/api-keys)
2. Create a new API key
3. Copy the key (looks like `sk-proj-...`)

## 2. Set Up This Project

### Option A: Using the Included Files

```bash
cd example-projects/openai-wgsl-generation/

# Copy the example API keys file and add your key
cp api_keys.json.example api_keys.json

# Edit api_keys.json with your OpenAI API key:
# {
#   "openai": "sk-proj-YOUR_ACTUAL_KEY_HERE"
# }
```

### Option B: Manual Setup

1. Open Patchwork
2. Click **File → Open Project** → select `project.json` in this folder
3. The project will load with all nodes already connected

## 3. Configure Your API Key

Two ways to provide the API key:

### Way 1: Using api_keys.json (Recommended)

- The **OpenAI Config** text node in the graph is pre-configured to use the key from `api_keys.json`
- Just make sure your key is in `api_keys.json` with the key name `"openai"`
- The Config node will read it when you click "Send"

### Way 2: Direct Entry

- Edit the **OpenAI Config** text node
- Replace `sk-proj-YOUR_API_KEY_HERE` with your actual OpenAI API key
- This method is **not recommended** for shared projects (exposes secrets)

## 4. Run the Workflow

1. **Open** `project.json` in Patchwork
2. The workflow looks like:
   ```
   System Prompt →
   User Prompt   → AI Request Node → JSON Extract → WGSL Viewer
   Config        →
   ```
3. Click the **"▶ Send"** button on the **Generate Shader** node
4. Watch for status:
   - `⏳ Thinking...` → API call in progress
   - `🟢 done` → Success! Shader code is being processed
   - Check the **Live WGSL Viewer** for the rendered result

## 5. Customize the Prompts

Edit the text nodes to generate different shader effects:

### For Mandelbrot Fractal:
```
Create a WGSL fragment shader that renders the Mandelbrot set:
- Use complex number iteration (real + imag parts)
- Color based on iteration count with smooth gradients
- Zoom into interesting regions
- Use iTime for slow animation
```

### For Perlin Noise Clouds:
```
WGSL shader for animated cloud patterns:
- Implement Perlin noise using sin/cos based randomness
- Layer multiple octaves for cloud-like turbulence
- Use iTime to animate the noise
- Apply color gradient from dark to light
```

### For Cellular Automaton:
```
Conway's Game of Life in WGSL:
- 64x64 grid of cells
- Count live neighbors using discrete steps
- Apply rules: birth at 3, survival at 2-3
- Show dead cells black, live cells white
- Animate one generation per frame
```

## 6. Troubleshooting

### "401 Unauthorized"
- Your API key is invalid or expired
- Check that `api_keys.json` has the correct `openai` key
- Verify the key starts with `sk-proj-`

### "429 Too Many Requests"
- You've hit OpenAI's rate limit
- Wait a few minutes and try again
- Consider upgrading your OpenAI plan

### WGSL Viewer shows "Shader compilation error"
- The generated code has syntax errors
- Try adjusting your prompt to be more specific
- Example: "Keep the shader simple and focus on clarity over complexity"

### No response after clicking Send
- Network might be slow; wait 10-30 seconds
- Check Patchwork console for errors
- Verify your internet connection

## 7. Advanced: Add Animation with Time

To make your shader animate:

1. Add a **Time** node (if available in your Patchwork version)
2. Connect its output to the **iTime** input of the **Live WGSL Viewer**
3. In your shader prompt, instruct it to use `iTime` for animation

Example enhanced prompt:
```
Create a plasma shader with:
- Base color from sin(uv.x + iTime * 0.5)
- Wave patterns that move over time
- Color shifts that cycle smoothly
- Keep performance high at 60 FPS
```

## 8. Save Your Generated Shaders

When you generate a shader you like:

1. Copy the code from the **Extract Code** node output
2. Save it as a `.wgsl` file in your project folder
3. Later, you can load it with a **File** node instead of generating

This lets you build a library of shader effects without re-generating them.

## 9. Example Generated Outputs

Here are some good prompts that typically generate working WGSL:

### Noise-based Effects
```
Simplex noise-like shader using sin and cos:
- Use formula: sin(uv.x * 3.0 + iTime) * cos(uv.y * 3.0) * 0.5 + 0.5
- Create multiple layers with different frequencies
- Mix colors based on noise values
- Make it smooth and animated
```

### Geometric Patterns
```
Create geometric patterns using distance functions:
- Circle at center: distance from center
- Rectangle pattern: max(abs(uv.x), abs(uv.y))
- Animate scale and rotation with iTime
- Use color based on distance value
```

### Color Gradients
```
Smooth color gradient shader:
- Map uv coordinates to hue (0-360 degrees)
- Use fixed saturation and lightness
- Convert HSL to RGB
- Animate hue rotation with iTime
```

## 10. Next Steps

- **Combine multiple prompts**: Create different effects by varying the user prompt
- **Share projects**: Copy this folder and commit it to git (excluding `api_keys.json`)
- **Create templates**: Save working shader configurations as new project folders
- **Chain with other nodes**: Connect shader output to other processing nodes

---

**Enjoy generating shaders with AI!** ✨🎨
