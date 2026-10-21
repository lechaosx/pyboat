fn fragment_surface_color(
    surface_color: vec3<f32>,
    direct_light: CelestialReceiverLight,
    terrain_shadow_transmittance: vec2<f32>,
    landform_grade: vec3<f32>,
    ambient_light: AtmosphericAmbientLight,
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
        moon_cloud_shadow_weight(direct_light),
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
        surface_color = surface.color;
        let terrain_shadow_transmittance = landform_shadow_transmittance(surface_direct_light);
        fragment_color = fragment_surface_color(
            surface_color,
            surface_direct_light,
            terrain_shadow_transmittance,
            surface_light_grade,
            ambient_light,
        );
    }
    return vec4<f32>(fragment_color, 1.0);
}
