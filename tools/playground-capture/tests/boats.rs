use std::{fs, io::BufReader, process::Command};

use game::{
    Waterplane, dinghy_parameters, hull_wave_fit_of_sea, procedural_water_depth_m,
    procedural_world_environment,
};
use rapier2d_f64::glamx::Vec2 as Vector;
use rapier2d_f64::prelude::Rotation as PhysicsRotation;
use simulation::{WorldGeometry, hull_outline_m};

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
    (hull.length_m * hull.beam_m) as f32 / (pixel_m * pixel_m)
}

fn assert_afloat(boat_m: Vector) {
    let depth_m =
        procedural_water_depth_m(WorldGeometry::default(), SEED, WORLD_TIME_SECONDS, boat_m);
    assert!(
        f64::from(depth_m) > dinghy_parameters().draft_m,
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

/// Over deep water of the default world, 100 clocks and four headings at
/// each of 100 points, where the drawn water stands highest over a drawn
/// hull's outline above the deck plane the boat is drawn at, for a scene of
/// pixel footprint `footprint_m`: the boat's position, clock, heading in
/// radians, and that height. The plane rises with the drawn wind sea fitted
/// over the hull and tilts with the whole sea, swell included.
fn highest_water_over_a_hull(footprint_m: f64) -> (Vector, u64, f64, f64) {
    let world = WorldGeometry::default();
    let hull = dinghy_parameters().hull;
    let point = Waterplane {
        heading_rad: 0.0,
        length_m: 0.0,
        beam_m: 0.0,
    };
    let mut highest = (Vector::ZERO, 0, 0.0, f64::NEG_INFINITY);
    for clock in 0..100_u64 {
        let tick = 600 + clock * 14_246_917;
        for index in 0..100 {
            let position_m = Vector::new(
                -30_000.0 + (index % 10) as f32 * 6_011.0 + clock as f32 * 37.0,
                -29_000.0 + (index / 10) as f32 * 5_903.0,
            );
            let sample = procedural_world_environment(world, SEED, tick, position_m);
            if sample.environment.water_depth_m < 20.0 {
                continue;
            }
            for quarter in 0..4 {
                let heading_rad =
                    f64::from(quarter) * std::f64::consts::FRAC_PI_4 + index as f64 * 0.1;
                let fit = hull_wave_fit_of_sea(
                    world,
                    SEED,
                    tick,
                    0.0,
                    position_m,
                    &sample.sea,
                    Waterplane::of_hull(hull, heading_rad),
                    footprint_m,
                );
                let rotation = PhysicsRotation::new(heading_rad);
                for corner_m in hull_outline_m(hull) {
                    let offset_m = rotation.transform_vector(corner_m);
                    let water_m = hull_wave_fit_of_sea(
                        world,
                        SEED,
                        tick,
                        0.0,
                        position_m + offset_m.as_vec2(),
                        &sample.sea,
                        point,
                        footprint_m,
                    )
                    .wind_sea
                    .height_m;
                    let height_m = water_m - fit.wind_sea.height_m - fit.sea.slope.dot(offset_m);
                    if height_m > highest.3 {
                        highest = (position_m, tick, heading_rad, height_m);
                    }
                }
            }
        }
    }
    highest
}

/// The hull pixels a boat at `boat_m` changes in `view` at `tick`, heading
/// `heading_rad`, drawn behind the scene's surfaces or in front of them.
fn hull_pixels_drawn(
    view: &View,
    boat_m: Vector,
    tick: u64,
    heading_rad: f64,
    unhidden: bool,
) -> usize {
    let world_time = (tick as f64 / 60.0).to_string();
    let boat = format!("{},{},{}", boat_m.x, boat_m.y, heading_rad.to_degrees());
    let time = ["--world-time", world_time.as_str()];
    let without = capture(view, &time, "hull-without");
    let mut extra = vec![
        "--world-time",
        world_time.as_str(),
        "--boat",
        boat.as_str(),
        "--boat-hull-only",
    ];
    if unhidden {
        extra.push("--boat-unhidden");
    }
    let with = capture(view, &extra, "hull-with");
    without
        .chunks(4)
        .zip(with.chunks(4))
        .filter(|(a, b)| a != b)
        .count()
}

/// Where the drawn water stands highest over a hull's outline above its
/// deck plane, which the undrawn swell tilts, the hull stays wholly in view:
/// in a 30 m view of 540 rows and a 240 m view of 1080, the hull changes as
/// many pixels behind the scene's surfaces as in front of them.
#[test]
fn the_water_never_hides_an_afloat_hull() {
    for (view_height_m, size) in [(30.0, (960, 540)), (240.0, (1920, 1080))] {
        let footprint_m = f64::from(view_height_m) / f64::from(size.1);
        let (boat_m, tick, heading_rad, height_m) = highest_water_over_a_hull(footprint_m);
        let view = View {
            camera_m: [boat_m.x, boat_m.y],
            view_height_m,
            size,
        };
        let unhidden = hull_pixels_drawn(&view, boat_m, tick, heading_rad, true);
        let drawn = hull_pixels_drawn(&view, boat_m, tick, heading_rad, false);
        assert!(
            unhidden as f32 > 0.5 * hull_pixels(&view) && drawn == unhidden,
            "a {view_height_m} m view: {drawn} of {unhidden} hull pixels drawn where the water stands {height_m:.3} m over the deck plane at {boat_m:?}, tick {tick}",
        );
    }
}
