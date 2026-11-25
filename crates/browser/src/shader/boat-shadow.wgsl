struct LightProjection { matrix: mat4x4<f32>, periodic: vec4<f32> };
@group(0) @binding(0) var<uniform> light_projection: LightProjection;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct ShadowVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) coverage: f32,
};

@vertex
fn vertex(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>, @location(2) coverage_or_projected_sheer_m: f32, @builtin(instance_index) instance: u32) -> ShadowVertex {
    let offset = select(0.0, (f32(instance) - 1.0) * light_projection.periodic.y, light_projection.periodic.w > 0.0);
    let coverage = select(coverage_or_projected_sheer_m, 1.0, color.z >= 0.0);
    return ShadowVertex(light_projection.matrix * vec4<f32>(position + vec3<f32>(offset, 0.0, 0.0), 1.0), color.xy, coverage);
}

@fragment
fn fragment(input: ShadowVertex) {
    if textureSample(image, image_sampler, input.uv).a * input.coverage < 0.5 {
        discard;
    }
}
