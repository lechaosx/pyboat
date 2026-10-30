// The light reaching a surface: `surface_light` grades diffuse light, then
// gives each beam's direct light on the surface. `contribution` and
// `cloud_shadow_visibility` gate light contributions and cloud shadows out
// of it.
fn fragment_surface_light(
    direct_light: CelestialReceiverLight,
    terrain_shadow_transmittance: vec2<f32>,
    surface_light: vec3<f32>,
    ambient_light: AtmosphericAmbientLight,
    bounce_conditions: GroundBounceConditions,
    contribution: vec4<f32>,
    cloud_shadow_visibility: f32,
) -> vec3<f32> {
    let beam_transmittance = bounce_conditions.cumulus_beam_transmittance
        * ambient_light.beam_transmittance;
    let cloud_transmittance = mix(
        vec2<f32>(1.0),
        celestial_cloud_transmittance_at(direct_light),
        cloud_shadow_visibility,
    );
    let regional_cloud_transmittance = mix(
        vec2<f32>(1.0),
        beam_transmittance,
        cloud_shadow_visibility * tuning.cloud_shadows.x,
    );
    let sun_transmittance = cloud_shadow_transmittance(
        cloud_transmittance.x,
        regional_cloud_transmittance.x,
        direct_light.sun_direction,
        1.0,
    );
    let moon_transmittance = cloud_shadow_transmittance(
        cloud_transmittance.y,
        regional_cloud_transmittance.y,
        direct_light.moon_direction,
        moon_cloud_shadow_weight(direct_light, ambient_light),
    );
    let split = ambient_light.split;
    let ambient = split.diffuse_share * contribution.z * ambient_light.clear_sky
        + contribution.w * (ambient_light.canopy + ambient_light.canopy_scattered);
    let sun_color = sun_direct_light_color(direct_light);
    let moon_color = moon_direct_light_color(direct_light);
    let first_pass_light = ambient
        + contribution.x * beam_transmittance.x * split.direct.x * sun_color
        + contribution.y * beam_transmittance.y * split.direct.y * moon_color;
    let bounce = ground_bounce_light(
        first_pass_light,
        bounce_conditions.sky_reflectance.x + bounce_conditions.sky_reflectance.y,
        bounce_conditions.albedo,
    );
    return surface_light.x * (ambient + bounce)
        + contribution.x
            * terrain_shadow_transmittance.x
            * sun_transmittance
            * surface_light.y
            * sun_color
        + contribution.y
            * terrain_shadow_transmittance.y
            * moon_transmittance
            * surface_light.z
            * moon_color;
}

// The composed surface at a rendered position: its height, the relief
// gradient the pixel resolves, as it lights it, and whether it lies above the
// water. Its surface's unresolved slope includes the terrain's, as lit.
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
            mix(surface.unresolved_slope, ice.unresolved_slope, terrain.ice_amount),
        ), partial_land_ice);
    }
    surface.unresolved_slope = lit_unresolved_slope(terrain, surface.unresolved_slope);
    return SceneSurface(terrain.elevation_m, lit_terrain_gradient(terrain), is_land, surface);
}

// Landform relief is exaggerated threefold, a presentation compromise that
// stands in for the depth cues a flat top-down view lacks at landform scale.
// The metre-scale micro relief, already about one in slope at true scale,
// stays true.
const RELIEF_EXAGGERATION: f32 = 3.0;

// The terrain slope a pixel resolves, as it lights the surface.
fn lit_terrain_gradient(terrain: TerrainSample) -> vec2<f32> {
    return RELIEF_EXAGGERATION
        * (terrain.gradient - terrain.unresolved_gradient - terrain.micro_relief_gradient)
        + terrain.micro_relief_gradient;
}

