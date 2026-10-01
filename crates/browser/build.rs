use std::{env, fmt::Write as _, fs, path::PathBuf};

#[path = "src/diagnostics.rs"]
mod diagnostics;

const SCENE_COMPONENTS: &[&str] = &[
    "src/shader/components/scene-state.wgsl",
    "src/shader/components/vertex.wgsl",
    "src/shader/components/noise.wgsl",
    "src/shader/components/environment.wgsl",
    "src/shader/components/clouds.wgsl",
    "src/shader/components/lighting.wgsl",
    "src/shader/components/terrain.wgsl",
    "src/shader/components/water-materials.wgsl",
    "src/shader/components/composition.wgsl",
];

fn main() {
    let scene = read_components(SCENE_COMPONENTS);
    let cache_sample = read_component("src/shader/components/cloud-shadow-cache-sample.wgsl");
    let cache_debug_disabled =
        read_component("src/shader/components/cloud-shadow-cache-debug-disabled.wgsl");
    let cache_fixed_tile =
        read_component("src/shader/components/cloud-shadow-cache-fixed-tile.wgsl");
    let cache_runtime_tile =
        read_component("src/shader/components/cloud-shadow-cache-runtime-tile.wgsl");
    let playground_cache_debug =
        read_component("src/shader/components/playground-cloud-shadow-cache-debug.wgsl");
    let direct_cloud_shadows = read_component("src/shader/components/direct-cloud-shadows.wgsl");
    let production_entry = read_component("src/shader/production-entry.wgsl");
    let production = compose(&[
        &scene,
        &cache_sample,
        &cache_fixed_tile,
        &cache_debug_disabled,
        &production_entry,
    ]);
    write_shader(
        &production,
        &["vertex", "fragment"],
        "scene-production.wgsl",
    );
    let cache_generate = read_component("src/shader/components/cloud-shadow-cache-generate.wgsl");
    let production_cache = compose(&[
        &scene,
        &direct_cloud_shadows,
        &cache_fixed_tile,
        &production_entry,
        &cache_generate,
        &read_component("src/shader/production-cloud-shadow-cache-entry.wgsl"),
    ]);
    write_shader(
        &production_cache,
        &[
            "discover_cloud_shadow_cache_tiles",
            "cloud_shadow_cache_vertex",
            "cloud_shadow_cache_fragment",
        ],
        "scene-production-cloud-shadow-cache.wgsl",
    );
    let playground_interface = read_component("src/shader/playground-interface.wgsl");
    let startup = read_components(&[
        "src/shader/components/scene-state.wgsl",
        "src/shader/components/vertex.wgsl",
        "src/shader/playground-startup-entry.wgsl",
    ]);
    write_shader(
        &startup,
        &["vertex", "fragment_playground_startup"],
        "scene-playground-startup.wgsl",
    );
    let playground = compose(&[
        &scene,
        &cache_sample,
        &playground_cache_debug,
        &playground_interface,
        &read_component("src/shader/playground-entry.wgsl"),
    ]);
    write_shader(
        &playground,
        &["vertex", "fragment_playground_scene"],
        "scene-playground.wgsl",
    );
    let playground_direct = compose(&[
        &scene,
        &direct_cloud_shadows,
        &playground_interface,
        &read_component("src/shader/playground-entry.wgsl"),
    ]);
    write_shader(
        &playground_direct,
        &["vertex", "fragment_playground_scene"],
        "scene-playground-direct.wgsl",
    );
    let playground_cache = compose(&[
        &scene,
        &direct_cloud_shadows,
        &cache_runtime_tile,
        &playground_interface,
        &cache_generate,
        &read_component("src/shader/playground-cloud-shadow-cache-entry.wgsl"),
    ]);
    write_shader(
        &playground_cache,
        &[
            "discover_cloud_shadow_cache_tiles",
            "cloud_shadow_cache_vertex",
            "cloud_shadow_cache_fragment",
        ],
        "scene-playground-cloud-shadow-cache.wgsl",
    );
    let diagnostic_common = read_component("src/shader/components/diagnostic-common.wgsl");
    for &name in diagnostics::DIAGNOSTIC_ENTRY_POINTS {
        let mechanism = read_component(diagnostic_mechanism(name));
        let entry = read_component(&format!("src/shader/diagnostics/{name}.wgsl"));
        let diagnostic = compose(&[
            &scene,
            &direct_cloud_shadows,
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
