// Placed-quad compositor shader (D075, D080-D081, D090).
//
// The vertex stage generates two triangles (6 vertices) from a per-vertex
// corner factor in {0,1}². The quad is positioned in clip space between
// `dest_min` and `dest_max` (NDC), and its UVs run from `uv_min` spanning
// `uv_span`. This lets a layer cover the whole surface (base/overlay) OR a
// viewport sub-rectangle (a scroll layer, D090) that samples a content-sized
// texture at a scroll offset.
//
// UV origin (0,0) is top-left to match tiny-skia's pixel layout. Out-of-range
// UV returns transparent (alpha=0), revealing the layer below — this both
// bounds a scroll layer to its content extent and confines it to its viewport.

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
};

struct LayerUniforms {
    // Clip-space (NDC) placement of the quad. dest_min = (left, bottom),
    // dest_max = (right, top). Full surface = (-1,-1)..(1,1).
    dest_min: vec2<f32>,
    dest_max: vec2<f32>,
    // UV at the top-left corner and the UV span across the quad.
    // Full-texture = uv_min (0,0), uv_span (1,1).
    uv_min:   vec2<f32>,
    uv_span:  vec2<f32>,
};

@group(0) @binding(0) var t_frame:  texture_2d<f32>;
@group(0) @binding(1) var s_frame:  sampler;
@group(0) @binding(2) var<uniform> u_layer: LayerUniforms;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Per-vertex corner factor: x in {0=left,1=right}, y in {0=top,1=bottom}.
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let c = corner[idx];

    // Position: lerp NDC x left→right, and y top→bottom (corner.y=0 is top).
    let ndc_x = mix(u_layer.dest_min.x, u_layer.dest_max.x, c.x);
    let ndc_y = mix(u_layer.dest_max.y, u_layer.dest_min.y, c.y);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv            = u_layer.uv_min + c * u_layer.uv_span;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    // Transparent for out-of-range UV (D081 — content boundary / viewport clip).
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return textureSample(t_frame, s_frame, uv);
}
