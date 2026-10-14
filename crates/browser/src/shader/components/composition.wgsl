struct LowCloudSunLighting {
    surface_amount: f32,
    elevated_amount: f32,
    total_amount: f32,
};

fn low_cloud_sun_lighting(
    low_cloud_light: CelestialReceiverLight,
) -> LowCloudSunLighting {
    let visibility = direct_light_visibility().x;
    let total_amount = visibility * low_cloud_light.sun_amount;
    let surface_amount = min(
        total_amount,
        visibility * sun_amount_at_horizon_with_eclipse_for_direction(
            celestial_horizon_z_at_height(0.0),
            low_cloud_light.solar_eclipse_fraction,
            low_cloud_light.sun_direction,
        ),
    );
    let elevated_amount = total_amount - surface_amount;
    return LowCloudSunLighting(
        surface_amount,
        elevated_amount,
        total_amount,
    );
}

fn cloud_color(
    surface_color: vec3<f32>,
    cloud_sample: LowCloudSample,
    upper_transmittance: vec2<f32>,
    contribution_visibility: vec4<f32>,
    ambient_light: AtmosphericAmbientLight,
    low_cloud_weather: CloudWeatherSample,
    low_cloud_light: CelestialReceiverLight,
) -> vec3<f32> {
    let cloud_amount = cloud_sample.body_amount;
    let limited_gradient = cloud_sample.gradient
        / (1.0 + length(cloud_sample.gradient) / 0.25);
    let cloud_normal = normalize(vec3<f32>(
        -limited_gradient,
        1.0,
    ));
    let sun_facing = max(dot(cloud_normal, low_cloud_light.sun_direction), 0.0);
    let moon_facing = max(dot(cloud_normal, low_cloud_light.moon_direction), 0.0);
    let low_cloud_sun = low_cloud_sun_lighting(low_cloud_light);
    let sun_atmospheric_transmittance = celestial_atmospheric_transmittance(
        low_cloud_light, low_cloud_light.sun_direction,
    );
    let moon_atmospheric_transmittance = celestial_atmospheric_transmittance(
        low_cloud_light, low_cloud_light.moon_direction,
    );
    let sun_direct_shape = low_cloud_sun.surface_amount * sun_facing;
    let moon_direct_shape = low_cloud_light.moon_amount * moon_facing;
    let elevated_sun_scatter = low_cloud_sun.elevated_amount;
    let sunlight = low_cloud_sun.total_amount * upper_transmittance.x;
    let moonlight = low_cloud_light.moon_amount * upper_transmittance.y;
    let depth = cloud_sample.optical_depth / max(cloud_amount, 0.001);
    let edge_response = (1.0 - smoothstep(0.30, 0.78, cloud_amount))
        * smoothstep(0.10, 0.55, cloud_amount)
        * tuning.cloud_lighting.x;
    let depth_shadow = smoothstep(0.45, 1.35, cloud_sample.optical_depth)
        * tuning.cloud_lighting.y;
    let cloud_material = mix(
        vec3<f32>(0.68, 0.72, 0.74),
        vec3<f32>(0.96, 0.97, 0.96),
        0.30 + 0.36 * min(depth / 1.25, 1.0),
    );
    let storm_amount = clamp(
        0.06 * cloud_amount * tuning.cloud_rendering.z,
        0.0,
        1.0,
    );
    let neutral_material = mix(
        cloud_material,
        vec3<f32>(0.12, 0.15, 0.17),
        storm_amount,
    );
    let neutral_cloud = linear_material_color(max(
        vec3<f32>(0.16, 0.18, 0.20),
        0.70 * neutral_material - depth_shadow,
    ));
    let direct_whitening = 0.70 * 0.25
        * (1.0 - storm_amount)
        * (linear_material_color(vec3<f32>(0.96, 0.97, 0.96))
            - linear_material_color(vec3<f32>(0.68, 0.72, 0.74)));
    let sun_cloud = neutral_cloud
        + direct_whitening * sun_direct_shape
        + linear_material_color(neutral_material)
            * sunlight
            * mix(0.12, 0.36, sun_direct_shape)
        + sunlight * edge_response;
    let moon_cloud = neutral_cloud
        + direct_whitening * moon_direct_shape
        + linear_material_color(neutral_material)
            * moonlight
            * mix(0.12, 0.36, moon_direct_shape)
        + moonlight * edge_response;
    let cloud_horizon_scattering = calculate_local_horizon_scattering_at(low_cloud_weather);
    let direct_sun_color = sun_light_color_for_direction(
        cloud_horizon_scattering,
        low_cloud_light.sun_direction,
    );
    let elevated_sun_light = contribution_visibility.x
        * upper_transmittance.x
        * 0.26
        * sqrt(elevated_sun_scatter)
        * sun_atmospheric_transmittance
        * direct_sun_color
        * (0.12 + 0.88 * edge_response);
    let ambient_color = ambient_contribution_share(low_cloud_light) * (
        contribution_visibility.z * atmospheric_ambient_color_with_visibility(
            ambient_light,
            vec2<f32>(1.0, 0.0),
        )
        + contribution_visibility.w * atmospheric_ambient_color_with_visibility(
            ambient_light,
            vec2<f32>(0.0, 1.0),
        )
    );
    let sun_color = contribution_visibility.x * upper_transmittance.x
        * 0.38
        * sqrt(clamp(low_cloud_sun.surface_amount, 0.0, 1.0))
        * sun_atmospheric_transmittance
        * direct_sun_color
        * mix(0.25, 1.0, sun_facing)
        + elevated_sun_light;
    let moon_color = contribution_visibility.y
        * upper_transmittance.y
        * 0.16
        * sqrt(clamp(low_cloud_light.moon_amount, 0.0, 1.0))
        * moon_atmospheric_transmittance
        * calculate_moon_light_color_for_direction(
            cloud_horizon_scattering,
            low_cloud_light.moon_direction,
        )
        * mix(0.25, 1.0, moon_facing);
    let cloud = (
        neutral_cloud * ambient_color
        + sun_cloud * sun_color
        + moon_cloud * moon_color
    );
    let body = smoothstep(0.08, 0.18, cloud_amount);
    let optical_depth = tuning.cloud_rendering.x * cloud_sample.optical_depth;
    let opacity = low_cloud_material_color_visibility()
        * tuning.cloud_rendering.y * body * (1.0 - exp(-1.35 * optical_depth));
    return mix(surface_color, cloud, opacity);
}

