use super::*;
use browser::BoatScene;

fn probe_shadows(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    map_view: &wgpu::TextureView,
    frame_buffer: &wgpu::Buffer,
    normal: [f32; 3],
    points: &[[f32; 4]],
) -> Vec<u8> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow visibility probes"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{}\n{}",
                include_str!("../../../crates/browser/src/shader/components/boat-shadows.wgsl"),
                r#"
@group(1) @binding(0) var<storage, read> probes: array<vec4<f32>>;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}
@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let probe = probes[u32(p.x)];
    boat_shadow_receiver_normal = RECEIVER_NORMAL;
    return vec4<f32>(vec3<f32>(boat_shadow_transmittance(probe.xyz, u32(probe.w))), 1.0);
}
"#
            )
            .replace(
                "RECEIVER_NORMAL",
                &format!(
                    "vec3<f32>({:?}, {:?}, {:?})",
                    normal[0], normal[1], normal[2]
                ),
            )
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow visibility probes"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(map_view),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: frame_buffer.as_entire_binding(),
            },
        ],
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(points),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let input = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let output = capture_texture(device, points.len() as u32, 1);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output.create_view(&Default::default()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &frame, &[]);
        pass.set_bind_group(1, &input, &[]);
        pass.draw(0..3, 0..1);
    }
    read_texture(device, queue, encoder, &output).unwrap()
}

fn boat(position: Vector) -> BoatScene {
    let mut scene = boat_scene(
        dinghy_parameters(),
        BoatMotion {
            position_m: PhysicsVector::new(f64::from(position.x), f64::from(position.y)),
            velocity_m_s: PhysicsVector::ZERO,
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.6,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        },
    );
    scene.apparent_wind_velocity_m_s = Vector::new(0.0, 8.0);
    scene
}

