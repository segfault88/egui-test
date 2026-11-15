struct Globals {
    // World space -> clip space for the camera.
    view_proj: mat4x4<f32>,
    // Camera position in world space (w unused).
    camera_position: vec4<f32>,
    // Unit vector pointing from the scene towards the light (w unused).
    light_direction: vec4<f32>,
    // Amount of light that reaches surfaces facing away from the light.
    ambient: f32,
    // 1.0 if we must convert our linear colors to sRGB ourselves, because the render target
    // doesn't do it for us.
    encode_srgb: f32,
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
    @location(1) world_position: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let world_position = model * vec4<f32>(vertex.position, 1.0);

    var out: VertexOutput;
    out.clip_position = globals.view_proj * world_position;
    out.color = instance.color.rgb;
    out.world_position = world_position.xyz;
    // The model matrix only translates and scales uniformly, so it can transform normals as-is.
    out.world_normal = (model * vec4<f32>(vertex.position, 0.0)).xyz;
    return out;
}

fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let low = linear * 12.92;
    let high = 1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, linear <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let to_light = globals.light_direction.xyz;
    let to_camera = normalize(globals.camera_position.xyz - in.world_position);

    // Lambert diffuse: brightness falls off with the angle between the surface and the light.
    let n_dot_l = dot(normal, to_light);
    let diffuse = max(n_dot_l, 0.0);

    // Blinn-Phong specular highlight, only on the lit side.
    let half_vector = normalize(to_light + to_camera);
    let specular = 0.3 * pow(max(dot(normal, half_vector), 0.0), 48.0) * step(0.0, n_dot_l);

    var color = in.color * (globals.ambient + diffuse) + vec3<f32>(specular);

    if globals.encode_srgb > 0.5 {
        color = linear_to_srgb(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)));
    }
    return vec4<f32>(color, 1.0);
}
