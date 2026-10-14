use std::borrow::Cow;

use crate::UPSCALE_SHADER;

/// Spatial upscale from a scene rendered below the output size. Both filters
/// read and write display-encoded values through non-sRGB views of `format`.
pub struct Upscaler {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    easu: wgpu::RenderPipeline,
    rcas: wgpu::RenderPipeline,
}

pub struct UpscaleTargets {
    pub scene_size: (u32, u32),
    pub output_size: (u32, u32),
    /// The scene pass renders here, through the scene pipeline's view format.
    pub scene_view: wgpu::TextureView,
    easu_input: wgpu::BindGroup,
    easu_output: wgpu::TextureView,
    rcas_input: wgpu::BindGroup,
}

impl Upscaler {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Upscale shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UPSCALE_SHADER)),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Upscale source layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Upscale pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("fullscreen_vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            format,
            easu: pipeline("easu"),
            rcas: pipeline("rcas"),
            layout,
        }
    }

    pub fn targets(
        &self,
        device: &wgpu::Device,
        scene_size: (u32, u32),
        output_size: (u32, u32),
        scene_view_format: wgpu::TextureFormat,
    ) -> UpscaleTargets {
        let texture = |label, (width, height), view_formats: &[wgpu::TextureFormat]| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats,
            })
        };
        let input = |texture: &wgpu::Texture| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Upscale source"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                }],
            })
        };
        let scene_view_formats = [scene_view_format];
        let scene = texture(
            "Scaled scene",
            scene_size,
            if scene_view_format == self.format {
                &[]
            } else {
                &scene_view_formats
            },
        );
        let easu_output = texture("EASU output", output_size, &[]);
        UpscaleTargets {
            scene_size,
            output_size,
            scene_view: scene.create_view(&wgpu::TextureViewDescriptor {
                format: Some(scene_view_format),
                ..Default::default()
            }),
            easu_input: input(&scene),
            easu_output: easu_output.create_view(&wgpu::TextureViewDescriptor::default()),
            rcas_input: input(&easu_output),
        }
    }

    /// `output` must be a non-sRGB view. Timestamps, when given as a query set
    /// with beginning and end indices, span both passes.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        targets: &UpscaleTargets,
        output: &wgpu::TextureView,
        timestamps: Option<(&wgpu::QuerySet, u32, u32)>,
    ) {
        for (pipeline, input, view, timestamp_writes) in [
            (
                &self.easu,
                &targets.easu_input,
                &targets.easu_output,
                timestamps.map(
                    |(query_set, beginning, _)| wgpu::RenderPassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: Some(beginning),
                        end_of_pass_write_index: None,
                    },
                ),
            ),
            (
                &self.rcas,
                &targets.rcas_input,
                output,
                timestamps.map(|(query_set, _, end)| wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: None,
                    end_of_pass_write_index: Some(end),
                }),
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Upscale pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, input, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
