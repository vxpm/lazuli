use glam::Vec4;
use wesl::include_wesl;
use zerocopy::IntoBytes;

pub struct XfbBlitter {
    group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl XfbBlitter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&group_layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..16,
            }],
        });

        let shader = include_wesl!("xfb_blit");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("xfb blit pipeline"),
            layout: Some(&layout),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multisample: Default::default(),
            depth_stencil: None,
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            group_layout,
            pipeline,
            sampler,
        }
    }

    pub fn blit_to_target(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let bottom_right_x = top_left.x + dimensions.width;
        let bottom_right_y = top_left.y + dimensions.height;

        let size = texture.texture().size();
        assert!(bottom_right_x <= size.width);
        assert!(bottom_right_y <= size.height);
        assert!(top_left.z + dimensions.depth_or_array_layers <= size.depth_or_array_layers);

        use zerocopy::IntoBytes;

        let uvs = Vec4::new(
            top_left.x as f32 / size.width as f32,
            top_left.y as f32 / size.height as f32,
            bottom_right_x as f32 / size.width as f32,
            bottom_right_y as f32 / size.height as f32,
        );

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, uvs.as_bytes());
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
    }
}

pub struct ColorBlitter {
    group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl ColorBlitter {
    pub fn new(device: &wgpu::Device) -> Self {
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&group_layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..16,
            }],
        });

        let shader = include_wesl!("color_blit");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("color blit pipeline"),
            layout: Some(&layout),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multisample: Default::default(),
            depth_stencil: None,
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            group_layout,
            pipeline,
            sampler,
        }
    }

    pub fn blit_to_target(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let bottom_right_x = top_left.x + dimensions.width;
        let bottom_right_y = top_left.y + dimensions.height;

        let size = texture.texture().size();
        assert!(bottom_right_x <= size.width);
        assert!(bottom_right_y <= size.height);
        assert!(top_left.z + dimensions.depth_or_array_layers <= size.depth_or_array_layers);

        let uvs = Vec4::new(
            top_left.x as f32 / size.width as f32,
            top_left.y as f32 / size.height as f32,
            bottom_right_x as f32 / size.width as f32,
            bottom_right_y as f32 / size.height as f32,
        );

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, uvs.as_bytes());
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
    }

    pub fn blit_to_texture(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("color blit to texture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        self.blit_to_target(device, source, top_left, dimensions, &mut pass);
        std::mem::drop(pass);
    }
}

pub struct DepthBlitter {
    resolve_group_layout: wgpu::BindGroupLayout,
    resolve_pipeline: wgpu::RenderPipeline,
    blit_group_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl DepthBlitter {
    pub fn new(device: &wgpu::Device) -> Self {
        let resolve_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                }],
            });

        let blit_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let resolve_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&resolve_group_layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..16,
            }],
        });

        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&blit_group_layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..16,
            }],
        });

        let resolve_shader = include_wesl!("depth_resolve");
        let resolve_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth resolve"),
            source: wgpu::ShaderSource::Wgsl(resolve_shader.into()),
        });

        let blit_shader = include_wesl!("depth_blit");
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth blit"),
            source: wgpu::ShaderSource::Wgsl(blit_shader.into()),
        });

        let resolve_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth resolve pipeline"),
            layout: Some(&resolve_layout),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                module: &resolve_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &resolve_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multisample: Default::default(),
            depth_stencil: None,
            multiview: None,
            cache: None,
        });

        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth blit pipeline"),
            layout: Some(&blit_layout),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                module: &blit_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multisample: Default::default(),
            depth_stencil: None,
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            resolve_group_layout,
            resolve_pipeline,
            blit_group_layout,
            blit_pipeline,
            sampler,
        }
    }

    fn resolve_depth(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::Texture {
        let bottom_right_x = top_left.x + dimensions.width;
        let bottom_right_y = top_left.y + dimensions.height;

        let size = texture.texture().size();
        assert!(bottom_right_x <= size.width);
        assert!(bottom_right_y <= size.height);
        assert!(top_left.z + dimensions.depth_or_array_layers <= size.depth_or_array_layers);

        let resolved = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth resolved"),
            size: dimensions,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let resolved_view = resolved.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth resolve pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &resolved_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let uvs = Vec4::new(
            top_left.x as f32 / size.width as f32,
            top_left.y as f32 / size.height as f32,
            bottom_right_x as f32 / size.width as f32,
            bottom_right_y as f32 / size.height as f32,
        );

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.resolve_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture),
            }],
        });

        pass.set_pipeline(&self.resolve_pipeline);
        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, uvs.as_bytes());
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);

        resolved
    }

    fn blit_resolved_to_target(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let bottom_right_x = top_left.x + dimensions.width;
        let bottom_right_y = top_left.y + dimensions.height;

        let size = texture.texture().size();
        assert!(bottom_right_x <= size.width);
        assert!(bottom_right_y <= size.height);
        assert!(top_left.z + dimensions.depth_or_array_layers <= size.depth_or_array_layers);

        let uvs = Vec4::new(
            top_left.x as f32 / size.width as f32,
            top_left.y as f32 / size.height as f32,
            bottom_right_x as f32 / size.width as f32,
            bottom_right_y as f32 / size.height as f32,
        );

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.blit_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        pass.set_pipeline(&self.blit_pipeline);
        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, uvs.as_bytes());
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
    }

    pub fn blit_to_texture(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        top_left: wgpu::Origin3d,
        dimensions: wgpu::Extent3d,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let resolved = self.resolve_depth(device, source, top_left, dimensions, encoder);
        let resolved_view = resolved.create_view(&Default::default());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("resolved depth blit to texture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        self.blit_resolved_to_target(
            device,
            &resolved_view,
            wgpu::Origin3d::ZERO,
            dimensions,
            &mut pass,
        );

        std::mem::drop(pass);
    }
}
