struct View {
    clip_from_world: mat4x4<f32>,
    eye: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) ray_point: vec3<f32>,
    @location(1) @interpolate(flat) start_radius: vec4<f32>,
    @location(2) @interpolate(flat) end: vec3<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

const NO_HIT: f32 = 1e30;
const PARALLEL_EPSILON: f32 = 1e-8;

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) ray_point: vec3<f32>,
    @location(2) start_radius: vec4<f32>, @location(3) end: vec3<f32>,
    @location(4) color: vec4<f32>) -> VertexOutput {
    return VertexOutput(vec4<f32>(position, 0.0, 1.0), ray_point, start_radius, end, color);
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let direction = normalize(input.ray_point - view.eye.xyz);
    let segment = input.end - input.start_radius.xyz;
    let length = length(segment);
    let axis = segment / length;
    let offset = view.eye.xyz - input.start_radius.xyz;
    let axial_origin = dot(offset, axis);
    let axial_direction = dot(direction, axis);
    let radial_origin = offset - axis * axial_origin;
    let radial_direction = direction - axis * axial_direction;
    let speed = dot(radial_direction, radial_direction);
    let radius = input.start_radius.w;
    var distance = NO_HIT;
    var normal = vec3<f32>(0.0);

    // Intersect the curved side using the closest radial vector for thin tubes.
    if speed > PARALLEL_EPSILON {
        let along = -dot(radial_origin, radial_direction) / speed;
        let closest = radial_origin + along * radial_direction;
        let chord = (radius * radius - dot(closest, closest)) / speed;
        if chord >= 0.0 {
            for (var root = 0u; root < 2u; root += 1u) {
                let candidate = along + select(-sqrt(chord), sqrt(chord), root == 1u);
                let height = axial_origin + candidate * axial_direction;
                if candidate > 0.0 && candidate < distance && height >= 0.0 && height <= length {
                    distance = candidate;
                    normal = normalize(radial_origin + candidate * radial_direction);
                }
            }
        }
    }

    // Close the ends so axial and inside views retain a solid surface.
    if abs(axial_direction) > PARALLEL_EPSILON {
        for (var cap = 0u; cap < 2u; cap += 1u) {
            let height = select(0.0, length, cap == 1u);
            let candidate = (height - axial_origin) / axial_direction;
            let radial = radial_origin + candidate * radial_direction;
            if candidate > 0.0 && candidate < distance && dot(radial, radial) <= radius * radius {
                distance = candidate;
                normal = select(-axis, axis, cap == 1u);
            }
        }
    }
    if distance == NO_HIT { discard; }

    let point = view.eye.xyz + distance * direction;
    let projected = view.clip_from_world * vec4<f32>(point, 1.0);
    let depth = projected.z / projected.w;
    if depth < 0.0 || depth > 1.0 { discard; }
    let light = normalize(vec3<f32>(-0.5, 1.0, 0.7));
    let brightness = 0.25 + 0.75 * max(dot(normal, light), 0.0);
    return FragmentOutput(vec4<f32>(input.color.rgb * brightness, input.color.a), depth);
}
