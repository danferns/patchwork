@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let p = uv * 6.0;
    let v1 = sin(p.x + t);
    let v2 = sin(p.y + t * 0.7);
    let v3 = sin(p.x + p.y + t * 0.5);
    let v4 = sin(length(p - vec2<f32>(3.0, 3.0)) + t);
    let v = (v1 + v2 + v3 + v4) * 0.25;
    let r = sin(v * 3.14) * 0.5 + 0.5;
    let g = sin(v * 3.14 + 2.09) * 0.5 + 0.5;
    let b = sin(v * 3.14 + 4.18) * 0.5 + 0.5;
    return vec4<f32>(r, g, b, 1.0);
}
