fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    return broad_landform_elevation_m(position_m);
}

fn sampled_landform_horizon(
    position_m: vec2<f32>,
    light_direction: vec3<f32>,
    angular_radius_rad: f32,
    source_index: u32,
) -> vec2<f32> {
    return landform_horizon(position_m, light_direction, angular_radius_rad);
}

fn sampled_cloud_transmittance_at(plane_position_m: vec2<f32>, source_index: u32) -> f32 {
    let rays = cloud_shadow_rays(plane_position_m, source_index);
    if rays.amount <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 1.0;
    }
    return cloud_shadow_field_transmittance(direct_cloud_shadow_texel(plane_position_m, rays));
}

// Cumulus drivers with their cover's gradient across one cloud weather grid
// spacing at the shortest.
fn direct_cumulus_drivers(position_m: vec2<f32>) -> CumulusDrivers {
    let step_m = 64.0;
    let drivers = cumulus_drivers(cloud_weather_with_land_at(position_m));
    let east = cumulus_drivers(cloud_weather_with_land_at(position_m + vec2<f32>(step_m, 0.0)));
    let north = cumulus_drivers(cloud_weather_with_land_at(position_m + vec2<f32>(0.0, step_m)));
    return CumulusDrivers(drivers, vec2<f32>(east.x - drivers.x, north.x - drivers.x) / step_m);
}

fn direct_cloud_shadow_texel(plane_position_m: vec2<f32>, rays: CloudShadowRays) -> vec4<f32> {
    let deck_weather = cloud_weather_with_land_at(rays.deck.position_m);
    let guess = cumulus_ray(plane_position_m, rays, cumulus_height_m(deck_weather));
    let cumulus = cumulus_ray(
        plane_position_m,
        rays,
        cumulus_height_m(cloud_weather_with_land_at(guess.position_m)),
    );
    return cloud_shadow_texel(
        plane_position_m,
        rays,
        cumulus,
        direct_cumulus_drivers(cumulus.position_m),
        deck_drivers(deck_weather),
        0.0,
    );
}
