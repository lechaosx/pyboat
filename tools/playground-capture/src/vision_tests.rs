#[test]
fn luminance_coverage_matches_msaa_color_coverage() {
    use wgpu::util::DeviceExt;
    let (_, device, queue) =
        pollster::block_on(super::hardware_device(wgpu::Features::empty())).unwrap();
    let texture = |label, format, samples| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 3,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let color = texture("resolved color", wgpu::TextureFormat::Rgba16Float, 1);
    let aux = texture("resolved luminance", wgpu::TextureFormat::Rg16Float, 1);
    let msaa_color = texture("color coverage", wgpu::TextureFormat::Rgba16Float, 4);
    let msaa_aux = texture("luminance coverage", wgpu::TextureFormat::Rg16Float, 4);
    let output_color = texture("composite color", wgpu::TextureFormat::Rgba16Float, 1);
    let output_aux = texture("composite luminance", wgpu::TextureFormat::Rg16Float, 1);
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("coverage seed"),source:wgpu::ShaderSource::Wgsl(r#"
      @vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));return vec4<f32>(uv*vec2<f32>(2.,-2.)+vec2<f32>(-1.,1.),0.,1.);}
      struct Out{@location(0) color:vec4<f32>,@location(1) auxiliary:vec4<f32>,@builtin(sample_mask) mask:u32};
      @fragment fn fragment(@builtin(position) p:vec4<f32>)->Out{let masks=array<u32,3>(0u,1u,15u);return Out(vec4<f32>(0.,0.,0.,1.),vec4<f32>(2000.,0.,0.,1.),masks[u32(p.x)]);}
    "#.into())});
    let seed = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            targets: &[
                Some(wgpu::TextureFormat::Rgba16Float.into()),
                Some(wgpu::TextureFormat::Rg16Float.into()),
            ],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 4,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    });
    let composite_source = include_str!("../../../crates/browser/src/shader/boat-composite.wgsl");
    let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual boat composite"),
        source: wgpu::ShaderSource::Wgsl(composite_source.into()),
    });
    let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &composite_shader,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &composite_shader,
            entry_point: Some("fragment_vision"),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rg16Float,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let bounds = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0f32; 8]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let color_view = color.create_view(&Default::default());
    let aux_view = aux.create_view(&Default::default());
    let inputs = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &composite.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: bounds.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&aux_view),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let mc = msaa_color.create_view(&Default::default());
    let ma = msaa_aux.create_view(&Default::default());
    let oc = output_color.create_view(&Default::default());
    let oa = output_aux.create_view(&Default::default());
    let attachment = |view, resolve_target, clear| {
        Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })
    };
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                attachment(&mc, Some(&color_view), wgpu::Color::TRANSPARENT),
                attachment(&ma, Some(&aux_view), wgpu::Color::TRANSPARENT),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&seed);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                attachment(&oc, None, wgpu::Color::BLACK),
                attachment(
                    &oa,
                    None,
                    wgpu::Color {
                        r: 1000.,
                        g: 0.,
                        b: 0.,
                        a: 1.,
                    },
                ),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&composite);
        pass.set_bind_group(0, &inputs, &[]);
        pass.draw(0..3, 0..1);
    }
    let bytes = super::read_texture(&device, &queue, encoder, &output_aux).unwrap();
    let decode = |at| {
        let bits = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let exp = (bits >> 10) & 31;
        let mant = bits & 1023;
        if exp == 0 {
            f32::from(mant) * 2f32.powi(-24)
        } else {
            (1. + f32::from(mant) / 1024.) * 2f32.powi(i32::from(exp) - 15)
        }
    };
    let values = [decode(0), decode(4), decode(8)];
    assert_eq!(
        values,
        [1000., 1250., 2000.],
        "world1000/boat2000 cd/m² at coverage0,1/4,1"
    );
}

