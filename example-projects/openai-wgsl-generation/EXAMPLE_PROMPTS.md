# Example Prompts for WGSL Shader Generation

Copy and paste these prompts into the **User Prompt** text node to generate different shader effects.

## 1. Animated Plasma

**System:**
```
You are a WGSL shader expert. Generate only valid WGSL code.
Use uniforms: iTime (f32), iResolution (vec2<f32>), fragCoord (vec2<f32>).
Output: rgba color as vec4<f32>.
```

**Prompt:**
```
Create an animated plasma effect shader:
- Normalize coordinates to 0-1 range
- Use sin(uv.x * 3.0 + iTime) and cos(uv.y * 3.0 - iTime)
- Layer 3 frequencies for complex patterns: sin(len*5), sin(len*3), sin(len*7)
- Create a color gradient: red from sin, green from cos, blue from time
- Keep it smooth and performant
```

**Expected Output:** Rippling, color-shifting plasma effect

---

## 2. Mandelbrot Fractal

**Prompt:**
```
WGSL Mandelbrot set renderer:
- Normalize fragment coordinates to -2 to 2 range (real) and -1.5 to 1.5 range (imaginary)
- Iterate complex number z = z^2 + c up to 100 iterations
- Stop if |z| > 2
- Color by iteration count: use vec3(f32(i) / 100.0) for grayscale
- Invert colors: 1.0 - color for dark background
- Add zoom over time: scale coordinates by 1.0 + iTime * 0.1
```

**Expected Output:** Deep, zooming Mandelbrot fractal

---

## 3. Simplex Noise Clouds

**Prompt:**
```
Animated cloud pattern shader using noise approximation:
- Use layered sin/cos for Perlin-like noise
- Calculate noise as: sin(uv.x * scale + iTime) * cos(uv.y * scale - iTime)
- Layer 4 octaves: scale 2, 4, 8, 16 with decreasing amplitude
- Sum layers: result = (n1 + n2/2 + n3/4 + n4/8) / (1 + 0.5 + 0.25 + 0.125)
- Create soft white clouds: color = vec3<f32>(noise_value)
- Add motion: shift uv by sin(iTime) * 0.1
```

**Expected Output:** Flowing cloud patterns

---

## 4. Color Grid

**Prompt:**
```
Create a smooth color grid shader:
- Divide screen into 8x8 grid using floor(uv * 8.0)
- Calculate grid index: gx = floor(uv.x * 8.0), gy = floor(uv.y * 8.0)
- Create color from grid position: hue = (gx + gy * 8) / 64 * 360
- Use HSL to RGB conversion (fixed saturation=1.0, lightness=0.5)
- Add animation by rotating hue: hue += iTime * 30
- Smooth transitions using fract() for sub-pixel gradients
```

**Expected Output:** Colorful animated grid

---

## 5. Rotating Shapes

**Prompt:**
```
Rotating geometric shapes shader:
- Calculate distance from center: dist = length(uv - 0.5)
- Calculate angle from center: angle = atan2(uv.y - 0.5, uv.x - 0.5)
- Rotate angle: angle += iTime
- Create rings: rings = sin(dist * 10.0 + angle) * 0.5 + 0.5
- Create rays: rays = sin(angle * 6.0) * 0.5 + 0.5
- Combine: color = vec3(rings * rays * (1.0 - dist))
- Use color mapping: red, green, blue channels from different parts
```

**Expected Output:** Spinning kaleidoscope-like pattern

---

## 6. Wave Interference

**Prompt:**
```
Wave interference pattern:
- Create two sine waves from different directions
- Wave 1: sin((uv.x + uv.y) * 5.0 + iTime * 2.0)
- Wave 2: sin((uv.x - uv.y) * 5.0 - iTime * 2.0)
- Combine with multiplication for interference: result = wave1 * wave2
- Create standing wave pattern: result = abs(result)
- Color gradient: use result value to interpolate between colors
- Add smooth transitions with smoothstep() for anti-aliasing
```

