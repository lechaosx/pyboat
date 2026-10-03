fn encode_display_color(linear_color: vec3<f32>) -> vec3<f32> {
    if scene.exposure_and_output.z <= 0.0 {
        return linear_color;
    }
    // Floating-point WebGPU canvases expect extended sRGB encoding; SDR
    // attachments perform this conversion through their -srgb texture view.
    return select(
        1.055 * pow(max(linear_color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055,
        12.92 * linear_color,
        linear_color <= vec3<f32>(0.0031308),
    );
}

fn tone_map_scene_color(scene_color: vec3<f32>) -> vec3<f32> {
    let exposed = max(scene_color * scene.exposure_and_output.x, vec3<f32>(0.0));
    let maximum = max(exposed.r, max(exposed.g, exposed.b));
    let shoulder = 0.8;
    if maximum <= shoulder {
        return exposed;
    }
    let range = scene.exposure_and_output.y - shoulder;
    let compressed = shoulder + range * (1.0 - exp(-(maximum - shoulder) / range));
    // One scale for all channels preserves the hue of bright material colors.
    return exposed * (compressed / maximum);
}
