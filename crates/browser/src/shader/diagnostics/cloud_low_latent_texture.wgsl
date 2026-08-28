@fragment
fn fragment_cloud_low_latent_texture(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_cloud_low_latent_texture(initialize_playground_base(input));
}
