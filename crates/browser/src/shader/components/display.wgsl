fn linear_material_color(display_color: vec3<f32>) -> vec3<f32> {
    // Material palettes are authored in sRGB; light multiplies their linear reflectance.
    return select(
        pow((max(display_color, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4)),
        display_color / 12.92,
        display_color <= vec3<f32>(0.04045),
    );
}

fn night_vision_color(linear_color: vec3<f32>, adapted_gain: f32) -> vec3<f32> {
    let rod_fraction = smoothstep(0.0, 4.0, log2(max(1.0, adapted_gain)));
    // Jacobs et al. (2015), https://www.emilyacooper.org/pubs/2015Jacobs_TOG.pdf:
    // normalized scotopic RGB weights preserve neutral reflectance while rods
    // favor blue-green over red. Game light has no absolute luminance, so four
    // stops of adaptation gain stand in for the mesopic range of 0.005-5 cd/m².
    let rod_signal = dot(linear_color, vec3<f32>(0.0, 0.8451, 0.1459) / 0.991);
    return mix(linear_color, vec3<f32>(rod_signal), rod_fraction);
}

fn vision_noise_sample(cell: vec2<u32>, phase: u32) -> f32 {
    var key = cell.x * 0x9e3779b9u ^ cell.y * 0x85ebca6bu ^ phase * 0xc2b2ae35u;
    key = (key ^ (key >> 16u)) * 0x7feb352du;
    key = (key ^ (key >> 15u)) * 0x846ca68bu;
    key = key ^ (key >> 16u);
    return f32(key >> 8u) / 16777215.0 * 2.0 - 1.0;
}

fn vision_noise_field(cell: vec2<u32>, blend: vec2<f32>, phase: u32) -> f32 {
    return mix(
        mix(vision_noise_sample(cell, phase), vision_noise_sample(cell + vec2<u32>(1u, 0u), phase), blend.x),
        mix(vision_noise_sample(cell + vec2<u32>(0u, 1u), phase), vision_noise_sample(cell + vec2<u32>(1u), phase), blend.x),
        blend.y,
    );
}

fn adapted_view_color(linear_color: vec3<f32>, adapted_gain: f32, css_position_px: vec2<f32>, noise_phase: f32) -> vec3<f32> {
    if adapted_gain <= 1.0 {
        return linear_color;
    }
    let vision_color = night_vision_color(linear_color, adapted_gain);
    // The four-stop mesopic range of `night_vision_color`.
    let rod_fraction = smoothstep(0.0, 4.0, log2(adapted_gain));
    let signal = dot(vision_color, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Relative uncertainty grows as photon signal falls. Multiplicative noise
    // preserves black and has zero mean; the cap keeps faint hazards readable.
    // 0.002 sets the noise at unit signal in place of a photon count from
    // absolute luminance.
    let amplitude = rod_fraction * min(0.12, 0.002 / sqrt(max(signal, 0.000001)));
    // CSS-pixel cells keep the grain's on-screen size the same at every graphics setting.
    let grid = css_position_px / 4.0;
    let cell = vec2<u32>(floor(grid));
    let blend = smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), fract(grid));
    let phase = noise_phase % 4096.0;
    let first_phase = u32(floor(phase));
    let temporal_blend = smoothstep(0.0, 1.0, fract(phase));
    let noise = mix(
        vision_noise_field(cell, blend, first_phase),
        vision_noise_field(cell, blend, (first_phase + 1u) % 4096u),
        temporal_blend,
    );
    return vision_color * (1.0 + amplitude * noise);
}

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

fn tone_map_scene_color(scene_color: vec3<f32>, css_position_px: vec2<f32>) -> vec3<f32> {
    let exposed = max(
        adapted_view_color(scene_color, scene.exposure_and_output.x, css_position_px, scene.exposure_and_output.w) * scene.exposure_and_output.x,
        vec3<f32>(0.0),
    );
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
