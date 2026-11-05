use std::{fs, process::Command};

use browser::DroneCamera;
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed;

/// The elevation each pixel of a production capture's terrain depth sees.
fn terrain_elevations_m(camera_m: Vector, view_height_m: f32, width: u32, height: u32) -> Vec<f32> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-terrain-depth-{}-{:08x}-{:08x}-{:08x}.png",
        std::process::id(),
        camera_m.x.to_bits(),
        camera_m.y.to_bits(),
        view_height_m.to_bits(),
    ));
    let elevations = output.with_extension("f32");
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
            "--diagnostic",
            "scene_production",
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
            "--seed",
            &SEED.to_string(),
            "--camera-x",
            &camera_m.x.to_string(),
            "--camera-y",
            &camera_m.y.to_string(),
            "--view-height",
            &view_height_m.to_string(),
            "--output",
            output.to_str().unwrap(),
            "--terrain-elevations",
            elevations.to_str().unwrap(),
        ])
        .output()
        .expect("headless capture should start");
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr),
    );
    let bytes = fs::read(&elevations).unwrap();
    for path in [output.clone(), output.with_extension("json"), elevations] {
        fs::remove_file(path).unwrap();
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect()
}

#[test]
fn the_terrain_depth_meets_eye_rays_where_the_camera_picks() {
    let (width, height) = (480, 270);
    // The tall island from close to wide, then a wide archipelago. Beyond about
    // 34 km a pick steps a 1080p pixel at a time and misses narrow crests.
    for (camera_m, view_height_m) in [
        (Vector::new(-30_600.0, 10_600.0), 300.0),
        (Vector::new(-30_600.0, 10_600.0), 2_000.0),
        (Vector::new(-30_600.0, 10_600.0), 20_000.0),
        (Vector::new(-4_000.0, 0.0), 20_000.0),
        (Vector::new(-4_000.0, 0.0), 34_000.0),
    ] {
        let elevations_m = terrain_elevations_m(camera_m, view_height_m, width, height);
        let camera = DroneCamera::production(WorldGeometry::default(), camera_m, view_height_m);
        let mut errors_px = Vec::new();
        let mut raised = 0;
        for row in (0..height).step_by(6) {
            for column in (0..width).step_by(6) {
                let view_point = Vector::new(
                    (column as f32 + 0.5 - 0.5 * width as f32) / height as f32,
                    (0.5 * height as f32 - row as f32 - 0.5) / height as f32,
                );
                let (_, elevation_m) = camera.terrain_point(SEED, view_point, 1080);
                let rendered_m = elevations_m[(row * width + column) as usize];
                if elevation_m.max(rendered_m) <= 0.0 {
                    continue;
                }
                // Elevation moves the hit along the eye ray by the view point's
                // offset scaled by the elevation over the camera height.
                errors_px.push(
                    view_point.length() * height as f32 * (rendered_m - elevation_m).abs()
                        / camera.height_m,
                );
                raised += usize::from(elevation_m > 50.0);
            }
        }
        errors_px.sort_by(f32::total_cmp);
        let median_px = errors_px[errors_px.len() / 2];
        let within_pixel =
            errors_px.iter().filter(|&&error| error <= 1.0).count() as f32 / errors_px.len() as f32;
        let label = format!(
            "{view_height_m} m view at {camera_m:?}: {} land samples, {raised} above 50 m, \
             median {median_px} px, {within_pixel} within a pixel, largest {} px",
            errors_px.len(),
            errors_px.last().unwrap(),
        );
        assert!(raised > 40, "{label}");
        assert!(median_px < 0.25, "{label}");
        // Wider views coarsen the landform layer, and rays grazing its coarser
        // ridges slip past them or onto them.
        if view_height_m <= 2_000.0 {
            assert!(within_pixel == 1.0, "{label}");
        } else {
            assert!(within_pixel > 0.9, "{label}");
        }
    }
}
