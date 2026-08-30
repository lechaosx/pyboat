fn cloud_color(
    surface_color: vec3<f32>,
    cloud_sample: LowCloudSample,
    upper_transmittance: vec2<f32>,
    contribution_visibility: vec4<f32>,
    ambient_sky: RegionalCanopyConditions,
) -> vec3<f32> {
    let cloud_amount = cloud_sample.body_amount;
    let limited_gradient = cloud_sample.gradient
        / (1.0 + length(cloud_sample.gradient) / 0.25);
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
    let depth = cloud_sample.optical_depth / max(cloud_amount, 0.001);
    let edge_light = sunlight
        * (1.0 - smoothstep(0.30, 0.78, cloud_amount))
        * smoothstep(0.10, 0.55, cloud_amount)
        * tuning.cloud_lighting.x;
    let depth_shadow = smoothstep(0.45, 1.35, cloud_sample.optical_depth)
        * tuning.cloud_lighting.y;
    let cloud_material = mix(
        vec3<f32>(0.68, 0.72, 0.74),
        vec3<f32>(0.96, 0.97, 0.96),
        0.30 + 0.36 * min(depth / 1.25, 1.0) + 0.25 * direct_light,
    );
    var cloud = max(vec3<f32>(0.16, 0.18, 0.20), mix(
        cloud_material,
        vec3<f32>(0.12, 0.15, 0.17),
        clamp(0.06 * cloud_amount * tuning.cloud_rendering.z, 0.0, 1.0),
    ) * (0.70 + sunlight * mix(0.12, 0.36, direct_light))
        + edge_light
        - depth_shadow);
    cloud *= low_cloud_material_color_visibility() * (
        ambient_contribution_share() * (
            contribution_visibility.z * calculate_atmospheric_ambient_color_with_visibility(
                ambient_sky,
                vec2<f32>(1.0, 0.0),
            )
            + contribution_visibility.w * calculate_atmospheric_ambient_color_with_visibility(
                ambient_sky,
                vec2<f32>(0.0, 1.0),
            )
        )
        + contribution_visibility.x * upper_transmittance.x
            * sun_direct_contribution_color() * mix(0.25, 1.0, sun_facing)
        + contribution_visibility.y * upper_transmittance.y
            * moon_direct_contribution_color() * mix(0.25, 1.0, moon_facing)
    );
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
    canopy: RegionalCanopyConditions,
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
    let upper_optical_depth = procedural_upper_cloud_optical_depth(
        projected_position_m,
        canopy,
    );
    return mix(
        1.0,
        cloud_shadow_transmittance(upper_optical_depth, light),
        layer_visibility.w,
    );
}

fn upper_cloud_transmittance_at_low_cloud(
    position_m: vec2<f32>,
    low_height_m: f32,
    upper_height_m: f32,
    canopy: RegionalCanopyConditions,
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
            canopy,
        );
    }
    return transmittance;
}

fn fragment_surface_color(
    surface_color: vec3<f32>,
    position_m: vec2<f32>,
    elevation_m: f32,
    terrain_shadow_transmittance: vec2<f32>,
    landform_grade: vec2<f32>,
    ambient_sky: RegionalCanopyConditions,
    weather: CloudWeatherSample,
) -> vec3<f32> {
    let layer = fragment_layer_visibility();
    let contribution = fragment_light_visibility();
    let depths = celestial_cloud_shadow_depths_at(
        position_m,
        max(elevation_m, 0.0),
        weather,
        ambient_sky,
    );
    let sun_transmittance = cloud_shadow_transmittance(
        layer.y * depths.low.x + layer.z * depths.upper.x,
        sun_direction(),
    );
    let moon_transmittance = cloud_shadow_transmittance(
        layer.y * depths.low.y + layer.z * depths.upper.y,
        moon_direction(),
    );
    let ambient = ambient_contribution_share() * (
        contribution.z * calculate_atmospheric_ambient_color_with_visibility(
            ambient_sky,
            vec2<f32>(1.0, 0.0),
        )
        + contribution.w * calculate_atmospheric_ambient_color_with_visibility(
            ambient_sky,
            vec2<f32>(0.0, 1.0),
        )
    );
    let light = landform_grade.x * ambient
        + contribution.x
            * terrain_shadow_transmittance.x
            * sun_transmittance
            * landform_grade.y
            * sun_direct_contribution_color()
        + contribution.y
            * terrain_shadow_transmittance.y
            * moon_transmittance
            * landform_grade.y
            * moon_direct_contribution_color();
    return surface_color * light;
}

