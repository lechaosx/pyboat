use std::{env, fmt::Write as _, fs, path::PathBuf};

#[path = "src/diagnostics.rs"]
mod diagnostics;

const SCENE_COMPONENTS: &[&str] = &[
    "src/shader/components/scene-state.wgsl",
    "src/shader/components/vertex.wgsl",
    "src/shader/components/display.wgsl",
    "src/shader/components/adapting-field.wgsl",
    "src/shader/components/noise.wgsl",
    "src/shader/components/environment.wgsl",
    "src/shader/components/environment-fields.wgsl",
    "src/shader/components/clouds.wgsl",
    "src/shader/components/lighting.wgsl",
    "src/shader/components/terrain.wgsl",
    "src/shader/components/water-materials.wgsl",
    "src/shader/components/composition.wgsl",
];

fn main() {
    let scene = read_components(SCENE_COMPONENTS);
    let occluder_fields = read_component("src/shader/components/occluder-fields.wgsl");
    let direct_occluder_fields =
        read_component("src/shader/components/direct-occluder-fields.wgsl");
    let occluder_generate = read_component("src/shader/components/occluder-field-generate.wgsl");
    let landform_horizon_generate =
        read_component("src/shader/components/landform-horizon-generate.wgsl");
    let occluder_inspection_disabled =
        read_component("src/shader/components/occluder-field-inspection-disabled.wgsl");
    let terrain_cache = read_component("src/shader/components/terrain-cache.wgsl");
    let direct_terrain_noise = read_component("src/shader/components/direct-terrain-noise.wgsl");
    let baked_surface = read_component("src/shader/components/baked-surface.wgsl");
    let direct_surface = read_component("src/shader/components/direct-surface.wgsl");
    let baked_surface_generate =
        read_component("src/shader/components/baked-surface-generate.wgsl");
    let production_entry = read_component("src/shader/production-entry.wgsl");
    let production = compose(&[
        &scene,
        &occluder_fields,
        &terrain_cache,
        &baked_surface,
        &occluder_inspection_disabled,
        &production_entry,
    ]);
    write_shader(
        &production,
        &["vertex", "fragment", "fragment_illumination"],
        "scene-production.wgsl",
    );
    write_shader(
        &compose(&[&production, &read_component("src/shader/boat-sprite.wgsl")]),
        &[
            "vertex",
            "vertex_boat_sprite",
            "fragment_boat_sprite",
            "fragment_sail_depth",
        ],
        "boat-sprite.wgsl",
    );
    write_shader(
        &compose(&[
            &scene,
            &direct_occluder_fields,
            &direct_terrain_noise,
            &direct_surface,
            &production_entry,
            &read_component("src/shader/adapting-field-blur.wgsl"),
        ]),
        &["fullscreen_vertex", "blur_adapting_field"],
        "adapting-field-blur.wgsl",
    );
    let environment_fields = compose(&[
        &scene,
        &direct_occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &production_entry,
        &read_component("src/shader/components/environment-field-generate.wgsl"),
    ]);
    write_shader(
        &environment_fields,
        &["generate_environment_fields"],
        "environment-fields.wgsl",
    );
    let production_occluders = compose(&[
        &scene,
        &direct_occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &production_entry,
        &read_component("src/shader/production-occluder-field-entry.wgsl"),
        &occluder_generate,
    ]);
    write_shader(
        &production_occluders,
        &["generate_occluder_fields"],
        "occluder-fields.wgsl",
    );
    let production_landform_horizons = compose(&[
        &scene,
        &occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &occluder_inspection_disabled,
        &production_entry,
        &read_component("src/shader/production-occluder-field-entry.wgsl"),
        &landform_horizon_generate,
    ]);
    write_shader(
        &production_landform_horizons,
        &["generate_landform_horizons"],
        "landform-horizons.wgsl",
    );
    let terrain_cache_generate = compose(&[
        &scene,
        &direct_occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &production_entry,
        &read_component("src/shader/components/terrain-cache-generate.wgsl"),
    ]);
    write_shader(
        &terrain_cache_generate,
        &["generate_terrain_cache"],
        "terrain-cache.wgsl",
    );
    let production_baked_surface_generate = compose(&[
        &scene,
        &direct_occluder_fields,
        &terrain_cache,
        &direct_surface,
        &production_entry,
        &read_component("src/shader/production-baked-surface-entry.wgsl"),
        &baked_surface_generate,
    ]);
    write_shader(
        &production_baked_surface_generate,
        &["generate_baked_surface"],
        "baked-surface.wgsl",
    );
    let upscale = read_component("src/shader/upscale.wgsl");
    write_shader(
        &upscale,
        &["fullscreen_vertex", "easu", "rcas"],
        "upscale.wgsl",
    );
    // The served shader must carry the source's licence notice, which naga drops
    // with every other comment.
    let licence: String = upscale
        .lines()
        .take_while(|line| line.starts_with("//"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert!(
        licence.contains("Copyright"),
        "upscale.wgsl must start with its licence"
    );
    let upscale_output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("upscale.wgsl");
    let upscale_shader = fs::read_to_string(&upscale_output).unwrap();
    fs::write(upscale_output, format!("{licence}\n{upscale_shader}")).unwrap();
    let playground_interface = read_component("src/shader/playground-interface.wgsl");
    let startup = read_components(&[
        "src/shader/components/scene-state.wgsl",
        "src/shader/components/vertex.wgsl",
        "src/shader/components/display.wgsl",
        "src/shader/playground-startup-entry.wgsl",
    ]);
    write_shader(
        &startup,
        &["vertex", "fragment_playground_startup"],
        "scene-playground-startup.wgsl",
    );
    let playground = compose(&[
        &scene,
        &occluder_fields,
        &terrain_cache,
        &baked_surface,
        &read_component("src/shader/components/playground-occluder-field-inspection.wgsl"),
        &playground_interface,
        &read_component("src/shader/playground-entry.wgsl"),
    ]);
    write_shader(
        &playground,
        &[
            "vertex",
            "fragment_playground_scene",
            "fragment_playground_neutral_scene",
            "fragment_playground_illumination",
        ],
        "scene-playground.wgsl",
    );
    let playground_direct = compose(&[
        &scene,
        &direct_occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &playground_interface,
        &read_component("src/shader/playground-direct-entry.wgsl"),
    ]);
    write_shader(
        &playground_direct,
        &[
            "vertex",
            "fragment_playground_scene",
            "fragment_playground_illumination",
        ],
        "scene-playground-direct.wgsl",
    );
    let playground_occluders = compose(&[
        &scene,
        &direct_occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &playground_interface,
        &read_component("src/shader/playground-occluder-field-entry.wgsl"),
        &occluder_generate,
    ]);
    write_shader(
        &playground_occluders,
        &["generate_occluder_fields"],
        "occluder-fields-playground.wgsl",
    );
    let playground_landform_horizons = compose(&[
        &scene,
        &occluder_fields,
        &direct_terrain_noise,
        &direct_surface,
        &read_component("src/shader/components/playground-occluder-field-inspection.wgsl"),
        &playground_interface,
        &read_component("src/shader/playground-occluder-field-entry.wgsl"),
        &landform_horizon_generate,
    ]);
    write_shader(
        &playground_landform_horizons,
        &["generate_landform_horizons"],
        "landform-horizons-playground.wgsl",
    );
    let playground_baked_surface_generate = compose(&[
        &scene,
        &direct_occluder_fields,
        &terrain_cache,
        &direct_surface,
        &playground_interface,
        &read_component("src/shader/playground-baked-surface-entry.wgsl"),
        &baked_surface_generate,
    ]);
    write_shader(
        &playground_baked_surface_generate,
        &["generate_baked_surface"],
        "baked-surface-playground.wgsl",
    );
    let diagnostic_common = read_component("src/shader/components/diagnostic-common.wgsl");
    for &name in diagnostics::DIAGNOSTIC_ENTRY_POINTS {
        let mechanism = read_component(diagnostic_mechanism(name));
        let entry = read_component(&format!("src/shader/diagnostics/{name}.wgsl"));
        let diagnostic = compose(&[
            &scene,
            &direct_occluder_fields,
            &direct_terrain_noise,
            &direct_surface,
            &playground_interface,
            &diagnostic_common,
            &mechanism,
            &entry,
        ]);
        write_shader(
            &diagnostic,
            &["vertex", &format!("fragment_{name}")],
            &format!("scene-playground-{name}.wgsl"),
        );
    }
    write_diagnostic_lookup();
}

fn read_components(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| read_component(path))
        .collect::<Vec<_>>()
        .join("\n")
}

fn diagnostic_mechanism(name: &str) -> &'static str {
    if name.starts_with("cloud_") || name.starts_with("lighting_") {
        "src/shader/components/diagnostic-cloud.wgsl"
    } else if name.starts_with("temperature_")
        || name.starts_with("atmosphere_")
        || name.starts_with("surface_air_")
        || name.starts_with("pressure_")
        || name.starts_with("wind_")
    {
        "src/shader/components/diagnostic-atmosphere.wgsl"
    } else if name.starts_with("terrain_") || name.starts_with("material_") {
        "src/shader/components/diagnostic-terrain.wgsl"
    } else if name.starts_with("climate_") {
        "src/shader/components/diagnostic-climate.wgsl"
    } else if name.starts_with("water_") {
        "src/shader/components/diagnostic-water.wgsl"
    } else {
        panic!("diagnostic entry point has no mechanism component: {name}");
    }
}