#[test]
fn mast_casts_a_finite_source_shadow_beyond_the_caster_far_plane() {
    let (_, device, queue) = pollster::block_on(hardware_device(wgpu::Features::empty())).unwrap();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            ADAPTING_FIELD_LAYOUT_ENTRY,
            browser::BoatShadowMaps::SCENE_LAYOUT_ENTRIES[0],
            browser::BoatShadowMaps::SCENE_LAYOUT_ENTRIES[1],
        ],
    });
    let blank = transparent_png();
    let mut sprites = BoatSprites::with_images(
        &device,
        &queue,
        &layout,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [&blank, &blank, browser::BOAT_IMAGES[2], &blank, &blank],
    )
    .unwrap();
    let scene = boat(Vector::new(-14900.0, -5330.0));
    let world = WorldGeometry::default();
    let camera = Vector::new(-14900.0, -5330.0);
    let source = [1000.0, 0.0, 2000.0, 4.0];
    let pose = browser::drawn_boat_pose(world, 0x5eed, 37.0, camera, 0.0, None, 0.01);
    let mast = scene.sail_m[0] - camera;
    let caster = rapier2d_f64::glamx::Vec3::new(mast.x, mast.y, pose.sheer_m + 2.0);
    let source_point = rapier2d_f64::glamx::Vec3::from_slice(&source[..3]);
    let receiver = source_point + (caster - source_point) * 1.02;
    let mut encoder = device.create_command_encoder(&Default::default());
    sprites.prepare_shadows(
        &device,
        &queue,
        &mut encoder,
        std::slice::from_ref(&scene),
        &[ClothMotion::new(&scene).pose()],
        world,
        0x5eed,
        camera,
        (960, 600),
        12.0,
        0.01,
        37.0,
        [source, [0.0; 4], [0.0; 4], [0.0; 4]],
    );
    let rgba = probe_shadows(
        &device,
        &queue,
        encoder,
        &sprites.shadows.view,
        &sprites.shadows.uniforms,
        [1.0, 0.0, 0.0],
        &[
            receiver.extend(2.0).to_array(),
            (receiver + rapier2d_f64::glamx::Vec3::Y * 0.5)
                .extend(2.0)
                .to_array(),
        ],
    );
    assert!(rgba[0] < 50, "mast ray remained lit: {rgba:?}");
    assert_eq!(
        rgba[4], 255,
        "empty texels shadowed a receiver beyond far depth"
    );

    let receiver = rapier2d_f64::glamx::Vec3::new(mast.x + 0.043, mast.y, pose.sheer_m + 0.8);
    let mut caster_boat = scene.clone();
    let cloth = ClothMotion::new(&caster_boat).pose();
    let sail = browser::sail_mesh_with_cloth(&caster_boat, 37.0, cloth);
    let face = sail
        .as_chunks::<3>()
        .0
        .iter()
        .find(|face| {
            face.iter().all(|v| v.height_m > 2.5 && v.height_m < 4.0)
                && (face[0].position_m - face[1].position_m).length() > 0.01
                && (face[1].position_m - face[2].position_m).length() > 0.01
        })
        .unwrap();
    let caster = face
        .iter()
        .map(|v| {
            rapier2d_f64::glamx::Vec3::new(
                v.position_m.x - camera.x,
                v.position_m.y - camera.y,
                pose.sheer_m + v.height_m,
            )
        })
        .sum::<rapier2d_f64::glamx::Vec3>()
        / 3.0;
    let fraction = (source_point.z - caster.z) / (source_point.z - receiver.z);
    let target = source_point + (receiver - source_point) * fraction;
    let shift = (target - caster).truncate();
    for point in caster_boat
        .hull_m
        .iter_mut()
        .chain(caster_boat.sail_m.iter_mut())
        .chain(caster_boat.rudder_m.iter_mut())
    {
        *point += shift;
    }
    caster_boat.tiller_end_m += shift;
    let mut cloth_caster = BoatSprites::with_images(
        &device,
        &queue,
        &layout,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [&blank, browser::BOAT_IMAGES[1], &blank, &blank, &blank],
    )
    .unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    cloth_caster.prepare_shadows(
        &device,
        &queue,
        &mut encoder,
        std::slice::from_ref(&caster_boat),
        &[cloth],
        world,
        0x5eed,
        camera,
        (960, 600),
        12.0,
        0.01,
        37.0,
        [source, [0.0; 4], [0.0; 4], [0.0; 4]],
    );
    let rgba = probe_shadows(
        &device,
        &queue,
        encoder,
        &cloth_caster.shadows.view,
        &cloth_caster.shadows.uniforms,
        [1.0, 0.0, 0.0],
        &[receiver.extend(2.0).to_array()],
    );
    assert!(
        rgba[0] < 50,
        "the other boat's sail failed to shadow the mast receiver: {rgba:?}"
    );
}

#[test]
fn shadow_filter_compares_each_tap_on_the_receiver_plane() {
    let (_, device, queue) = pollster::block_on(hardware_device(wgpu::Features::empty())).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(r#"
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}
@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {
    let xy = (p.xy / 16.0 - 0.5) * vec2<f32>(2.0, -2.0);
    // Stands in for the shadow pass's constant depth bias, which frag_depth bypasses.
    return select(0.45 + 0.2 * xy.x + 0.1 * xy.y + 1.0e-6, 0.1, p.x >= 4.0 && p.x < 8.0 && p.y >= 4.0 && p.y < 8.0);
}
"#.into()) });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            targets: &[],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    array_layer_count: Some(1),
                    ..Default::default()
                }),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    let mut frame = browser::BoatShadowUniforms::zeroed();
    frame.matrices[0] = rapier2d_f64::glamx::Mat4::IDENTITY.to_cols_array_2d();
    frame.active[0][0] = 1.0;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&frame),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let points = [
        [0.3, -0.3, 0.48, 0.0],
        [0.37, -0.23, 0.501, 0.0],
        [-0.25, 0.25, 0.425, 0.0],
    ];
    let rgba = probe_shadows(
        &device,
        &queue,
        encoder,
        &texture.create_view(&Default::default()),
        &uniform,
        [-0.2, -0.1, 1.0],
        &points,
    );
    assert_eq!(
        rgba[0], 255,
        "sloping receiver shadowed itself at the first fractional texel"
    );
    assert_eq!(
        rgba[4], 255,
        "sloping receiver shadowed itself at the second fractional texel"
    );
    assert_eq!(
        rgba[8], 0,
        "receiver-plane correction erased the raised caster's shadow"
    );
}
