mod data;
mod framebuffer;
mod pipeline;
mod texture;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec2};
use lazuli::modules::render::{Action, Sampler, Scaling, Viewport};
use lazuli::system::gx::color::Rgba;
use lazuli::system::gx::pix::{ConstantAlpha, Scissor};
use lazuli::system::gx::tev::Fog;
use lazuli::system::gx::xform::{Channel, Light};
use lazuli::system::gx::{EFB_HEIGHT, EFB_WIDTH, MatrixId, Topology, Vertex, VertexStream};
use rustc_hash::FxBuildHasher;
use schnellru::{ByLength, LruMap};
use seq_macro::seq;
use zerocopy::IntoBytes;

use crate::alloc::Allocator;
use crate::blit::{ColorBlitter, Converter, DepthBlitter};
use crate::clear::Cleaner;
use crate::render::texture::TextureRef;

pub struct Shared {
    pub output: Mutex<wgpu::TextureView>,
    pub rendered_anything: AtomicBool,
}

struct Allocators {
    index: Allocator,
    storage: Allocator,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct TexSlotConfig {
    texture: TextureRef,
    sampler: Sampler,
    scaling: Scaling,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct DataGroupEntries {
    vertices: wgpu::Buffer,
    matrices: wgpu::Buffer,
    configs: wgpu::Buffer,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TexturesGroupEntries {
    textures: [wgpu::TextureView; 8],
    samplers: [wgpu::Sampler; 8],
}

type GroupCache<K> = LruMap<K, wgpu::BindGroup, ByLength, FxBuildHasher>;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shared: Arc<Shared>,

    current_transfer_encoder: wgpu::CommandEncoder,
    current_render_encoder: wgpu::CommandEncoder,
    current_pass: wgpu::RenderPass<'static>,

    // components
    pipeline_config: pipeline::Config,
    embedded_fb: framebuffer::Embedded,
    external_fb: framebuffer::External,
    allocators: Allocators,
    tex_slots: [TexSlotConfig; 8],
    cleaner: Cleaner,
    converter: Converter,
    color_blitter: ColorBlitter,
    depth_blitter: DepthBlitter,
    data_read_buffer: wgpu::Buffer,

    // caches
    pipeline_cache: pipeline::Cache,
    texture_cache: texture::Cache,
    textures_group_cache: GroupCache<TexturesGroupEntries>,

    // state
    viewport: Viewport,
    scissor: Scissor,
    clear_color: Rgba,
    clear_depth: f32,
    current_config: data::Config,
    current_config_dirty: bool,

    indices: Vec<u32>,
    vertices: Vec<data::Vertex>,
    matrices: Vec<Mat4>,
    configs: Vec<data::Config>,

    actions: u64,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> (Self, Arc<Shared>) {
        let embedded_fb = framebuffer::Embedded::new(&device);
        let external_fb = framebuffer::External::new(&device);

        let allocators = Allocators {
            index: Allocator::new(&device, wgpu::BufferUsages::INDEX),
            storage: Allocator::new(&device, wgpu::BufferUsages::STORAGE),
        };

        let pipeline_cache = pipeline::Cache::new(&device);
        let texture_cache = texture::Cache::default();

        let color = embedded_fb.color();
        let multisampled_color = embedded_fb.multisampled_color();
        let depth = embedded_fb.depth();

        let shared = Arc::new(Shared {
            output: Mutex::new(external_fb.framebuffer().clone()),
            rendered_anything: AtomicBool::new(false),
        });

        let cleaner = Cleaner::new(&device);
        let converter = Converter::new(&device);
        let color_blitter = ColorBlitter::new(&device);
        let depth_blitter = DepthBlitter::new(&device);

        let data_read_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("data read buffer"),
            size: EFB_WIDTH * EFB_HEIGHT * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let transfer_encoder = device.create_command_encoder(&Default::default());
        let mut render_encoder = device.create_command_encoder(&Default::default());
        let pass = render_encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lazuli render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: multisampled_color,
                    depth_slice: None,
                    resolve_target: Some(color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();

        let mut value = Self {
            device,
            queue,
            shared: shared.clone(),

            current_transfer_encoder: transfer_encoder,
            current_render_encoder: render_encoder,
            current_pass: pass,

            pipeline_config: Default::default(),
            embedded_fb,
            external_fb,
            allocators,
            tex_slots: Default::default(),
            cleaner,
            converter,
            color_blitter,
            depth_blitter,
            data_read_buffer,

            pipeline_cache,
            texture_cache,
            textures_group_cache: LruMap::with_hasher(ByLength::new(512), FxBuildHasher),

            viewport: Default::default(),
            scissor: Default::default(),
            clear_color: Default::default(),
            clear_depth: 1.0,
            current_config: Default::default(),
            current_config_dirty: true,

            vertices: Vec::new(),
            indices: Vec::new(),
            configs: Vec::new(),
            matrices: Vec::new(),

            actions: 0,
        };

        value.reset();
        (value, shared)
    }

    pub fn exec(&mut self, action: Action) {
        match action {
            Action::SetXfbDimensions(dims) => self.set_xfb_dimensions(dims),
            Action::SetEfbFormat(fmt) => self.set_efb_format(fmt),
            Action::SetViewport(viewport) => self.set_viewport(viewport),
            Action::SetScissor(scissor) => self.set_scissor(scissor),
            Action::SetCullingMode(mode) => self.set_culling_mode(mode),
            Action::SetClearColor(color) => self.set_clear_color(color),
            Action::SetClearDepth(depth) => self.set_clear_depth(depth),
            Action::SetBlendMode(mode) => self.set_blend_mode(mode),
            Action::SetDepthMode(mode) => self.set_depth_mode(mode),
            Action::SetAlphaTest(test) => self.set_alpha_test(test),
            Action::SetConstantAlpha(mode) => self.set_constant_alpha_mode(mode),
            Action::SetProjectionMatrix(mtx) => self.set_projection_mtx(mtx.value()),
            Action::SetTexEnvConfig(config) => self.set_texenv_config(config),
            Action::SetTexGenConfig(config) => self.set_texgen_config(config),
            Action::LoadTexture { id, texture } => self.load_texture(id, texture),
            Action::LoadClut { id, clut } => self.load_clut(id, clut),
            Action::SetTextureSlot {
                slot,
                texture_id,
                clut_ref,
                sampler,
                scaling,
            } => self.set_texture_slot(slot, texture_id, clut_ref, sampler, scaling),
            Action::Draw(topology, vertices) => match topology {
                Topology::QuadList => self.draw_quad_list(&vertices),
                Topology::TriangleList => self.draw_triangle_list(&vertices),
                Topology::TriangleStrip => self.draw_triangle_strip(&vertices),
                Topology::TriangleFan => self.draw_triangle_fan(&vertices),
                Topology::LineList => tracing::warn!("ignored line list primitive"),
                Topology::LineStrip => tracing::warn!("ignored line strip primitive"),
                Topology::PointList => tracing::warn!("ignored point list primitive"),
            },
            Action::SetAmbient(idx, color) => self.set_ambient(idx, color.into()),
            Action::SetMaterial(idx, color) => self.set_material(idx, color.into()),
            Action::SetColorChannel(idx, control) => self.set_color_channel(idx, control),
            Action::SetAlphaChannel(idx, control) => self.set_alpha_channel(idx, control),
            Action::SetLight(idx, light) => self.set_light(idx, light),
            Action::SetFog(fog) => self.set_fog(fog),
            Action::CopyColor {
                args,
                format,
                response,
                id,
            } => self.copy_color(args, format, response, id),
            Action::CopyDepth {
                args,
                format,
                response,
                id,
            } => self.copy_depth(args, format, response, id),
            Action::CopyXfb { args, id } => self.copy_xfb(args, id),
            Action::PresentXfb(parts) => self.present_xfb(parts),
        }

        self.actions += 1;
    }

    fn debug(&mut self, s: impl AsRef<str>) {
        let string = s.as_ref();
        let lines = string.lines();
        for line in lines {
            self.current_pass.insert_debug_marker(line);
        }
    }

    fn insert_vertex(&mut self, vertex: &Vertex, matrices: &[(MatrixId, u32)]) -> u32 {
        let get_matrix = |idx| matrices.iter().find_map(|(i, m)| (*i == idx).then_some(*m));
        let vertex = data::Vertex {
            position: vertex.position,
            config_idx: self.configs.len() as u32 - 1,
            normal: vertex.normal,
            _pad0: 0,

            position_mtx_idx: get_matrix(vertex.pos_norm_matrix).unwrap(),
            normal_mtx_idx: get_matrix(vertex.pos_norm_matrix.normal()).unwrap(),
            _pad1: 0,
            _pad2: 0,

            chan0: vertex.chan0,
            chan1: vertex.chan1,

            tex_coord: vertex.tex_coords,
            tex_coord_mtx_idx: seq! {
                N in 0..8 {
                    [#(get_matrix(vertex.tex_coords_matrix[N]).unwrap(),)*]
                }
            },
        };

        let idx = self.vertices.len();
        self.vertices.push(vertex);

        idx as u32
    }

    fn apply_scissor_and_viewport(&mut self) {
        let (scissor_x, scissor_y) = self.scissor.top_left();
        let (scissor_width, scissor_height) = self.scissor.dimensions();
        let (scissor_offset_x, scissor_offset_y) = self.scissor.offset();

        let (scissor_effective_x, scissor_effective_y) = (
            scissor_x
                .saturating_sub_signed(scissor_offset_x)
                .min(EFB_WIDTH as u32),
            scissor_y
                .saturating_sub_signed(scissor_offset_y)
                .min(EFB_HEIGHT as u32),
        );

        let scissor_max_width = EFB_WIDTH as u32 - scissor_effective_x;
        let scissor_max_height = EFB_HEIGHT as u32 - scissor_effective_y;
        self.current_pass.set_scissor_rect(
            scissor_effective_x,
            scissor_effective_y,
            scissor_width.min(scissor_max_width),
            scissor_height.min(scissor_max_height),
        );

        self.current_pass.set_viewport(
            self.viewport.top_left_x - scissor_offset_x as f32,
            self.viewport.top_left_y - scissor_offset_y as f32,
            self.viewport.width,
            self.viewport.height,
            self.viewport.near_depth.clamp(0.0, 1.0),
            self.viewport.far_depth.clamp(0.0, 1.0),
        );
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        if self.viewport != viewport {
            self.flush(format_args!("changed viewport to {viewport:?}"));
            self.viewport = viewport;
        }
    }

    fn set_scissor(&mut self, scissor: Scissor) {
        if self.scissor != scissor {
            self.flush(format_args!("changed scissor to {scissor:?}"));
            self.scissor = scissor;
        }
    }

    fn set_clear_color(&mut self, rgba: Rgba) {
        self.clear_color = rgba;
    }

    fn set_clear_depth(&mut self, depth: f32) {
        self.clear_depth = depth;
    }

    fn set_constant_alpha_mode(&mut self, mode: ConstantAlpha) {
        self.debug(format!("set constant alpha mode to {mode:?}"));
        self.current_config.constant_alpha = if mode.enabled() {
            mode.value() as u32
        } else {
            u32::MAX
        };
        self.current_config_dirty = true;
    }

    fn set_ambient(&mut self, idx: u8, color: Rgba) {
        self.current_config.ambient[idx as usize] = color;
        self.current_config_dirty = true;
    }

    fn set_material(&mut self, idx: u8, color: Rgba) {
        self.current_config.material[idx as usize] = color;
        self.current_config_dirty = true;
    }

    fn set_color_channel(&mut self, idx: u8, control: Channel) {
        self.current_config.color_channels[idx as usize].update(control);
        self.current_config_dirty = true;
    }

    fn set_alpha_channel(&mut self, idx: u8, control: Channel) {
        self.current_config.alpha_channels[idx as usize].update(control);
        self.current_config_dirty = true;
    }

    fn set_light(&mut self, idx: u8, light: Light) {
        self.current_config.lights[idx as usize].update(light);
        self.current_config_dirty = true;
    }

    fn set_fog(&mut self, fog: Fog) {
        self.pipeline_config.shader.texenv.fog.mode = fog.c.mode();
        self.pipeline_config.shader.texenv.fog.orthographic = fog.c.orthographic();
        self.current_config.fog.update(fog);
        self.current_config_dirty = true;
    }

    fn set_projection_mtx(&mut self, mtx: Mat4) {
        self.current_config.projection_mtx = mtx;
        self.current_config_dirty = true;
    }

    fn flush_config(&mut self) {
        if std::mem::take(&mut self.current_config_dirty) {
            self.debug("flushing config");
            self.configs.push(self.current_config.clone());
        }
    }

    fn create_matrix_indices(&mut self, matrices: &[(MatrixId, Mat4)]) -> Vec<(MatrixId, u32)> {
        let mut indices = Vec::with_capacity(matrices.len());

        for (id, mat) in matrices.iter().copied() {
            let idx = self.matrices.len();
            self.matrices.push(mat);
            indices.push((id, idx as u32));
        }

        indices
    }

    fn draw_quad_list(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
            return;
        }

        if vertices.len() < 4 {
            tracing::warn!("malformed quad list draw call");
            return;
        }

        self.flush_config();
        let matrices = self.create_matrix_indices(matrices);
        for vertices in vertices.iter().array_chunks::<4>() {
            let [v0, v1, v2, v3] = vertices.map(|v| self.insert_vertex(v, &matrices));
            self.indices.extend_from_slice(&[v0, v1, v2]);
            self.indices.extend_from_slice(&[v0, v2, v3]);
        }
    }

    fn draw_triangle_list(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
            return;
        }

        if vertices.len() < 3 {
            tracing::warn!("malformed triangle list draw call");
            return;
        }

        self.flush_config();
        let matrices = self.create_matrix_indices(matrices);
        for vertices in vertices.iter().array_chunks::<3>() {
            let vertices = vertices.map(|v| self.insert_vertex(v, &matrices));
            self.indices.extend_from_slice(&vertices);
        }
    }

    fn draw_triangle_strip(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
            return;
        }

        if vertices.len() < 3 {
            tracing::warn!("malformed triangle strip draw call");
            return;
        }

        self.flush_config();
        let matrices = self.create_matrix_indices(matrices);
        let mut iter = vertices.iter();
        let mut v0 = self.insert_vertex(iter.next().unwrap(), &matrices);
        let mut v1 = self.insert_vertex(iter.next().unwrap(), &matrices);

        for (i, v2) in iter.enumerate() {
            let v2 = self.insert_vertex(v2, &matrices);

            // flip to preserve vertex order (cw)
            if i.is_multiple_of(2) {
                self.indices.extend_from_slice(&[v0, v1, v2]);
            } else {
                self.indices.extend_from_slice(&[v2, v1, v0]);
            }

            v0 = v1;
            v1 = v2;
        }
    }

    fn draw_triangle_fan(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
            return;
        }

        if vertices.len() < 3 {
            tracing::warn!("malformed triangle fan draw call");
            return;
        }

        self.flush_config();
        let matrices = self.create_matrix_indices(matrices);
        let mut iter = vertices.iter();
        let v0 = self.insert_vertex(iter.next().unwrap(), &matrices);
        let mut v1 = self.insert_vertex(iter.next().unwrap(), &matrices);
        for v2 in iter {
            let v2 = self.insert_vertex(v2, &matrices);
            self.indices.extend_from_slice(&[v0, v1, v2]);

            v1 = v2;
        }
    }