**Expected Output:** Interference patterns with moving waves

---

## 7. Tunnel Effect

**Prompt:**
```
3D tunnel shader using polar coordinates:
- Calculate distance from center: dist = length(uv - 0.5)
- Calculate angle from center: angle = atan2(uv.y - 0.5, uv.x - 0.5)
- Tunnel position: tunnel_pos = (iTime - 1.0 / dist) * 0.5
- Create rings: rings = sin(tunnel_pos * 20.0 + angle * 3.0) * 0.5 + 0.5
- Create radial pattern: radial = mod(1.0 / (dist + 0.1), 0.2) / 0.2
- Combine: color = vec3(rings * radial)
- Fade edges: multiply by (1.0 - dist) for tunnel effect
```

**Expected Output:** Flying through a tunnel

---

## 8. Fire Effect

**Prompt:**
```
Procedural fire effect shader:
- Create base heat using layered noise and sine waves
- Calculate heat as: (sin(uv.y * 10.0 + iTime) + sin(uv.x * 5.0)) * 0.5
- Add turbulence: heat += sin(uv.x * 3.0 - iTime) * 0.3
- Apply gravity by increasing darkness at top: heat *= (1.0 - uv.y)
- Color gradient:
  - Dark red (0-0.3): rgb(0.5, 0, 0)
  - Orange (0.3-0.6): rgb(1.0, 0.5, 0)
  - Yellow (0.6-1.0): rgb(1.0, 1.0, 0)
- Use heat value to interpolate between colors
```

**Expected Output:** Flickering fire-like patterns

---

## 9. Checkerboard Pattern

**Prompt:**
```
Animated checkerboard shader:
- Create checkerboard: checker = mod(floor(uv.x * 8.0) + floor(uv.y * 8.0), 2.0)
- This gives 0 or 1 alternating
- Create animation: animate = sin(iTime * 2.0) * 0.5 + 0.5
- Invert based on animation: checker = mix(checker, 1.0 - checker, animate)
- Apply color: color = vec3(checker)
- Add borders with smoothstep for anti-aliasing
```

**Expected Output:** Flashing checkerboard

---

## 10. Polar Coordinates Spiral

**Prompt:**
```
Logarithmic spiral shader:
- Convert Cartesian to polar: r = length(uv - 0.5), theta = atan2(uv.y - 0.5, uv.x - 0.5)
- Create logarithmic spiral: spiral = theta - log(r) * 2.0 + iTime
- Create spiral arms: arms = sin(spiral * 5.0) * 0.5 + 0.5
- Fade by distance: fade = 1.0 / (1.0 + r * 2.0)
- Create rings: rings = sin(r * 10.0 - iTime) * 0.5 + 0.5
- Combine: color = vec3(arms * rings * fade)
```

**Expected Output:** Spinning spiral galaxy

---

## Tips for Best Results

1. **Keep it simple**: Start with basic sine/cosine patterns, build complexity gradually
2. **Use iTime wisely**: Add `+ iTime` or `- iTime` to parameters for smooth animation
3. **Normalize coordinates**: Always map fragment coordinates to 0-1 or -1 to 1 range
4. **Test edge cases**: Make sure values don't go infinite (check denominators)
5. **Performance**: Avoid nested loops; use iterative formulas instead

## Testing Different Models

Same prompts work with different models:

- **gpt-4-turbo**: Best quality, handles complex algorithms
- **gpt-4**: Good balance of quality and speed
- **gpt-3.5-turbo**: Faster but may need simpler prompts

## Modifying Generated Code

If a shader doesn't work perfectly:

1. Check the error message in the WGSL Viewer
2. Common issues:
   - Undefined uniforms → Add them to the prompt
   - Type mismatches → Ask AI to fix type conversions
   - Infinite loops → Ask AI to use explicit iteration counts
3. You can edit the code directly in the WGSL Viewer
4. Save working versions as `.wgsl` files for reuse

---

**Happy creative coding!** 🎨✨