#[test]
fn visible_luminance_range_and_odd_pooling_phase() {
    use wgpu::util::DeviceExt;
    let (_, device, queue) =
        pollster::block_on(super::hardware_device(wgpu::Features::empty())).unwrap();
    let vision = browser::VisionPass::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let uniform_layout = super::uniform_layout(&device, "range uniforms");
    let field = browser::AdaptingField::new(&device, &uniform_layout);
    let options = super::default_options();
    let cache = super::capture_terrain_cache_frame(&options, &mut browser::TerrainCache::default())
        .unwrap();
    let sizes = browser::baked_surface_sizes((8, 3), options.bake_memory_bytes);
    let bake = super::capture_baked_surface_frame(
        &options,
        1.,
        &mut browser::BakedSurface::new(sizes),
        false,
    )
    .unwrap();
    let uniforms = super::scene_uniforms(&options, &cache, &bake).unwrap();
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let state = vision.state_bind_group(&device, &uniform, &field.view);
    let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&[9u32, 0, 0, 0]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let mut shader = browser::production_scene_shader_source().to_owned();
    shader.push_str(r#"
      @group(1) @binding(0) var<uniform> probe:vec4<u32>;
      @vertex fn probe_vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));return vec4<f32>(uv*vec2<f32>(2.,-2.)+vec2<f32>(-1.,1.),0.,1.);}
      struct ProbeOut{@location(0) color:vec4<f32>,@location(1) auxiliary:vec4<f32>};
      @fragment fn probe_fragment(@builtin(position) p:vec4<f32>)->ProbeOut{let values=array<f32,8>(0.,0.0000465,1.,32768.,65504.,100000000.,1600000000.,0.01);let index=u32(p.x)+u32(p.y)*probe.y;var y=values[index%8u];if probe.x<probe.y*probe.z {y=select(0.,1000.,index==probe.x);}return ProbeOut(vec4<f32>(0.),vision_auxiliary(vec3<f32>(y/(117300./PI)),1.));}
    "#);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual physical Y packing"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("probe_vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("probe_fragment"),
            targets: &[
                Some(wgpu::TextureFormat::Rgba16Float.into()),
                Some(wgpu::TextureFormat::Rg16Float.into()),
            ],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let param_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: params.as_entire_binding(),
        }],
    });
    let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[],
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Float,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let decode = |b: &[u8]| {
        let f = |i| {
            let bits = u16::from_le_bytes([b[i], b[i + 1]]);
            let exp = (bits >> 10) & 31;
            assert_ne!(exp, 31, "half overflow");
            let m = bits & 1023;
            if exp == 0 {
                f32::from(m) * 2f32.powi(-24)
            } else {
                (1. + f32::from(m) / 1024.) * 2f32.powi(i32::from(exp) - 15)
            }
        };
        f(0) + 32768. * f(2)
    };
    let expected = [
        0.,
        0.0000465,
        1.,
        32768.,
        65504.,
        100000000.,
        1600000000.,
        0.01,
    ];
    for size in [(8u32, 1u32), (9, 3), (45, 1)] {
        let targets = vision.targets(&device, size, size);
        let output = super::capture_texture(&device, size.0, size.1);
        let output_view = output.create_view(&Default::default());
        let count = size.0 * size.1;
        for phase in 0u32..=count {
            queue.write_buffer(&params, 0, bytemuck::bytes_of(&[phase, size.0, size.1, 0]));
            let mut encoder = device.create_command_encoder(&Default::default());
            field.encode_uniform(&mut encoder, 1.0, [1.0; 3]);
            let a = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[a(&targets.color_view), a(&targets.luminance_view)],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &empty, &[]);
                pass.set_bind_group(1, &param_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            if phase == count && size == (8, 1) {
                let bytes =
                    super::read_texture(&device, &queue, encoder, &targets.luminance).unwrap();
                for (i, b) in bytes.as_chunks::<4>().0.iter().enumerate() {
                    let actual = decode(b);
                    let error = (actual - expected[i]).abs();
                    assert!(
                        error <= expected[i] * 0.001 + 6e-8,
                        "Y{} maps to {actual}",
                        expected[i]
                    );
                }
                continue;
            }
            vision.encode(&mut encoder, &state, &targets, &output_view, true, None);
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &targets.luminance,
                    mip_level: size.0.max(size.1).ilog2(),
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                texture.as_image_copy(),
                texture.size(),
            );
            let bytes = super::read_texture(&device, &queue, encoder, &texture).unwrap();
            let actual = decode(&bytes);
            let target = if phase < count {
                1000. / count as f32
            } else {
                (0..count).map(|i| expected[i as usize % 8]).sum::<f32>() / count as f32
            };
            assert!(
                (actual - target).abs() <= target * 0.003 + 6e-8,
                "size {size:?}, phase {phase}: pooled {actual}, expected {target}"
            );
        }
    }
}

