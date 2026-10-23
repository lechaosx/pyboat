override CANOPY_TRANSFER_OUTPUT: u32 = 0u;
override CANOPY_TRANSFER_OPTICAL_THICKNESS: f32 = 0.0;

@fragment
fn fragment_lighting_canopy_transfer(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    let canopy = RegionalCanopyConditions(
        select(0.0, 1.0, CANOPY_TRANSFER_OUTPUT == 1u),
        CANOPY_TRANSFER_OPTICAL_THICKNESS,
    );
    let ambient = calculate_atmospheric_ambient_light(
        canopy,
        celestial_receiver_light_at_height(0.0),
    );
    return vec4<f32>(ambient.clear_sky + ambient.canopy + ambient.canopy_scattered, 1.0);
}
