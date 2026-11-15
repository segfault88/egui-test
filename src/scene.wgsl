struct Globals {
    // World space -> clip space for the camera.
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    // Position on the unit sphere. This is also the normal.
    @location(0) position: vec3<f32>,
}

// Per-sphere data, one instance each.
struct InstanceInput {
    @location(1) model_0: vec4<f32>,
    @location(2) model_1: vec4<f32>,
    @location(3) model_2: vec4<f32>,
    @location(4) model_3: vec4<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let world_position = model * vec4<f32>(vertex.position, 1.0);

    var out: VertexOutput;
    out.clip_position = globals.view_proj * world_position;
    out.color = instance.color.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
