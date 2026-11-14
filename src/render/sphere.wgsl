struct View {
    clip_from_world: mat4x4<f32>,
    eye: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) ray_point: vec3<f32>,
    @location(1) @interpolate(flat) center_radius: vec4<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) ray_point: vec3<f32>,
    @location(2) center_radius: vec4<f32>, @location(3) color: vec4<f32>) -> VertexOutput {
    return VertexOutput(vec4<f32>(position, 0.0, 1.0), ray_point, center_radius, color);
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let direction = normalize(input.ray_point - view.eye.xyz);
    let offset = view.eye.xyz - input.center_radius.xyz;
    let along = -dot(offset, direction);
    // Subtract the closest-point vector rather than squared distances for small spheres.
    let closest = offset + along * direction;
    let radius = input.center_radius.w;
    let chord = radius * radius - dot(closest, closest);
    if chord < 0.0 { discard; }
    let near = along - sqrt(chord);
    let far = along + sqrt(chord);
    let distance = select(far, near, near > 0.0);
    if distance <= 0.0 { discard; }

    let point = view.eye.xyz + distance * direction;
    let projected = view.clip_from_world * vec4<f32>(point, 1.0);
    let depth = projected.z / projected.w;
    if depth < 0.0 || depth > 1.0 { discard; }
    let normal = normalize(point - input.center_radius.xyz);
    let light = normalize(vec3<f32>(-0.5, 1.0, 0.7));
    let brightness = 0.25 + 0.75 * max(dot(normal, light), 0.0);
    return FragmentOutput(vec4<f32>(input.color.rgb * brightness, input.color.a), depth);
}
