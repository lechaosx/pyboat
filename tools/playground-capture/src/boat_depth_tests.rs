use super::*;
use browser::{BoatScene, ClothPose};
use wgpu::util::DeviceExt;

const SIZE: (u32, u32) = (512, 512);
const VIEW_M: f32 = 9.0;

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    scene: wgpu::BindGroup,
    light: wgpu::TextureView,
    size: (u32, u32),
    view_m: f32,
}

impl Fixture {
    fn new() -> Self {
        Self::at(SIZE, VIEW_M)
    }

    fn at(size: (u32, u32), view_m: f32) -> Self {
        let (_, device, queue) =
            pollster::block_on(hardware_device(wgpu::Features::empty())).unwrap();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("boat ordering fixture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                ADAPTING_FIELD_LAYOUT_ENTRY,
            ],
        });
        let uniforms = SceneUniforms {
            camera_and_view_m: [0.0, 0.0, view_m * size.0 as f32 / size.1 as f32, view_m],
            drone_camera: DroneCamera::production(WorldGeometry::default(), Vector::ZERO, view_m)
                .scene_uniform_row(),
            precipitation_phase_and_css_view_height_px: [0.0, size.1 as f32, 0.0, 0.0],
            exposure_and_output: [0.0, 1.0, 0.0, 0.0],
            ..SceneUniforms::zeroed()
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let light = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let write = |rgba: [f32; 4]| {
            queue.write_texture(
                light.as_image_copy(),
                bytemuck::cast_slice(&vec![rgba; 1024]),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512),
                    rows_per_image: Some(32),
                },
                light.size(),
            );
        };
        write([0.0, 1.0, 1.0, 1.0]);
        let adaptation = light.create_view(&Default::default());
        let scene = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&adaptation),
                },
            ],
        });
        // Adaptation and illumination carry different units.
        let illumination = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: light.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            illumination.as_image_copy(),
            bytemuck::cast_slice(&vec![[1.0_f32; 4]; 1024]),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512),
                rows_per_image: Some(32),
            },
            illumination.size(),
        );
        Self {
            device,
            queue,
            layout,
            scene,
            light: illumination.create_view(&Default::default()),
            size,
            view_m,
        }
    }

    fn render(&self, scenes: &[BoatScene], images: [&[u8]; 5], name: &str) -> Vec<u8> {
        let size = self.size;
        let view_m = self.view_m;
        let mut sprites = BoatSprites::with_images(
            &self.device,
            &self.queue,
            &self.layout,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            images,
        )
        .unwrap();
        let output = capture_texture(&self.device, size.0, size.1);
        let output_view = output.create_view(&Default::default());
        let surface = surface_height_texture(&self.device, size).create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for (view, clear) in [
            (
                &output_view,
                wgpu::Color {
                    r: 0.02,
                    g: 0.05,
                    b: 0.1,
                    a: 0.0,
                },
            ),
            (&surface, SURFACE_HEIGHT_CLEAR),
        ] {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        let cloth: Vec<ClothPose> = scenes
            .iter()
            .map(|scene| ClothMotion::new(scene).pose())
            .collect();
        sprites.encode(
            &self.device,
            &self.queue,
            &mut encoder,
            &output_view,
            size,
            &self.scene,
            &surface,
            &self.light,
            scenes,
            &cloth,
            WorldGeometry::default(),
            0x5eed,
            Vector::ZERO,
            view_m,
            DroneCamera::PRODUCTION_HEIGHT_M,
            view_m / size.1 as f32,
            0.3,
        );
        let rgba = read_texture(&self.device, &self.queue, encoder, &output).unwrap();
        if let Ok(dir) = env::var("PYBOAT_BOAT_DEPTH_ARTIFACTS") {
            fs::create_dir_all(&dir).unwrap();
            let mut opaque = rgba.clone();
            for pixel in opaque.as_chunks_mut::<4>().0.iter_mut() {
                pixel[3] = 255;
            }
            write_png(
                &PathBuf::from(dir).join(format!("{name}.png")),
                size.0,
                size.1,
                &opaque,
            )
            .unwrap();
        }
        rgba
    }
}

fn scene(id: u64, heading: f64, damaged: bool) -> BoatScene {
    let mut scene = boat_scene(
        dinghy_parameters(),
        BoatMotion {
            position_m: PhysicsVector::ZERO,
            velocity_m_s: PhysicsVector::ZERO,
            angle_rad: heading,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.75,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        },
    );
    scene.id = Some(id.to_string());
    scene.recently_damaged = damaged;
    scene.apparent_wind_velocity_m_s = Vector::new(4.0, 7.0);
    scene
}

#[test]
fn overlapping_boats_have_the_same_visible_surfaces_in_either_submission_order() {
    let fixture = Fixture::new();
    let a = scene(1, 0.0, false);
    let b = scene(2, 1.15, true);
    let forward = fixture.render(&[a.clone(), b.clone()], browser::BOAT_IMAGES, "forward");
    let reverse = fixture.render(&[b, a], browser::BOAT_IMAGES, "reverse");
    let changed = forward
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reverse.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 0, "submission order changes {changed} pixels");
}