    fn reset(&mut self) {
        self.indices.clear();
        self.vertices.clear();
        self.matrices.clear();
        self.configs.clear();
        self.current_config_dirty = true;
    }

    fn get_data_group(&mut self, entries: DataGroupEntries) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: self.pipeline_cache.data_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &entries.vertices,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &entries.matrices,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &entries.configs,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        })
    }

    fn get_textures_group(&mut self, entries: TexturesGroupEntries) -> wgpu::BindGroup {
        self.textures_group_cache
            .get_or_insert(entries.clone(), || {
                let textures_group_entries: [wgpu::BindGroupEntry; 16] =
                    std::array::from_fn(|binding| {
                        let tex = binding / 2;
                        let resource = match binding % 2 {
                            0 => wgpu::BindingResource::TextureView(&entries.textures[tex]),
                            1 => wgpu::BindingResource::Sampler(&entries.samplers[tex]),
                            _ => unreachable!(),
                        };

                        wgpu::BindGroupEntry {
                            binding: binding as u32,
                            resource,
                        }
                    });

                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: self.pipeline_cache.textures_group_layout(),
                    entries: &textures_group_entries,
                })
            })
            .unwrap()
            .clone()
    }

    /// Flushes all pending draws as a single draw call.
    fn flush(&mut self, reason: std::fmt::Arguments) {
        if self.vertices.is_empty() {
            return;
        }

        self.debug(format!("[FLUSH]: {reason}"));
        let scaling_array = self.tex_slots.map(|s| Vec2::new(s.scaling.u, s.scaling.v));
        let lodbias_array = self.tex_slots.map(|s| s.sampler.mode.lod_bias());

        let index_buf = self.allocators.index.allocate(
            &self.device,
            &mut self.current_transfer_encoder,
            self.indices.as_bytes(),
        );
        let vertices_buf = self.allocators.storage.allocate(
            &self.device,
            &mut self.current_transfer_encoder,
            self.vertices.as_bytes(),
        );
        let matrices_buf = self.allocators.storage.allocate(
            &self.device,
            &mut self.current_transfer_encoder,
            self.matrices.as_bytes(),
        );
        let configs_buf = self.allocators.storage.allocate(
            &self.device,
            &mut self.current_transfer_encoder,
            self.configs.as_bytes(),
        );

        let data_group = self.get_data_group(DataGroupEntries {
            vertices: vertices_buf,
            matrices: matrices_buf,
            configs: configs_buf,
        });

        let textures = self.tex_slots.map(|s| {
            self.texture_cache
                .get_texture(&self.device, &self.queue, s.texture)
                .clone()
        });

        let samplers = self.tex_slots.map(|s| {
            self.texture_cache
                .get_sampler(&self.device, s.sampler)
                .clone()
        });

        let textures_group = self.get_textures_group(TexturesGroupEntries { textures, samplers });

        self.apply_scissor_and_viewport();

        let pipeline = self.pipeline_cache.get(&self.device, &self.pipeline_config);

        self.current_pass.set_pipeline(pipeline);
        self.current_pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            0,
            scaling_array.as_bytes(),
        );
        self.current_pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            64,
            lodbias_array.as_bytes(),
        );
        self.current_pass.set_bind_group(0, Some(&data_group), &[]);
        self.current_pass
            .set_bind_group(1, Some(&textures_group), &[]);
        self.current_pass.set_index_buffer(
            index_buf.slice(..self.indices.as_bytes().len() as u64),
            wgpu::IndexFormat::Uint32,
        );
        self.current_pass
            .draw_indexed(0..self.indices.len() as u32, 0, 0..1);

        self.reset();
    }

    // Finishes the current render pass and starts the next one.
    fn submit(&mut self) {
        self.flush(format_args!("finishing pass"));

        let color = self.embedded_fb.color();
        let depth = self.embedded_fb.depth();
        let multisampled_color = self.embedded_fb.multisampled_color();

        let transfer_encoder = self.device.create_command_encoder(&Default::default());
        let mut render_encoder = self.device.create_command_encoder(&Default::default());
        let pass = render_encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: multisampled_color,
                    depth_slice: None,
                    resolve_target: Some(color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();

        let prev_transfer_encoder =
            std::mem::replace(&mut self.current_transfer_encoder, transfer_encoder);
        let prev_render_encoder =
            std::mem::replace(&mut self.current_render_encoder, render_encoder);

        let previous_pass = std::mem::replace(&mut self.current_pass, pass);
        std::mem::drop(previous_pass);

        let transfer_cmds = prev_transfer_encoder.finish();
        let render_cmds = prev_render_encoder.finish();

        self.queue.submit([transfer_cmds, render_cmds]);
        self.device.poll(wgpu::PollType::Poll).unwrap();

        self.allocators.index.free();
        self.allocators.storage.free();
        self.textures_group_cache.clear();

        self.shared.rendered_anything.store(true, Ordering::Relaxed);
    }
}
