struct Uniforms {
    // Rotation around the Z axis, in radians.
    angle: f32,
    // Viewport width / height, so the triangle isn't stretched.
    aspect: f32,
    // Padding: uniform buffers must be a multiple of 16 bytes on WebGL2.
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    // The classic triangle: no vertex buffer, positions and colors come from the vertex index.
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.7),
        vec2<f32>(-0.6, -0.5),
        vec2<f32>(0.6, -0.5),
    );
    var colors = array<vec3<f32>, 3>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0),
    );

    let p = positions[index];
    let c = cos(uniforms.angle);
    let s = sin(uniforms.angle);
    let rotated = vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);

    var out: VertexOutput;
    out.position = vec4<f32>(rotated.x / uniforms.aspect, rotated.y, 0.0, 1.0);
    out.color = colors[index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
