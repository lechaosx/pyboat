override CANOPY_TRANSFER_OUTPUT: u32 = 0u;
override CANOPY_TRANSFER_OPTICAL_THICKNESS: f32 = 0.0;

// Light on level ground under a clear sky (0) or a full canopy (1), with each
// beam reduced by the canopy's regional beam transmittance, at half scale so
// clear noon light stays below the 8-bit ceiling.
@fragment
fn fragment_lighting_canopy_transfer(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    let canopy = RegionalCanopyConditions(
        select(0.0, 1.0, CANOPY_TRANSFER_OUTPUT == 1u),
        CANOPY_TRANSFER_OPTICAL_THICKNESS,
    );
    let receiver = celestial_receiver_light_at_height(0.0);
    let ambient = calculate_atmospheric_ambient_light(canopy, receiver);
    let split = ambient.split;
    let light = split.diffuse_share * ambient.clear_sky
        + ambient.canopy
        + ambient.canopy_scattered
        + ambient.beam_transmittance.x * split.direct.x * sun_direct_light_color(receiver)
        + ambient.beam_transmittance.y * split.direct.y * moon_direct_light_color(receiver);
    return vec4<f32>(0.5 * light, 1.0);
}
