@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time * 0.5;
    let r = sin(uv.x * 3.14 + t) * 0.5 + 0.5;
    let g = sin(uv.y * 3.14 + t * 1.3) * 0.5 + 0.5;
    let b = sin((uv.x + uv.y) * 2.0 - t) * 0.5 + 0.5;
    return vec4<f32>(r, g, b, 1.0);
}