fn upper_cloud_transmittance_at_low_cloud_for_source(
    low_cloud_light: CelestialReceiverLight,
    source: CelestialSourceLight,
) -> f32 {
    let lit = source.amount > 0.0;
    if !lit && !EVALUATE_ALL_BRANCHES {
        return 1.0;
    }
    let query = cloud_shadow_query(
        low_cloud_light,
        source,
        false,
        true,
    );
    var transmittance = 1.0;
    if !query.resolved || EVALUATE_ALL_BRANCHES {
        let representative_light = cloud_shadow_query_direction(query, 0u);
        let projected_position_m = cloud_shadow_query_projection(
            query,
            0u,
        ).upper_position_m;
        let upper_optical_depth = tuning.cloud_shadows.y
            * sampled_upper_cloud_optical_depth_at(projected_position_m);
        transmittance = mix(
            1.0,
            cloud_layer_transmittance(
                upper_optical_depth,
                representative_light,
                query.receiver.horizon_z,
            ),
            layer_visibility.w,
        );
    }
    if query.resolved || EVALUATE_ALL_BRANCHES {
        transmittance = select(
            transmittance,
            resolved_upper_cloud_transmittance(query),
            query.resolved,
        );
    }
    return select(1.0, transmittance, lit);
}

fn resolved_upper_cloud_transmittance(query: CloudShadowQuery) -> f32 {
    let lower_optical_depth = tuning.cloud_shadows.y
        * sampled_upper_cloud_optical_depth_at(query.lower_projection.upper_position_m);
    let upper_optical_depth = tuning.cloud_shadows.y
        * sampled_upper_cloud_optical_depth_at(query.upper_projection.upper_position_m);
    let side_optical_depth = tuning.cloud_shadows.y
        * sampled_upper_cloud_optical_depth_at(query.side_projection.upper_position_m);
    let transmittance = (1.0 / 3.0) * (
        cloud_layer_transmittance(
            lower_optical_depth,
            query.directions.lower,
            query.receiver.horizon_z,
        )
            + cloud_layer_transmittance(
                upper_optical_depth,
                query.directions.upper,
                query.receiver.horizon_z,
            )
            + cloud_layer_transmittance(
                side_optical_depth,
                query.directions.side,
                query.receiver.horizon_z,
            )
    );
    return mix(1.0, transmittance, layer_visibility.w);
}

