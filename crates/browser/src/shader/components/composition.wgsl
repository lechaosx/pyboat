fn fragment_surface_color(
    surface_color: vec3<f32>,
    direct_light: CelestialReceiverLight,
    terrain_shadow_transmittance: vec2<f32>,
    landform_grade: vec3<f32>,
    ambient_light: AtmosphericAmbientLight,
    bounce_conditions: GroundBounceConditions,
) -> vec3<f32> {
    let contribution = fragment_light_visibility();
    let cloud_transmittance = mix(
        vec2<f32>(1.0),
        celestial_cloud_transmittance_at(direct_light),
        fragment_layer_visibility().y,
    );
    let sun_transmittance = cloud_shadow_transmittance(
        cloud_transmittance.x,
        direct_light.sun_direction,
        direct_light.horizon_z,
        1.0,
    );
    let moon_transmittance = cloud_shadow_transmittance(
        cloud_transmittance.y,
        direct_light.moon_direction,
        direct_light.horizon_z,
        moon_cloud_shadow_weight(direct_light, ambient_light),
    );
    let split = ambient_light.split;
    let ambient = split.diffuse_share * contribution.z * ambient_light.clear_sky
        + contribution.w * (ambient_light.canopy + ambient_light.canopy_scattered);
    let sun_light = split.direct.x * sun_direct_light_color();
    let moon_light = split.direct.y * moon_direct_light_color(direct_light);
    let beam_transmittance = bounce_conditions.cumulus_beam_transmittance
        * ambient_light.beam_transmittance;
    let first_pass_light = ambient
        + contribution.x * beam_transmittance.x * sun_light
        + contribution.y * beam_transmittance.y * moon_light;
    let bounce = ground_bounce_light(
        first_pass_light,
        bounce_conditions.sky_reflectance.x + bounce_conditions.sky_reflectance.y,
        bounce_conditions.albedo,
    );
    let light = landform_grade.x * (ambient + bounce)
        + contribution.x
            * terrain_shadow_transmittance.x
            * sun_transmittance
            * landform_grade.y
            * sun_light
        + contribution.y
            * terrain_shadow_transmittance.y
            * moon_transmittance
            * landform_grade.z
            * moon_light;
    return linear_material_color(surface_color) * light;
}

// The composed surface at a rendered position: its height, the relief
// gradient that lights it, and whether it lies above the water.
struct SceneSurface {
    elevation_m: f32,
    gradient: vec2<f32>,
    is_land: bool,
    surface: TerrainSurface,
};

fn direct_sea_ice_detail(position_m: vec2<f32>, terrain: TerrainSample) -> SeaIceDetail {
    var detail = SeaIceDetail(0.0, 0.0, vec2<f32>(0.0));
    if terrain.water_ice_amount > 0.0 || EVALUATE_ALL_BRANCHES {
        detail = sea_ice_detail_at(position_m, ice_micro_relief_sample(position_m));
    }
    return detail;
}