#[test]
fn scaled_linear_vision_preserves_dc_and_hdr_mixtures() {
    use wgpu::util::DeviceExt;
    let (_, device, queue) =
        pollster::block_on(super::hardware_device(wgpu::Features::empty())).unwrap();
    let scene_size = (7, 5);
    let output_size = (19, 13);
    let vision = browser::VisionPass::new(&device, wgpu::TextureFormat::Rgba16Float);
    let targets = vision.targets(&device, output_size, scene_size);
    let upscale = browser::Upscaler::new(&device, browser::VISION_COLOR_FORMAT);
    let scaled = upscale.targets(
        &device,
        scene_size,
        output_size,
        browser::VISION_COLOR_FORMAT,
    );
    let layout = super::uniform_layout(&device, "linear vision fixture");
    let field = browser::AdaptingField::new(&device, &layout);
    let options = super::default_options();
    let cache = super::capture_terrain_cache_frame(&options, &mut browser::TerrainCache::default())
        .unwrap();
    let sizes = browser::baked_surface_sizes(scene_size, options.bake_memory_bytes);
    let bake = super::capture_baked_surface_frame(
        &options,
        1.,
        &mut browser::BakedSurface::new(sizes),
        false,
    )
    .unwrap();
    let mut uniforms = super::scene_uniforms(&options, &cache, &bake).unwrap();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let state = vision.state_bind_group(&device, &buffer, &field.view);
    let mut source = browser::production_scene_shader_source().to_owned();
    source.push_str(r#"
      @vertex fn linear_probe_vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));return vec4<f32>(uv*vec2<f32>(2.,-2.)+vec2<f32>(-1.,1.),0.,1.);}
      struct LinearProbeOut{@location(0) color:vec4<f32>,@location(1) auxiliary:vec4<f32>};
      @fragment fn linear_probe_fragment(@builtin(position) p:vec4<f32>)->LinearProbeOut {
        var physical=vec3<f32>(0.1,0.2,0.3);
        if scene.exposure_and_output.w>0.0 && p.x>3.0 {physical=vec3<f32>(1600.,320.,16.);}
        let adaptation=field_adaptation(p.xy);
        return LinearProbeOut(vec4<f32>(tone_map_scene_color(physical,adaptation,p.xy),1.),vision_auxiliary(physical,1.));
      }
    "#);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let seed = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("linear_probe_vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("linear_probe_fragment"),
            targets: &[
                Some(browser::VISION_COLOR_FORMAT.into()),
                Some(browser::VISION_LUMINANCE_FORMAT.into()),
            ],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("VISION_INTERMEDIATE", 1.)],
                ..Default::default()
            },
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let seed_state = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &seed.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(&field.view),
            },
        ],
    });
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: output_size.0,
            height: output_size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&Default::default());
    let half = |bytes: &[u8]| {
        let b = u16::from_le_bytes([bytes[0], bytes[1]]);
        let exponent = (b >> 10) & 31;
        let mantissa = b & 1023;
        let value = if exponent == 0 {
            f32::from(mantissa) * 2f32.powi(-24)
        } else if exponent == 31 {
            f32::INFINITY
        } else {
            (1. + f32::from(mantissa) / 1024.) * 2f32.powi(i32::from(exponent) - 15)
        };
        if b & 32768 != 0 { -value } else { value }
    };
    for mixed in [false, true] {
        let mut reference = None;
        let mut linear_reference = None;
        for peak in [1., 4., 16.] {
            uniforms.exposure_and_output[1] = peak;
            uniforms.exposure_and_output[3] = f32::from(mixed);
            queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniforms));
            let mut encoder = device.create_command_encoder(&Default::default());
            field.encode_uniform(&mut encoder, 1., [1.; 3]);
            let a = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[a(&scaled.scene_view), a(&targets.scene_luminance_view)],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&seed);
                pass.set_bind_group(0, &seed_state, &[]);
                pass.draw(0..3, 0..1);
            }
            upscale.encode(&mut encoder, &scaled, &targets.color_view, None);
            vision.upscale_luminance(&mut encoder, &state, &targets);
            let linear = super::read_texture(&device, &queue, encoder, &targets.color).unwrap();
            assert!(
                linear
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|channel| half(channel).is_finite())
            );
            if let Some(previous) = &linear_reference {
                assert_eq!(&linear, previous, "linear stimulus depends on output peak");
            } else {
                linear_reference = Some(linear);
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            vision.encode(&mut encoder, &state, &targets, &output_view, true, None);
            let bytes = super::read_texture(&device, &queue, encoder, &output).unwrap();
            let pixels: Vec<[f32; 3]> = bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| [half(&b[0..2]), half(&b[2..4]), half(&b[4..6])])
                .collect();
            assert!(
                pixels
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && *v >= 0. && *v <= peak),
                "mixed={mixed},peak={peak}"
            );
            if !mixed {
                for pixel in &pixels {
                    for (actual, expected) in pixel.iter().zip([0.1, 0.2, 0.3]) {
                        assert!((actual - expected).abs() < 0.002, "DC {pixel:?}");
                    }
                }
            }
            if let Some(previous) = &reference {
                let previous: &Vec<[f32; 3]> = previous;
                for (a, b) in previous.iter().zip(&pixels) {
                    if a.iter().all(|v| *v < 0.79) {
                        assert_eq!(a, b, "mixed={mixed},peak={peak}: below shoulder");
                    }
                }
            }
            if peak == 1. {
                reference = Some(pixels);
            }
        }
    }
}
