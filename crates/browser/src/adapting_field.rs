use std::borrow::Cow;

use crate::ADAPTING_FIELD_BLUR_SHADER;

/// Texels along each axis of the illumination sampled over the view and of
/// the adapting field blurred from it; adapting-field.wgsl mirrors them.
pub const ILLUMINATION_TEXELS: u32 = 32;
pub const ADAPTING_FIELD_TEXELS: u32 = 32;

/// Format of both textures, which hold stops of scene light and its colour at
/// unit luminance.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// The eye's adapting field over the view: the scene's illumination, rendered
/// by a caller's illumination pipeline, then pooled over the field's extent
/// and adapted to over the seconds the scene uniforms give. The scene reads
/// `view` at binding 7 of its first group.
pub struct AdaptingField {
    illumination_view: wgpu::TextureView,
    pub view: wgpu::TextureView,
    blur_pipeline: wgpu::RenderPipeline,
    blur_input: wgpu::BindGroup,
    field: wgpu::Texture,
    previous_field: wgpu::Texture,
}

impl AdaptingField {
    /// `uniform_layout` holds the scene uniforms alone at binding 0.
    pub fn new(device: &wgpu::Device, uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = |label, texels, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: texels,
                    height: texels,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | usage,
                view_formats: &[],
            })
        };
        let illumination = texture(
            "Scene illumination",
            ILLUMINATION_TEXELS,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let field = texture(
            "Adapting field",
            ADAPTING_FIELD_TEXELS,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let previous_field = texture(
            "Previous adapting field",
            ADAPTING_FIELD_TEXELS,
            wgpu::TextureUsages::COPY_DST,
        );
        let illumination_view = illumination.create_view(&wgpu::TextureViewDescriptor::default());
        let input_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Adapting field input layout"),
            entries: &[input_entry(0), input_entry(1)],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Adapting field shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(ADAPTING_FIELD_BLUR_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Adapting field pipeline layout"),
            bind_group_layouts: &[Some(uniform_layout), Some(&input_layout)],
            immediate_size: 0,
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Adapting field pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("fullscreen_vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("blur_adapting_field"),
                targets: &[Some(FORMAT.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            blur_input: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Adapting field input"),
                layout: &input_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&illumination_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &previous_field.create_view(&wgpu::TextureViewDescriptor::default()),
                        ),
                    },
                ],
            }),
            illumination_view,
            view: field.create_view(&wgpu::TextureViewDescriptor::default()),
            blur_pipeline,
            field,
            previous_field,
        }
    }

    /// The colour target of an illumination pipeline.
    pub fn illumination_target() -> wgpu::ColorTargetState {
        FORMAT.into()
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.field
    }

    /// Renders the illumination that `draw` records, then blurs it into the
    /// field's target and adapts the field toward it. Before anything is drawn
    /// the illumination holds daylight's zero stops in the display's white.
    /// Timestamps, when given as a query set and a beginning index, mark the
    /// start of the first pass.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        uniforms: &wgpu::BindGroup,
        timestamps: Option<(&wgpu::QuerySet, u32)>,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene illumination pass"),
                color_attachments: &[attachment(&self.illumination_view)],
                depth_stencil_attachment: None,
                timestamp_writes: timestamps.map(|(query_set, beginning)| {
                    wgpu::RenderPassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some(beginning),
                        end_of_pass_write_index: None,
                    }
                }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            draw(&mut pass);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Adapting field pass"),
            color_attachments: &[attachment(&self.view)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.blur_pipeline);
        pass.set_bind_group(0, uniforms, &[]);
        pass.set_bind_group(1, &self.blur_input, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        encoder.copy_texture_to_texture(
            self.field.as_image_copy(),
            self.previous_field.as_image_copy(),
            self.field.size(),
        );
    }

    /// Adapts the whole view to `light`, in scene light, of the colour `white`
    /// at unit luminance, in place of the field `encode` pools.
    pub fn encode_uniform(&self, encoder: &mut wgpu::CommandEncoder, light: f32, white: [f32; 3]) {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Uniform adapting field pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(light.max(game::NIGHT_SKY_LIGHT).log2()),
                        g: f64::from(white[0]),
                        b: f64::from(white[1]),
                        a: f64::from(white[2]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}