fn upper_cloud_transmittance_at_low_cloud(
    low_cloud_light: CelestialReceiverLight,
) -> vec2<f32> {
    var transmittance = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let source = celestial_source_light(low_cloud_light, source_index);
        transmittance[source_index] = upper_cloud_transmittance_at_low_cloud_for_source(
            low_cloud_light,
            source,
        );
    }
    return transmittance;
}

fn fragment_surface_color(
    surface_color: vec3<f32>,
    direct_light: CelestialReceiverLight,
    terrain_shadow_transmittance: vec2<f32>,
    landform_grade: vec3<f32>,
    ambient_light: AtmosphericAmbientLight,
) -> vec3<f32> {
    let layer = fragment_layer_visibility();
    let contribution = fragment_light_visibility();
    let depths = celestial_cloud_shadow_depths_at(direct_light);
    let sun_transmittance = cloud_disc_shadow_transmittance(
        layer.y * depths.sun.low + layer.z * depths.sun.upper,
        direct_light.sun_direction,
    );
    let moon_transmittance = cloud_disc_shadow_transmittance(
        layer.y * depths.moon.low + layer.z * depths.moon.upper,
        direct_light.moon_direction,
    );
    let ambient = ambient_contribution_share(direct_light) * (
        contribution.z * atmospheric_ambient_color_with_visibility(
            ambient_light,
            vec2<f32>(1.0, 0.0),
        )
        + contribution.w * atmospheric_ambient_color_with_visibility(
            ambient_light,
            vec2<f32>(0.0, 1.0),
        )
    );
    let light = landform_grade.x * ambient
        + contribution.x
            * terrain_shadow_transmittance.x
            * sun_transmittance
            * landform_grade.y
            * sun_direct_contribution_color_for_receiver(direct_light)
        + contribution.y
            * terrain_shadow_transmittance.y
            * moon_transmittance
            * landform_grade.z
            * moon_direct_contribution_color_for_receiver(direct_light);
    return linear_material_color(surface_color) * light;
}

