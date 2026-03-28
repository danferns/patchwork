@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let p = (uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r = length(p);

    let ring = sin(r * 20.0 - t * 3.0) * 0.5 + 0.5;
    let glow = smoothstep(0.8, 0.0, r);

    let red   = ring * u.bass * glow;
    let green = ring * u.mid * glow * 0.8;
    let blue  = ring * u.treble * glow;

    let dot = smoothstep(0.12, 0.08, r) * u.amp;

    return vec4<f32>(red + dot, green + dot * 0.5, blue + dot * 0.8, 1.0);
}
