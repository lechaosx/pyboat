struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) is_terrain: f32,
};

struct VertexOutput {
    // Equal-depth passes require the same clip positions across pipelines.
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_position_m: vec2<f32>,
    @location(2) @interpolate(flat) is_terrain: f32,
    // Position in CSS pixels from the top left, independent of render resolution.
    @location(3) css_position_px: vec2<f32>,
    @location(4) coverage: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    output.world_position_m = scene.camera_and_view_m.xy
        + 0.5 * input.position * scene.camera_and_view_m.zw;
    output.is_terrain = input.is_terrain;
    // Overlay edges encode coverage in [-1, 0], retaining the vertex layout
    // shared by gameplay, the playground, and headless capture.
    output.coverage = clamp(1.0 + input.is_terrain, 0.0, 1.0);
    let css_view_height_px = scene.precipitation_phase_and_css_view_height_px.y;
    output.css_position_px = (vec2<f32>(0.5, -0.5) * input.position + 0.5) * vec2<f32>(
        css_view_height_px * scene.camera_and_view_m.z / scene.camera_and_view_m.w,
        css_view_height_px,
    );
    return output;
}
