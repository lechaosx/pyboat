use std::borrow::Cow;

pub const VISION_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const VISION_LUMINANCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

pub struct VisionPass {
    input_layout: wgpu::BindGroupLayout,
    state_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    downsample: wgpu::RenderPipeline,
    upscale_luminance: wgpu::RenderPipeline,
    finish: wgpu::RenderPipeline,
    unfiltered: wgpu::RenderPipeline,
}

pub struct VisionTargets {
    pub output_size: (u32, u32),
    pub scene_size: (u32, u32),
    pub color: wgpu::Texture,
    pub luminance: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub luminance_view: wgpu::TextureView,
    pub scene_luminance_view: wgpu::TextureView,
    mip_color: Vec<wgpu::TextureView>,
    mip_luminance: Vec<wgpu::TextureView>,
    inputs: Vec<wgpu::BindGroup>,
    all: wgpu::BindGroup,
    scene_luminance_input: Option<wgpu::BindGroup>,
}

impl VisionPass {
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Vision inputs"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mut field_entry = texture_entry(7);
        field_entry.ty = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let state_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Vision scene and eye state"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                field_entry,
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Whole-view vision"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(crate::VISION_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Whole-view vision layout"),
            bind_group_layouts: &[Some(&state_layout), Some(&input_layout)],
            immediate_size: 0,
        });
        let pipeline =
            |entry_point, targets: &[Option<wgpu::ColorTargetState>], constants: &[(&str, f64)]| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry_point),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vision_vertex"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry_point),
                        targets,
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants,
                            ..Default::default()
                        },
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let downsample = pipeline(
            "vision_downsample",
            &[
                Some(VISION_COLOR_FORMAT.into()),
                Some(VISION_LUMINANCE_FORMAT.into()),
            ],
            &[],
        );
        let upscale_luminance = pipeline(
            "vision_upscale_luminance",
            &[Some(VISION_LUMINANCE_FORMAT.into())],
            &[],
        );
        let finish = pipeline("vision_finish", &[Some(output_format.into())], &[]);
        let unfiltered = pipeline(
            "vision_finish",
            &[Some(output_format.into())],
            &[("APPLY_ACUITY", 0.0)],
        );
        Self {
            input_layout,
            state_layout,
            sampler,
            downsample,
            upscale_luminance,
            finish,
            unfiltered,
        }
    }

    pub fn state_bind_group(
        &self,
        device: &wgpu::Device,
        buffer: &wgpu::Buffer,
        field: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vision scene and eye"),
            layout: &self.state_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(field),
                },
            ],
        })
    }

    pub fn targets(
        &self,
        device: &wgpu::Device,
        output_size: (u32, u32),
        scene_size: (u32, u32),
    ) -> VisionTargets {
        let levels = output_size.0.max(output_size.1).ilog2() + 1;
        let texture = |label, (width, height), mip_level_count, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let color = texture(
            "Vision linear color",
            output_size,
            levels,
            VISION_COLOR_FORMAT,
        );
        let luminance = texture(
            "Vision physical luminance",
            output_size,
            levels,
            VISION_LUMINANCE_FORMAT,
        );
        let mip_views = |texture: &wgpu::Texture| {
            (0..levels)
                .map(|level| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: level,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>()
        };
        let mip_color = mip_views(&color);
        let mip_luminance = mip_views(&luminance);
        let input = |color: &wgpu::TextureView, luminance: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Vision image inputs"),
                layout: &self.input_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(color),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(luminance),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let inputs = mip_color
            .iter()
            .zip(&mip_luminance)
            .map(|(color, luminance)| input(color, luminance))
            .collect();
        let all = input(
            &color.create_view(&Default::default()),
            &luminance.create_view(&Default::default()),
        );
        let color_view = mip_color[0].clone();
        let luminance_view = mip_luminance[0].clone();
        let scene_luminance_view = if scene_size == output_size {
            luminance_view.clone()
        } else {
            texture(
                "Scaled physical luminance",
                scene_size,
                1,
                VISION_LUMINANCE_FORMAT,
            )
            .create_view(&Default::default())
        };
        let scene_luminance_input =
            (scene_size != output_size).then(|| input(&color_view, &scene_luminance_view));
        VisionTargets {
            output_size,
            scene_size,
            color,
            luminance,
            color_view,
            luminance_view,
            scene_luminance_view,
            mip_color,
            mip_luminance,
            inputs,
            all,
            scene_luminance_input,
        }
    }

    pub fn upscale_luminance(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        state: &wgpu::BindGroup,
        targets: &VisionTargets,
    ) {
        if let Some(input) = &targets.scene_luminance_input {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Upscale physical luminance"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.luminance_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.upscale_luminance);
            pass.set_bind_group(0, state, &[]);
            pass.set_bind_group(1, input, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        state: &wgpu::BindGroup,
        targets: &VisionTargets,
        output: &wgpu::TextureView,
        acuity: bool,
        timestamps: Option<(&wgpu::QuerySet, Option<u32>, u32)>,
    ) {
        let attachment = |view| {
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
        let mut first = true;
        for level in 1..if acuity { targets.mip_color.len() } else { 1 } {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Vision mip"),
                color_attachments: &[
                    attachment(&targets.mip_color[level]),
                    attachment(&targets.mip_luminance[level]),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: timestamps
                    .filter(|(_, start, _)| first && start.is_some())
                    .map(|(query_set, start, _)| wgpu::RenderPassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: start,
                        end_of_pass_write_index: None,
                    }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.downsample);
            pass.set_bind_group(0, state, &[]);
            pass.set_bind_group(1, &targets.inputs[level - 1], &[]);
            pass.draw(0..3, 0..1);
            first = false;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Vision output"),
            color_attachments: &[attachment(output)],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps.map(|(query_set, start, end)| {
                wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: if first { start } else { None },
                    end_of_pass_write_index: Some(end),
                }
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(if acuity {
            &self.finish
        } else {
            &self.unfiltered
        });
        pass.set_bind_group(0, state, &[]);
        pass.set_bind_group(1, &targets.all, &[]);
        pass.draw(0..3, 0..1);
    }
}