fn composed_scene_surface(
    position_m: vec2<f32>,
    terrain: TerrainSample,
    sea_ice_detail: SeaIceDetail,
    water_height_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> SceneSurface {
    let is_land = terrain.elevation_m >= water_height_m;
    let bare_land = is_land && terrain.water_ice_amount == 0.0;
    var land_surface = 0.0;
    if bare_land || EVALUATE_ALL_BRANCHES {
        land_surface = select(0.0, material_fractal_value(
            LAND_SURFACE_NOISE_FIELD,
            position_m,
            96.0,
            4u,
        ), bare_land);
    }
    let sea_ice = terrain.water_ice_amount > 0.0;
    let land_ice = !sea_ice && is_land && terrain.ice_amount >= 1.0;
    var surface: TerrainSurface;
    if sea_ice || EVALUATE_ALL_BRANCHES {
        surface = sea_ice_surface(
            position_m,
            terrain.water_ice_amount,
            terrain.bedrock_elevation_m,
            sea_ice_detail,
        );
    }
    if land_ice || EVALUATE_ALL_BRANCHES {
        surface = select_terrain_surface(
            surface,
            ice_surface(position_m, land_surface),
            land_ice,
        );
    }
    if !(sea_ice || land_ice) || EVALUATE_ALL_BRANCHES {
        surface = select_terrain_surface(surface, terrain_surface(
            position_m,
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
    let partial_land_ice = bare_land
        && terrain.ice_amount > 0.0
        && terrain.ice_amount < 1.0;
    if partial_land_ice || EVALUATE_ALL_BRANCHES {
        let ice = ice_surface(position_m, land_surface);
        surface = select_terrain_surface(surface, TerrainSurface(
            mix(surface.color, ice.color, terrain.ice_amount),
            mix(surface.detail_gradient, ice.detail_gradient, terrain.ice_amount),
            mix(surface.roughness, ice.roughness, terrain.ice_amount),
        ), partial_land_ice);
    }
    return SceneSurface(terrain.elevation_m, terrain.gradient, is_land, surface);
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
        let water_height_m = tide_height_m();
        let scene_surface = scene_surface_at(
            input.world_position_m,
            surface_circle_position,
            water_height_m,
            atmosphere,
        );
        let is_land = scene_surface.is_land;
        var surface = scene_surface.surface;
        let surface_height_m = max(scene_surface.elevation_m, water_height_m);
        let surface_direct_light = celestial_receiver_light_at_height(surface_height_m);
        let ambient_light = calculate_atmospheric_ambient_light(
            ambient_sky,
            surface_direct_light,
        );
        var surface_light_grade = vec3<f32>(1.0);
        if is_land {
            surface_light_grade = landform_light_grade(
                3.0 * scene_surface.gradient + surface.detail_gradient,
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
        // The split sets direct light on level ground; relief grades it from there.
        let level_grade = landform_light_grade(
            vec2<f32>(0.0),
            surface.roughness,
            surface_direct_light,
        );
        surface_light_grade = vec3<f32>(
            surface_light_grade.x,
            surface_light_grade.yz / level_grade.yz,
        );
        surface_color = surface.color;
        let terrain_shadow_transmittance = landform_shadow_transmittance(surface_direct_light);
        fragment_color = fragment_surface_color(
            surface_color,
            surface_direct_light,
            terrain_shadow_transmittance,
            surface_light_grade,
            ambient_light,
            sampled_ground_bounce_conditions(input.world_position_m),
        );
    }
    return vec4<f32>(fragment_color, 1.0);
}

// Broadband albedos under diffuse light: open ocean 0.07 (Jin et al. 2004,
// Geophys. Res. Lett. 31), vegetated and bare land 0.17, snow 0.8, and
// snow-covered sea ice 0.75.
const REGIONAL_ALBEDOS: vec4<f32> = vec4<f32>(0.07, 0.17, 0.80, 0.75);

// What ground-sky exchange of light needs from a region: the ground's albedo,
// the share of each beam the cumulus field passes, and the sky's reflectance
// from below by its open part, cumulus included, and its canopy.
struct GroundBounceConditions {
    albedo: f32,
    cumulus_beam_transmittance: f32,
    sky_reflectance: vec2<f32>,
};

fn ground_bounce_conditions_from_field(field: vec4<f32>) -> GroundBounceConditions {
    return GroundBounceConditions(field.x, field.y, field.zw);
}

// The conditions at a position, packed as `ground_bounce_conditions_from_field`
// reads them, from the weather that sets the cumulus shadows there.
fn ground_bounce_field(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
    weather: CloudWeatherSample,
) -> vec4<f32> {
    let canopy = regional_canopy_conditions(weather);
    let cumulus = cumulus_drivers(weather);
    // Cumulus optical depth as `cumulus_shadow` in clouds.wgsl takes it; keep
    // the two synchronized. At four or more, cumulus pass under 2% of the beam
    // they cover.
    let cumulus_reflectance = two_stream_reflectance(
        (1.0 - CLOUD_ASYMMETRY) * mix(4.0, 25.0, cumulus.y),
    );
    return vec4<f32>(
        regional_ground_albedo(
            position_m,
            periodic_noise_circle_position(position_m),
            atmosphere,
            weather.rain,
        ),
        1.0 - cumulus.x,
        (1.0 - canopy.sky_fraction) * mix(
            clear_sky_reflectance_from_below(weather.regional),
            cumulus_reflectance,
            cumulus.x,
        ),
        canopy.sky_fraction * two_stream_reflectance(
            (1.0 - CLOUD_ASYMMETRY) * max(canopy.mean_optical_thickness, 0.0),
        ),
    );
}

// Ground albedo over the kilometres a cloud base overhead sees, from causes
// that vary over that distance. Land takes the island-cluster relief, where
// 0.78 cluster + 0.22 island exceeds 0.30 for the island noise across its
// range; ice and snow take sea level's climate and weather.
fn regional_ground_albedo(
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    atmosphere: SurfaceAtmosphereSample,
    rain: f32,
) -> f32 {
    let climate = atmosphere.climate;
    let cluster = terrain_broad_noise(TERRAIN_CLUSTER_NOISE, position_m, circle_position).value;
    let land = smoothstep(0.10, 0.67, cluster);
    let sea_ice = 1.0 - smoothstep(
        -2.0,
        0.0,
        water_temperature_c_from_climate(climate)
            + maritime_temperature_scale_c(climate) * cluster,
    );
    let land_ice = 1.0 - smoothstep(-12.0, -8.0, climate.annual_mean_air_temperature_c);
    var snow = 0.0;
    if land > 0.0 || EVALUATE_ALL_BRANCHES {
        snow = seasonal_snow_cover_from_fields(
            0.0,
            atmosphere.temperature_anomaly.value,
            atmosphere.water_vapor_g_kg,
            climate,
            rain,
        );
    }
    return mix(
        mix(REGIONAL_ALBEDOS.x, REGIONAL_ALBEDOS.w, sea_ice),
        mix(REGIONAL_ALBEDOS.y, REGIONAL_ALBEDOS.z, max(land_ice, snow)),
        land,
    );
}