// Root-mean-square unresolved slope that lights a surface: the terrain's, its
// landform relief exaggerated as `lit_terrain_gradient` takes it, and the
// surface detail's, independent of it.
fn lit_unresolved_slope(terrain: TerrainSample, surface_slope: f32) -> f32 {
    return sqrt(
        RELIEF_EXAGGERATION * RELIEF_EXAGGERATION
            * (terrain.unresolved_slope_variance - terrain.micro_relief_slope_variance)
            + terrain.micro_relief_slope_variance
            + surface_slope * surface_slope,
    );
}

// `landform_light_grade` of a land surface.
fn land_relief_light(
    scene_surface: SceneSurface,
    receiver: CelestialReceiverLight,
    normal_light: vec2<f32>,
) -> vec3<f32> {
    return landform_light_grade(
        scene_surface.gradient + scene_surface.surface.detail_gradient,
        scene_surface.surface.unresolved_slope,
        receiver,
        normal_light,
    );
}

// A rendered surface and the light reaching it.
struct LitSceneSurface {
    surface: TerrainSurface,
    light: vec3<f32>,
};

// `specular_scale`, `contribution`, and `cloud_shadow_visibility` as
// `water_light_grade` and `fragment_surface_light` take them.
fn lit_scene_surface(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
    specular_scale: f32,
    contribution: vec4<f32>,
    cloud_shadow_visibility: f32,
) -> LitSceneSurface {
    let ambient_sky = regional_canopy_conditions(surface_cloud_weather);
    let water_height_m = tide_height_m();
    let scene_surface = scene_surface_at(
        position_m,
        surface_circle_position,
        water_height_m,
        atmosphere,
    );
    let is_land = scene_surface.is_land;
    let surface = scene_surface.surface;
    let surface_height_m = max(scene_surface.elevation_m, water_height_m);
    let surface_direct_light = celestial_receiver_light_at_height(surface_height_m);
    let ambient_light = calculate_atmospheric_ambient_light(
        ambient_sky,
        surface_direct_light,
    );
    let split = ambient_light.split;
    var surface_light = vec3<f32>(1.0, split.direct);
    if is_land {
        surface_light = land_relief_light(scene_surface, surface_direct_light, split.normal);
    } else {
        // Water keeps its own relief grade of the split's level light.
        let grade = water_light_grade(
            surface.detail_gradient,
            surface.roughness,
            surface_direct_light,
            specular_scale,
        );
        let level_grade = water_light_grade(
            vec2<f32>(0.0),
            surface.roughness,
            surface_direct_light,
            specular_scale,
        );
        surface_light = vec3<f32>(grade.x, split.direct * grade.yz / level_grade.yz);
    }
    let terrain_shadow_transmittance = landform_shadow_transmittance(surface_direct_light);
    return LitSceneSurface(surface, fragment_surface_light(
        surface_direct_light,
        terrain_shadow_transmittance,
        surface_light,
        ambient_light,
        sampled_ground_bounce_conditions(position_m),
        contribution,
        cloud_shadow_visibility,
    ));
}

fn final_scene_color(
    input: VertexOutput,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
) -> vec4<f32> {
    var fragment_color = vec3<f32>(0.0);
    if input.is_terrain > 0.5 {
        let lit = lit_scene_surface(
            input.world_position_m,
            atmosphere,
            surface_circle_position,
            1.0,
            fragment_light_visibility(),
            fragment_layer_visibility().y,
        );
        fragment_color = linear_material_color(lit.surface.color) * lit.light;
    }
    return vec4<f32>(fragment_color, 1.0);
}

// Luminance of the light reaching the surface at a rendered position, in
// scene light: what the eye adapts to. It leaves out the surface's
// reflectance, glint included, and stays downstream of the lab's light and
// cloud shadow gates, as exposure does.
fn scene_illumination(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
) -> f32 {
    let lit = lit_scene_surface(
        position_m,
        atmosphere,
        surface_circle_position,
        0.0,
        vec4<f32>(1.0),
        1.0,
    );
    return dot(lit.light, vec3<f32>(0.2126, 0.7152, 0.0722));
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
