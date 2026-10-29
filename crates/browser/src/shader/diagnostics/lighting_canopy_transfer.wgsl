override CANOPY_TRANSFER_OUTPUT: u32 = 0u;
override CANOPY_TRANSFER_OPTICAL_THICKNESS: f32 = 0.0;

// Light on level ground under a clear sky (0) or a full canopy (1), with each
// beam reduced by the canopy's regional beam transmittance, as a view settled
// on clear-sky light exposes it and at half scale, so clear noon light stays below the 8-bit
// ceiling and night light above its floor.
// Output 2 sweeps the Sun, with the Moon set, from 0.5 degrees above the
// canopy's level on the left to 2.5 degrees below it on the right, under half
// cover: red is the light the canopy scatters down, in hundredths of a zenith
// Sun's clear-sky light, and green its red share.
@fragment
fn fragment_lighting_canopy_transfer(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    if CANOPY_TRANSFER_OUTPUT == 2u {
        let css_view_width_px = scene.precipitation_phase_and_css_view_height_px.y
            * scene.camera_and_view_m.z / scene.camera_and_view_m.w;
        let depression = radians(mix(-0.5, 2.5, input.css_position_px.x / css_view_width_px));
        sun_light_direction = vec3<f32>(cos(depression), 0.0, -sin(depression));
        geocentric_moon_light_direction = vec3<f32>(0.0, 0.0, -1.0);
        let scattered = calculate_atmospheric_ambient_light(
            RegionalCanopyConditions(0.5, CANOPY_TRANSFER_OPTICAL_THICKNESS),
            celestial_receiver_light_at_height(0.0),
        ).canopy_scattered;
        return vec4<f32>(
            100.0 * dot(scattered, LUMINANCE_WEIGHTS),
            scattered.r / max(scattered.r + scattered.g + scattered.b, 1e-30),
            0.0,
            1.0,
        );
    }
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
    return vec4<f32>(0.5 * adapted_gain(clear_sky_intensity(receiver)) * light, 1.0);
}