fn final_scene_color(
    input: VertexOutput,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
) -> vec4<f32> {
    var surface_color = input.color;
    var fragment_color = vec3<f32>(0.0);
    if input.is_terrain > 0.5 {
        let ambient_sky = regional_canopy_conditions(surface_cloud_weather);
        let terrain = procedural_terrain_sample_with_climate_from_circle(
            input.world_position_m,
            surface_circle_position,
            atmosphere.climate,
        );
        let water_height_m = tide_height_m();
        let is_land = terrain.elevation_m >= water_height_m;
        let bare_land = is_land && terrain.water_ice_amount == 0.0;
        var land_surface = 0.0;
        if bare_land || EVALUATE_ALL_BRANCHES {
            land_surface = select(0.0, material_fractal_value(
                vec2<u32>(0x6a09e667u, 0xbb67ae85u),
                input.world_position_m,
                96.0,
                4u,
            ), bare_land);
        }
        let sea_ice = terrain.water_ice_amount > 0.0;
        let land_ice = !sea_ice && is_land && terrain.ice_amount >= 1.0;
        var surface: TerrainSurface;
        if sea_ice || EVALUATE_ALL_BRANCHES {
            surface = sea_ice_surface(
                input.world_position_m,
                terrain.water_ice_amount,
                terrain.bedrock_elevation_m,
            );
        }
        if land_ice || EVALUATE_ALL_BRANCHES {
            surface = select_terrain_surface(
                surface,
                ice_surface(input.world_position_m, land_surface),
                land_ice,
            );
        }
        if !(sea_ice || land_ice) || EVALUATE_ALL_BRANCHES {
            surface = select_terrain_surface(surface, terrain_surface(
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
            ), !(sea_ice || land_ice));
        }
        let surface_height_m = max(terrain.elevation_m, water_height_m);
        let surface_direct_light = celestial_receiver_light_at_height(surface_height_m);
        let ambient_light = calculate_atmospheric_ambient_light(
            ambient_sky,
            surface_direct_light,
        );
        var surface_light_grade = vec3<f32>(1.0);
        let partial_land_ice = bare_land
            && terrain.ice_amount > 0.0
            && terrain.ice_amount < 1.0;
        if partial_land_ice || EVALUATE_ALL_BRANCHES {
            let ice = ice_surface(input.world_position_m, land_surface);
            surface = select_terrain_surface(surface, TerrainSurface(
                mix(surface.color, ice.color, terrain.ice_amount),
                mix(surface.detail_gradient, ice.detail_gradient, terrain.ice_amount),
                mix(surface.roughness, ice.roughness, terrain.ice_amount),
            ), partial_land_ice);
        }
        if is_land {
            surface_light_grade = landform_light_grade(
                3.0 * terrain.gradient + surface.detail_gradient,
                surface.roughness,
                surface_direct_light,
            );
        } else {
            surface_light_grade = landform_light_grade(
                surface.detail_gradient,
                surface.roughness,
                surface_direct_light,
            );
        }
        surface_color = surface.color;
        let terrain_shadow_transmittance = landform_shadow_transmittance(surface_direct_light);
        fragment_color = fragment_surface_color(
            surface_color,
            surface_direct_light,
            terrain_shadow_transmittance,
            surface_light_grade,
            ambient_light,
        );
        if layer_visibility.x > 0.0 {
            let low_height_m = LOW_CLOUD_BASE_HEIGHT_M;
            let low_cloud_position_m = visible_low_cloud_position_m(input.world_position_m);
            let low_cloud_weather = cloud_weather_from_environment_fields(low_cloud_position_m);
            let low_cloud = procedural_low_cloud_sample(
                low_cloud_position_m,
                low_cloud_weather,
            );
            let low_cloud_visible = low_cloud.body_amount > 0.08
                && low_cloud.optical_depth > 0.0
                && tuning.cloud_rendering.x > 0.0
                && tuning.cloud_rendering.y > 0.0;
            if low_cloud_visible || EVALUATE_ALL_BRANCHES {
                let low_cloud_light = celestial_receiver_light_at_position_and_height(
                    low_cloud_position_m,
                    low_height_m,
                );
                var upper_transmittance = vec2<f32>(1.0);
                if layer_visibility.w > 0.0 {
                    upper_transmittance = upper_cloud_transmittance_at_low_cloud(low_cloud_light);
                }
                let clouded_fragment_color = cloud_color(
                    fragment_color,
                    low_cloud,
                    upper_transmittance,
                    fragment_light_visibility(),
                    ambient_light,
                    low_cloud_weather,
                    low_cloud_light,
                );
                fragment_color = select(
                    fragment_color,
                    fragment_color + layer_visibility.x
                        * (clouded_fragment_color - fragment_color),
                    low_cloud_visible,
                );
            }
        }
    }
    return vec4<f32>(fragment_color, 1.0);
}