fn final_scene_color(
    input: VertexOutput,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
) -> vec4<f32> {
    var surface_color = input.color;
    var fragment_color = vec3<f32>(0.0);
    if input.is_terrain > 0.5 {
        let visual_weather = visual_cloud_weather(surface_cloud_weather);
        let ambient_sky = regional_canopy_conditions(
            atmosphere,
            surface_cloud_weather,
        );
        let terrain = procedural_terrain_sample_with_climate_from_circle(
            input.world_position_m,
            surface_circle_position,
            atmosphere.climate,
        );
        let water_height_m = tide_height_m();
        let is_land = terrain.elevation_m >= water_height_m;
        var land_surface = 0.0;
        if is_land && terrain.water_ice_amount == 0.0 {
            land_surface = material_fractal_value(
                vec2<u32>(0x6a09e667u, 0xbb67ae85u),
                input.world_position_m,
                96.0,
                4u,
            );
        }
        var surface: TerrainSurface;
        if terrain.water_ice_amount > 0.0 {
            surface = sea_ice_surface(
                input.world_position_m,
                terrain.water_ice_amount,
                terrain.bedrock_elevation_m,
            );
        } else if is_land && terrain.ice_amount >= 1.0 {
            surface = ice_surface(input.world_position_m, land_surface);
        } else {
            surface = terrain_surface(
                input.world_position_m,
                terrain.cluster,
                terrain.elevation_m,
                terrain.gradient,
                terrain.material_elevation_m,
                terrain.material_gradient,
                terrain.coastal_rock_amount,
                terrain.coastal_outcrop_amount,
                terrain.coastal_sediment_amount,
                water_height_m,
                land_surface,
                atmosphere,
            );
        }
        var surface_light_grade = vec2<f32>(1.0);
        if is_land {
            if terrain.water_ice_amount == 0.0
                && terrain.ice_amount > 0.0
                && terrain.ice_amount < 1.0
            {
                let ice = ice_surface(input.world_position_m, land_surface);
                surface.color = mix(surface.color, ice.color, terrain.ice_amount);
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
        fragment_color = fragment_surface_color(
            surface_color,
            input.world_position_m,
            max(terrain.elevation_m, water_height_m),
            terrain_shadow_transmittance,
            surface_light_grade,
            ambient_sky,
            visual_weather,
        );
        if layer_visibility.x > 0.0 {
            let low_height_m = LOW_CLOUD_BASE_HEIGHT_M;
            let upper_height_m = WEATHER_CANOPY_HEIGHT_M;
            let low_cloud = procedural_low_cloud_sample(
                input.world_position_m,
                visual_weather,
            );
            var upper_transmittance = vec2<f32>(1.0);
            if low_cloud.body_amount > 0.01 && layer_visibility.w > 0.0 {
                upper_transmittance = upper_cloud_transmittance_at_low_cloud(
                    input.world_position_m,
                    low_height_m,
                    upper_height_m,
                    ambient_sky,
                );
            }
            let clouded_fragment_color = cloud_color(
                fragment_color,
                low_cloud,
                upper_transmittance,
                fragment_light_visibility(),
                ambient_sky,
            );
            fragment_color += fragment_layer_visibility().x
                * (clouded_fragment_color - fragment_color);
        }
    }
    return vec4<f32>(fragment_color, 1.0);
}
