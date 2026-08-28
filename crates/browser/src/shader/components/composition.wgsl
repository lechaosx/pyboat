fn cloud_color(
    surface_color: vec3<f32>,
    cloud_sample: LowCloudSample,
    upper_transmittance: vec2<f32>,
) -> vec3<f32> {
    let cloud_amount = cloud_sample.amount;
    let relief_gradient = tuning.cloud_rendering.w * cloud_sample.gradient;
    let limited_gradient = relief_gradient
        / (1.0 + length(relief_gradient) / 0.25);
    let cloud_normal = normalize(vec3<f32>(
        -limited_gradient,
        1.0,
    ));
    let sun_facing = max(dot(cloud_normal, sun_direction()), 0.0);
    let moon_facing = max(dot(cloud_normal, moon_direction()), 0.0);
    let direct_light = direct_sun_amount() * sun_facing
        + direct_moon_amount() * moon_facing;
    let sunlight = direct_sun_amount() * upper_transmittance.x
        + direct_moon_amount() * upper_transmittance.y;
    let relief = cloud_sample.relief / max(cloud_amount, 0.001);
    let edge_light = sunlight
        * (1.0 - smoothstep(0.30, 0.78, cloud_amount))
        * smoothstep(0.10, 0.55, cloud_amount)
        * 0.18;
    let depth_shadow = smoothstep(0.45, 1.35, cloud_sample.optical_depth) * 0.14;
    let cloud_material = mix(
        vec3<f32>(0.68, 0.72, 0.74),
        vec3<f32>(0.96, 0.97, 0.96),
        0.30 + 0.45 * relief + 0.25 * direct_light,
    );
    var cloud = max(vec3<f32>(0.16, 0.18, 0.20), mix(
        cloud_material,
        vec3<f32>(0.12, 0.15, 0.17),
        clamp(cloud_sample.darkness * tuning.cloud_rendering.z, 0.0, 1.0),
    ) * (0.70 + sunlight * mix(0.12, 0.36, direct_light))
        + edge_light
        - depth_shadow);
    cloud *= ambient_contribution_color()
        + upper_transmittance.x
            * sun_direct_contribution_color() * mix(0.25, 1.0, sun_facing)
        + upper_transmittance.y
            * moon_direct_contribution_color() * mix(0.25, 1.0, moon_facing);
    let body = smoothstep(0.08, 0.18, cloud_amount);
    let optical_depth = tuning.cloud_rendering.x * cloud_sample.optical_depth;
    let opacity = tuning.cloud_rendering.y * body * (1.0 - exp(-1.35 * optical_depth));
    return mix(surface_color, cloud, opacity);
}

fn upper_cloud_transmittance_at_low_cloud_for_light(
    position_m: vec2<f32>,
    low_height_m: f32,
    upper_height_m: f32,
    light: vec3<f32>,
    light_amount: f32,
) -> f32 {
    if light_amount <= 0.0 || light.z <= 0.0 {
        return 1.0;
    }
    let projected_position_m = projected_cloud_position(
        position_m,
        low_height_m,
        upper_height_m,
        light,
    );
    let footprint_m = max(
        visual_pixel_footprint_m,
        finite_source_shadow_footprint_m(upper_height_m - low_height_m),
    );
    let upper = procedural_upper_cloud_sample_filtered(
        projected_position_m,
        footprint_m,
    );
    return mix(1.0, cloud_shadow_transmittance(upper.optical_depth, light), layer_visibility.w);
}

fn upper_cloud_transmittance_at_low_cloud(
    position_m: vec2<f32>,
    low_height_m: f32,
    upper_height_m: f32,
) -> vec2<f32> {
    var transmittance = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let is_sun = source_index == 0u;
        let light = select(moon_direction(), sun_direction(), is_sun);
        let light_amount = select(direct_moon_amount(), direct_sun_amount(), is_sun);
        transmittance[source_index] = upper_cloud_transmittance_at_low_cloud_for_light(
            position_m,
            low_height_m,
            upper_height_m,
            light,
            light_amount,
        );
    }
    return transmittance;
}

fn cloud_shadow_color(
    surface_color: vec3<f32>,
    position_m: vec2<f32>,
    elevation_m: f32,
    terrain_shadow_transmittance: vec2<f32>,
    landform_grade: vec2<f32>,
) -> vec3<f32> {
    let surface_height_m = max(elevation_m, 0.0);
    let depths = celestial_cloud_shadow_depths_at(position_m, surface_height_m);
    let sun_transmittance = cloud_shadow_transmittance(
        depths.low.x + depths.upper.x,
        sun_direction(),
    );
    let moon_transmittance = cloud_shadow_transmittance(
        depths.low.y + depths.upper.y,
        moon_direction(),
    );
    return surface_color * (
        landform_grade.x * ambient_contribution_color()
            + terrain_shadow_transmittance.x
                * sun_transmittance
                * landform_grade.y
                * sun_direct_contribution_color()
            + terrain_shadow_transmittance.y
                * moon_transmittance
                * landform_grade.y
                * moon_direct_contribution_color()
    );
}

