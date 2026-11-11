@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(@location(0) position: vec4<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VertexOutput {
    return VertexOutput(position, uv, color);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas, atlas_sampler, input.uv).r;
    return vec4<f32>(input.color.rgb, input.color.a * coverage);
}
