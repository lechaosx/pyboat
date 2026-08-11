struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) elevation_m: f32,
    @location(3) is_terrain: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) elevation_m: f32,
    @location(2) is_terrain: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    output.elevation_m = input.elevation_m;
    output.is_terrain = input.is_terrain;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.is_terrain > 0.5 {
        var terrain_color = vec3<f32>(0.025, 0.11, 0.15);
        if input.elevation_m >= -4.0 {
            terrain_color = vec3<f32>(0.04, 0.19, 0.25);
        }
        if input.elevation_m >= -1.0 {
            terrain_color = vec3<f32>(0.12, 0.42, 0.5);
        }
        if input.elevation_m >= 0.0 {
            terrain_color = vec3<f32>(0.76, 0.67, 0.43);
        }
        if input.elevation_m >= 0.4 {
            terrain_color = vec3<f32>(0.35, 0.31, 0.25);
        }
        if input.elevation_m >= 2.0 {
            terrain_color = vec3<f32>(0.15, 0.28, 0.17);
        }
        return vec4<f32>(terrain_color, 1.0);
    }
    return vec4<f32>(input.color, 1.0);
}
