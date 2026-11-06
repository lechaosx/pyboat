use std::{fs, io::BufReader, process::Command};

use game::{dinghy_parameters, procedural_water_depth_m};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed;
const WORLD_TIME_SECONDS: f64 = 43_200.0;

struct View {
    camera_m: [f32; 2],
    view_height_m: f32,
    size: (u32, u32),
}

fn capture(view: &View, extra: &[&str], name: &str) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-boats-{}-{name}-{:08x}-{:08x}.png",
        std::process::id(),
        view.camera_m[0].to_bits(),
        view.camera_m[1].to_bits(),
    ));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
            "--diagnostic",
            "scene_production",
            "--seed",
            &SEED.to_string(),
            "--world-time",
            &WORLD_TIME_SECONDS.to_string(),
            "--width",
            &view.size.0.to_string(),
            "--height",
            &view.size.1.to_string(),
            "--camera-x",
            &view.camera_m[0].to_string(),
            "--camera-y",
            &view.camera_m[1].to_string(),
            "--view-height",
            &view.view_height_m.to_string(),
            "--output",
            output.to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .expect("headless capture should start");
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr),
    );
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(&output).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    fs::remove_file(&output).unwrap();
    fs::remove_file(output.with_extension("json")).unwrap();
    bytes
}

/// Pixels that a boat at `boat_m` heading along world x changes in `view`.
fn boat_pixels(view: &View, boat_m: Vector) -> usize {
    let boat = format!("{},{},0", boat_m.x, boat_m.y);
    let without = capture(view, &[], "without");
    let with = capture(view, &["--boat", &boat], "with");
    without
        .chunks(4)
        .zip(with.chunks(4))
        .filter(|(a, b)| a != b)
        .count()
}

/// The hull's plan area in pixels of `view`, at its sea-level scale.
fn hull_pixels(view: &View) -> f32 {
    let hull = dinghy_parameters().hull;
    let pixel_m = view.view_height_m / view.size.1 as f32;
    hull.length_m * hull.beam_m / (pixel_m * pixel_m)
}

fn assert_afloat(boat_m: Vector) {
    let depth_m =
        procedural_water_depth_m(WorldGeometry::default(), SEED, WORLD_TIME_SECONDS, boat_m);
    assert!(
        depth_m > dinghy_parameters().draft_m,
        "water {depth_m} m deep at {boat_m:?}"
    );
}

/// The elevation of the terrain depth's surface at the image centre.
fn terrain_depth_elevation_at_centre_m(view: &View) -> f32 {
    let elevations = std::env::temp_dir().join(format!(
        "pyboat-boats-{}-elevations-{:08x}.f32",
        std::process::id(),
        view.camera_m[0].to_bits(),
    ));
    capture(
        view,
        &["--terrain-elevations", elevations.to_str().unwrap()],
        "elevations",
    );
    let bytes = fs::read(&elevations).unwrap();
    fs::remove_file(&elevations).unwrap();
    let (width, height) = view.size;
    let index = 4 * ((height / 2) * width + width / 2) as usize;
    f32::from_le_bytes(bytes[index..index + 4].try_into().unwrap())
}

#[test]
fn land_leaning_toward_the_camera_hides_a_boat_behind_it() {
    // The tall island's peak leans over the water east of it in a 2 km view.
    let view = View {
        camera_m: [-30_600.0, 10_600.0],
        view_height_m: 2_000.0,
        size: (1920, 1080),
    };
    let behind_m = Vector::new(-28_870.0, 11_470.0);
    let open_m = Vector::new(-32_330.0, 9_730.0);
    assert_afloat(behind_m);
    assert_afloat(open_m);
    let open = boat_pixels(&view, open_m);
    assert!(open > 0, "the boat in open water changes no pixel");
    assert_eq!(boat_pixels(&view, behind_m), 0);
}

#[test]
fn a_boat_against_a_steep_shore_stays_visible() {
    // The terrain depth's landform mesh interpolates across the waterline and
    // stands above the water here.
    let boat_m = Vector::new(-31_242.0, 9_146.0);
    let view = View {
        camera_m: [boat_m.x, boat_m.y],
        view_height_m: 15.0,
        size: (480, 270),
    };
    assert_afloat(boat_m);
    let mesh_m = terrain_depth_elevation_at_centre_m(&view);
    assert!(mesh_m > 0.5, "terrain depth at {mesh_m} m");
    let pixels = boat_pixels(&view, boat_m) as f32;
    assert!(pixels > 0.5 * hull_pixels(&view), "{pixels} pixels");
}

#[test]
fn a_boat_on_a_dry_beach_stays_visible() {
    let boat_m = Vector::new(-31_746.0, 10_160.0);
    let view = View {
        camera_m: [boat_m.x, boat_m.y],
        view_height_m: 15.0,
        size: (480, 270),
    };
    let depth_m =
        procedural_water_depth_m(WorldGeometry::default(), SEED, WORLD_TIME_SECONDS, boat_m);
    assert_eq!(depth_m, 0.0);
    let pixels = boat_pixels(&view, boat_m) as f32;
    assert!(pixels > 0.5 * hull_pixels(&view), "{pixels} pixels");
}
