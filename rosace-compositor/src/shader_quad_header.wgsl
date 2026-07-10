// Framework-provided prelude for registered shader pipelines (D109).
//
// Prepended to every user/built-in fragment source at `register_shader`
// time. Supplies the quad placement uniform and the standard vertex stage —
// pipeline authors write ONLY a fragment stage:
//
//     @group(0) @binding(1) var<uniform> u: MyUniforms;  // their struct
//     @fragment
//     fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> { … }
//
// `in.uv` runs (0,0) top-left → (1,1) bottom-right across the fill rect;
// `rosace_quad.size_px` is the rect size in physical pixels (for
// aspect-correct SDF math). Fragment output is LINEAR premultiplied-alpha
// color — the sRGB surface encodes on write (see build_cached_layer's
// gamma note in lib.rs).

struct RosaceQuad {
    // Clip-space placement: dest_min = (left, bottom), dest_max = (right, top).
    dest_min: vec2<f32>,
    dest_max: vec2<f32>,
    // Fill-rect size in physical pixels.
    size_px:  vec2<f32>,
    _pad:     vec2<f32>,
};
@group(0) @binding(0) var<uniform> rosace_quad: RosaceQuad;

struct RosaceVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> RosaceVsOut {
    // Two triangles from per-vertex corner factors — same pattern as the
    // layer compositor shader (shader.wgsl).
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let c = corner[idx];
    var out: RosaceVsOut;
    let ndc_x = mix(rosace_quad.dest_min.x, rosace_quad.dest_max.x, c.x);
    let ndc_y = mix(rosace_quad.dest_max.y, rosace_quad.dest_min.y, c.y);
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv  = c;
    return out;
}