fn precipitation_haze_color(surface_color: vec3<f32>) -> vec3<f32> {
    let precipitation = surface_cloud_weather.rain;
    if precipitation <= 0.001 {
        return surface_color;
    }
    let snow_fraction = 1.0 - smoothstep(
        -1.0,
        2.0,
        surface_current_air_temperature_c(),
    );
    let distance_amount = smoothstep(
        180.0,
        3500.0,
        scene.camera_and_view_m.w,
    );
    let haze_amount = precipitation * mix(0.025, 0.22, distance_amount);
    let haze_color = mix(
        0.62 * atmospheric_ambient_color(),
        vec3<f32>(0.72, 0.76, 0.78) * atmospheric_ambient_color(),
        snow_fraction,
    );
    return mix(surface_color, haze_color, haze_amount);
}

fn final_scene_color(input: VertexOutput) -> vec4<f32> {
    var surface_color = input.color;
    if input.is_terrain > 0.5 {
        let terrain = procedural_terrain_sample(input.world_position_m);
        let water_height_m = tide_height_m();
        let is_land = terrain.elevation_m >= water_height_m;
        var land_surface = 0.0;
        if is_land && terrain.water_ice_amount == 0.0 {
            land_surface = material_fractal_sample(
                vec2<u32>(0x6a09e667u, 0xbb67ae85u),
                input.world_position_m,
                96.0,
                4u,
            ).value;
        }
        var surface: TerrainSurface;
        if terrain.water_ice_amount > 0.0 {
            surface = sea_ice_surface(
                input.world_position_m,
                terrain.water_ice_amount,
                terrain.bedrock_elevation_m,
            );
        } else {
            surface = terrain_surface(
                input.world_position_m,
                terrain.elevation_m,
                terrain.gradient,
                terrain.material_elevation_m,
                terrain.material_gradient,
                terrain.coastal_rock_amount,
                terrain.coastal_outcrop_amount,
                terrain.coastal_sediment_amount,
                water_height_m,
                land_surface,
            );
        }
        var surface_light_grade = vec2<f32>(1.0);
        if is_land {
            if terrain.water_ice_amount == 0.0 && terrain.ice_amount > 0.0 {
                let ice = ice_surface(input.world_position_m, land_surface);
                surface.color = mix(
                    surface.color,
                    ice.color,
                    terrain.ice_amount,
                );
                surface.detail_gradient = mix(
                    surface.detail_gradient,
                    ice.detail_gradient,
                    terrain.ice_amount,
                );
                surface.roughness = mix(
                    surface.roughness,
                    ice.roughness,
                    terrain.ice_amount,
                );
            }
            surface_light_grade = landform_light_grade(
                3.0 * terrain.gradient + surface.detail_gradient,
                surface.roughness,
            );
        } else {
            surface_light_grade = landform_light_grade(
                surface.detail_gradient,
                surface.roughness,
            );
        }
        surface_color = surface.color;
        var terrain_shadow_transmittance = vec2<f32>(1.0);
        if is_land {
            terrain_shadow_transmittance = landform_shadow_transmittance(
                input.world_position_m,
                terrain.elevation_m,
            );
        }
        surface_color = cloud_shadow_color(
            surface_color,
            input.world_position_m,
            max(terrain.elevation_m, water_height_m),
            terrain_shadow_transmittance,
            surface_light_grade,
        );
        surface_color = precipitation_haze_color(surface_color);
        if layer_visibility.x > 0.0 {
            let low_height_m = LOW_CLOUD_BASE_HEIGHT_M;
            let upper_height_m = WEATHER_CANOPY_HEIGHT_M;
            let low_cloud = procedural_low_cloud_sample_with_weather(
                input.world_position_m,
                surface_cloud_weather,
                visual_pixel_footprint_m,
            );
            var upper_transmittance = vec2<f32>(1.0);
            if low_cloud.amount > 0.01 && layer_visibility.w > 0.0 {
                upper_transmittance = upper_cloud_transmittance_at_low_cloud(
                    input.world_position_m,
                    low_height_m + tuning.cloud_rendering.w * low_cloud.height_m,
                    upper_height_m,
                );
            }
            surface_color = mix(
                surface_color,
                cloud_color(surface_color, low_cloud, upper_transmittance),
                layer_visibility.x,
            );
        }
    }
    return vec4<f32>(surface_color, 1.0);
}
