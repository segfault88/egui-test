struct Globals {
    // World space -> clip space for the camera.
    view_proj: mat4x4<f32>,
    // World space -> clip space as seen from the light. Used to render and look up the shadow map.
    light_view_proj: mat4x4<f32>,
    // Camera position in world space (w unused).
    camera_position: vec4<f32>,
    // Unit vector pointing from the scene towards the light (w unused).
    light_direction: vec4<f32>,
    // Amount of light that reaches surfaces facing away from the light.
    ambient: f32,
    // 1.0 if we must convert our linear colors to sRGB ourselves, because the render target
    // doesn't do it for us.
    encode_srgb: f32,
    // 1.0 if shadows are enabled, 0.0 otherwise.
    shadows: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

// The scene's depth as seen from the light, rendered by `vs_shadow` each frame.
@group(1) @binding(0)
var shadow_map: texture_depth_2d;
// Compares a depth against the shadow map, with bilinear filtering of the result.
@group(1) @binding(1)
var shadow_sampler: sampler_comparison;

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

// Shadow pass: renders depth only, from the light's point of view.
@vertex
fn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    return globals.light_view_proj * model * vec4<f32>(vertex.position, 1.0);
}

// Returns how much of the light reaches this point: 0.0 = fully shadowed, 1.0 = fully lit.
fn light_visibility(world_position: vec3<f32>, normal: vec3<f32>) -> f32 {
    // Nudge the lookup point off the surface so it doesn't shadow itself ("shadow acne").
    let position = world_position + normal * 0.02;

    // Where is this point in the shadow map, and how far is it from the light?
    // The light's projection is orthographic, so there is no perspective divide.
    let light_clip = globals.light_view_proj * vec4<f32>(position, 1.0);
    let uv = light_clip.xy * vec2<f32>(0.5, -0.5) + 0.5;
    let depth = light_clip.z;

    // Percentage-closer filtering: average 3x3 comparisons to soften the shadow's edge.
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map));
    var visibility = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            visibility += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, depth);
        }
    }
    return visibility / 9.0;
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

    // Shadows block the direct light, but not the ambient light.
    let visibility = mix(1.0, light_visibility(in.world_position, normal), globals.shadows);

    var color = in.color * (globals.ambient + diffuse * visibility) + vec3<f32>(specular * visibility);

    if globals.encode_srgb > 0.5 {
        color = linear_to_srgb(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)));
    }
    return vec4<f32>(color, 1.0);
}