fn cloth_image(alpha: u8) -> Vec<u8> {
    let mut rgba = vec![0; 1024 * 1536 * 4];
    for y in 985..1405 {
        for x in 48..474 {
            rgba[(y * 1024 + x) * 4..(y * 1024 + x) * 4 + 4].copy_from_slice(&[30, 220, 70, alpha]);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1024, 1536);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    png
}

#[test]
fn the_higher_sail_wins_when_the_same_boats_exchange_rig_heights() {
    let fixture = Fixture::new();
    let blank = transparent_png();
    let cloth = cloth_image(255);
    let images = [blank.as_slice(), cloth.as_slice(), &blank, &blank, &blank];
    for (near_id, far_id) in [(1, 2), (2, 1)] {
        let mut near = scene(near_id, 0.0, false);
        near.sail_luff_m = 5.0;
        let mut far = scene(far_id, 0.0, false);
        far.wreck = true;
        far.sail_luff_m = 0.5;
        let near_only = fixture.render(
            std::slice::from_ref(&near),
            images,
            &format!("near-{near_id}"),
        );
        let far_only = fixture.render(
            std::slice::from_ref(&far),
            images,
            &format!("far-{near_id}"),
        );
        let combined = fixture.render(&[near, far], images, &format!("heights-{near_id}"));
        let background = fixture.render(&[], images, "empty");
        let mut checked = 0;
        for (((near, far), combined), background) in near_only
            .as_chunks::<4>()
            .0
            .iter()
            .zip(far_only.as_chunks::<4>().0.iter())
            .zip(combined.as_chunks::<4>().0.iter())
            .zip(background.as_chunks::<4>().0.iter())
        {
            if near[3] == 255
                && far[3] == 255
                && near != far
                && near != background
                && far != background
            {
                assert_eq!(combined, near, "the lower sail hid the higher sail");
                checked += 1;
            }
        }
        assert!(
            checked > 100,
            "only {checked} overlapping sail pixels were exercised"
        );
    }
}

fn solid_image(rgba: [u8; 4]) -> Vec<u8> {
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    png
}

#[test]
fn half_covered_rig_keeps_the_opaque_hull_behind_it() {
    let fixture = Fixture::new();
    let blank = transparent_png();
    let red = solid_image([220, 30, 30, 255]);
    let opaque = solid_image([30, 220, 70, 255]);
    let half = solid_image([30, 220, 70, 128]);
    let boat = scene(1, 0.0, false);
    let images = |rig| {
        [
            red.as_slice(),
            blank.as_slice(),
            rig,
            blank.as_slice(),
            blank.as_slice(),
        ]
    };
    let hull = fixture.render(std::slice::from_ref(&boat), images(&blank), "backing");
    let covered = fixture.render(std::slice::from_ref(&boat), images(&opaque), "opaque-rig");
    let blended = fixture.render(std::slice::from_ref(&boat), images(&half), "half-rig");
    let rig_only = fixture.render(
        &[boat],
        [&blank, &blank, &opaque, &blank, &blank],
        "rig-coverage",
    );
    let linear = |v: u8| {
        let x = f64::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let mut checked = 0;
    for (((hull, covered), blended), rig) in hull
        .as_chunks::<4>()
        .0
        .iter()
        .zip(covered.as_chunks::<4>().0.iter())
        .zip(blended.as_chunks::<4>().0.iter())
        .zip(rig_only.as_chunks::<4>().0.iter())
    {
        if rig[3] == 255 && hull[3] == 255 {
            for channel in 0..3 {
                let expected = (linear(hull[channel]) + linear(covered[channel])) * 0.5;
                assert!(
                    (linear(blended[channel]) - expected).abs() < 0.008,
                    "half coverage lost its backing: hull={hull:?}, opaque={covered:?}, half={blended:?}"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked > 100,
        "only {checked} rig-over-hull pixels were exercised"
    );
}

fn move_scene(scene: &mut BoatScene, offset: Vector) {
    scene.hull_m = scene.hull_m.map(|p| p + offset);
    scene.sail_m = scene.sail_m.map(|p| p + offset);
    scene.rudder_m = scene.rudder_m.map(|p| p + offset);
    scene.tiller_end_m += offset;
}

#[test]
fn nearby_boats_keep_rig_depth_order_while_rocking() {
    let fixture = Fixture::new();
    let mut a = scene(1, 0.3, false);
    let mut b = scene(2, -0.3, false);
    move_scene(&mut a, Vector::new(0.0, 1.05));
    move_scene(&mut b, Vector::new(0.0, -1.05));
    let sea = game::sea_components(
        WorldGeometry::default(),
        0x5eed,
        game::OceanWaveForcing {
            surface_wind_velocity_m_s: Vector::new(12.0, 6.0),
            swell: game::Swell::NONE,
            water_depth_m: 100.0,
        },
    );
    a.sea = Some(sea);
    b.sea = Some(sea);
    let pose = browser::drawn_boat_pose(
        WorldGeometry::default(),
        0x5eed,
        0.3,
        Vector::new(0.0, 1.05),
        0.3,
        a.sea.as_ref(),
        VIEW_M / SIZE.1 as f32,
    );
    assert!(
        pose.deck_slope.length() > 0.001,
        "the fixture must exercise boat tilt"
    );
    let forward = fixture.render(
        &[a.clone(), b.clone()],
        browser::BOAT_IMAGES,
        "nearby-tilted",
    );
    let reverse = fixture.render(&[b, a], browser::BOAT_IMAGES, "nearby-tilted-reversed");
    let changed = forward
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reverse.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        changed, 0,
        "tilted boat submission order changes {changed} pixels"
    );
}

#[test]
fn boat_coverage_survives_gameplay_and_wide_views() {
    for (size, view_m, name) in [
        ((1920, 1080), 240.0, "wide"),
        ((640, 360), 240.0, "wide"),
        ((640, 360), 15.0, "gameplay"),
    ] {
        let fixture = Fixture::at(size, view_m);
        let image = fixture.render(
            &[scene(1, 0.0, false)],
            browser::BOAT_IMAGES,
            &format!("{name}-{}", size.1),
        );
        assert!(image.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    }
}

#[test]
fn bow_windsock_stays_above_the_rocking_deck() {
    let fixture = Fixture::at((768, 768), 6.0);
    let blank = transparent_png();
    let hull = solid_image([30, 220, 70, 255]);
    let mut rgba = vec![0; 1024 * 1536 * 4];
    for y in 1085..1339 {
        for x in 509..1018 {
            rgba[(y * 1024 + x) * 4..(y * 1024 + x) * 4 + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let mut sock = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut sock, 1024, 1536);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    let mut boat = scene(1, 0.3, false);
    boat.sea = Some(game::sea_components(
        WorldGeometry::default(),
        0x5eed,
        game::OceanWaveForcing {
            surface_wind_velocity_m_s: Vector::new(12.0, 6.0),
            swell: game::Swell::NONE,
            water_depth_m: 100.0,
        },
    ));
    for (i, speed) in [0.0, 3.0, 8.0].into_iter().enumerate() {
        boat.apparent_wind_velocity_m_s = Vector::new(-speed, -speed * 0.3);
        boat.windsock = Some(browser::Windsock {
            anchor_m: boat.hull_m[0],
            apparent_wind_velocity_m_s: boat.apparent_wind_velocity_m_s,
        });
        let sock_only = fixture.render(
            std::slice::from_ref(&boat),
            [&blank, &sock, &blank, &blank, &blank],
            &format!("sock-only-{i}"),
        );
        let combined = fixture.render(
            std::slice::from_ref(&boat),
            [&hull, &sock, &blank, &blank, &blank],
            &format!("sock-deck-{i}"),
        );
        fixture.render(
            std::slice::from_ref(&boat),
            browser::BOAT_IMAGES,
            &format!("sock-art-{i}"),
        );
        let mut checked = 0;
        for (sock, combined) in sock_only
            .as_chunks::<4>()
            .0
            .iter()
            .zip(combined.as_chunks::<4>().0.iter())
        {
            if sock[3] == 255 {
                assert_eq!(sock, combined, "the deck occluded the raised windsock");
                checked += 1;
            }
        }
        assert!(
            checked > 100,
            "only {checked} windsock pixels were exercised"
        );
    }
}

#[test]
fn filtered_cloth_shows_its_hull_backing_instead_of_the_sea() {
    let fixture = Fixture::new();
    let blank = transparent_png();
    let hull = solid_image([220, 0, 0, 255]);
    let cloth = cloth_image(128);
    let opaque = cloth_image(255);
    let boat = scene(1, 0.0, false);
    let backing = fixture.render(
        std::slice::from_ref(&boat),
        [&hull, &blank, &blank, &blank, &blank],
        "solid-hull",
    );
    let coverage = fixture.render(
        std::slice::from_ref(&boat),
        [&blank, &opaque, &blank, &blank, &blank],
        "sail-coverage",
    );
    let combined = fixture.render(
        &[boat],
        [&hull, &cloth, &blank, &blank, &blank],
        "filtered-sail-backing",
    );
    let mut checked = 0;
    for ((backing, cloth), combined) in backing
        .as_chunks::<4>()
        .0
        .iter()
        .zip(coverage.as_chunks::<4>().0.iter())
        .zip(combined.as_chunks::<4>().0.iter())
    {
        if backing[3] == 255 && cloth[3] == 255 {
            assert_eq!(combined[3], 255, "the cloth punched a hole in the hull");
            assert!(
                combined[0] > 80,
                "the filtered cloth lost its red hull backing"
            );
            checked += 1;
        }
    }
    assert!(checked > 100);
}