fn read_component(path: &str) -> String {
    println!("cargo::rerun-if-changed={path}");
    fs::read_to_string(path).unwrap()
}

fn compose(components: &[&str]) -> String {
    components.join("\n")
}

fn write_shader(source: &str, entry_points: &[&str], output_name: &str) {
    let module = parse_shader(source);
    write_selected_shader(module, entry_points, output_name);
}

fn parse_shader(source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    module
}

fn write_selected_shader(mut module: naga::Module, entry_points: &[&str], output_name: &str) {
    module
        .entry_points
        .retain(|entry| entry_points.contains(&entry.name.as_str()));
    naga::compact::compact(&mut module, naga::compact::KeepUnused::No);
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let source =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .unwrap();
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(output_name);
    fs::write(output, source).unwrap();
}

fn write_diagnostic_lookup() {
    let mut source = String::from(
        "fn generated_playground_diagnostic_shader(name: &str) -> Option<&'static str> {\n    match name {\n",
    );
    for &name in diagnostics::DIAGNOSTIC_ENTRY_POINTS {
        writeln!(
            source,
            "        \"{name}\" => Some(include_str!(concat!(env!(\"OUT_DIR\"), \"/scene-playground-{name}.wgsl\"))),"
        )
        .unwrap();
    }
    source.push_str("        _ => None,\n    }\n}\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("diagnostic-shaders.rs");
    fs::write(output, source).unwrap();
}
