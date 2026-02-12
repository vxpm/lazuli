mod data;
mod framebuffer;
mod pipeline;
mod sampler;
mod texture;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec2};
use lazuli::modules::render::{
    Action, Clut, ClutAddress, CopyArgs, Sampler, Scaling, TexEnvConfig, TexGenConfig, Texture,
    TextureId, Viewport, XfbPart, oneshot,
};
use lazuli::system::gx::color::{Rgba, Rgba8};
use lazuli::system::gx::pix::{
    self, BlendMode, CompareMode, ConstantAlpha, DepthMode, DstBlendFactor, Scissor, SrcBlendFactor,
};
use lazuli::system::gx::xform::{ChannelControl, Light};
use lazuli::system::gx::{
    CullingMode, DEPTH_24_BIT_MAX, EFB_HEIGHT, EFB_WIDTH, MatrixId, Topology, Vertex, VertexStream,
    tev, tex,
};
use lazuli::system::vi::Dimensions;
use rustc_hash::{FxBuildHasher, FxHashMap};
use schnellru::{ByLength, LruMap};
use seq_macro::seq;
use zerocopy::{FromBytes, IntoBytes};

use crate::alloc::Allocator;
use crate::blit::{ColorBlitter, DepthBlitter};
use crate::clear::Cleaner;
use crate::render::pipeline::TexGenStageSettings;
use crate::render::texture::TextureSettings;

pub struct Shared {
    pub output: Mutex<wgpu::TextureView>,
    pub rendered_anything: AtomicBool,
}

struct Allocators {
    index: Allocator,
    storage: Allocator,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct TexSlotSettings {
    settings: TextureSettings,
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
    pipeline_settings: pipeline::Settings,
    embedded_fb: framebuffer::Embedded,
    external_fb: framebuffer::External,
    allocators: Allocators,
    tex_slots: [TexSlotSettings; 8],
    cleaner: Cleaner,
    color_blitter: ColorBlitter,
    depth_blitter: DepthBlitter,
    color_copy_buffer: wgpu::Buffer,
    depth_copy_buffer: wgpu::Buffer,
    color_copy_texture_pool: FxHashMap<wgpu::Extent3d, wgpu::TextureView>,
    depth_copy_texture_pool: FxHashMap<wgpu::Extent3d, wgpu::TextureView>,

    // caches
    pipeline_cache: pipeline::Cache,
    texture_cache: texture::Cache,
    sampler_cache: sampler::Cache,
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

fn set_channel(channel: &mut data::Channel, control: ChannelControl) {
    channel.material_from_vertex = control.material_from_vertex() as u32;
    channel.ambient_from_vertex = control.ambient_from_vertex() as u32;
    channel.lighting_enabled = control.lighting_enabled() as u32;
    channel.diffuse_attenuation = control.diffuse_attenuation() as u32;
    channel.attenuation = control.attenuation() as u32;
    channel.specular = !control.not_specular() as u32;

    let a = control.lights0to3();
    let b = control.lights4to7();
    channel.light_mask = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]].map(|b| b as u32);
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
        let sampler_cache = sampler::Cache::default();

        let color = embedded_fb.color();
        let multisampled_color = embedded_fb.multisampled_color();
        let depth = embedded_fb.depth();

        let shared = Arc::new(Shared {
            output: Mutex::new(external_fb.framebuffer().clone()),
            rendered_anything: AtomicBool::new(false),
        });

        let cleaner = Cleaner::new(&device);
        let color_blitter = ColorBlitter::new(&device);
        let depth_blitter = DepthBlitter::new(&device);

        let color_copy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("color copy buffer"),
            size: EFB_WIDTH * EFB_HEIGHT * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let depth_copy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth copy buffer"),
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

            pipeline_settings: Default::default(),
            embedded_fb,
            external_fb,
            allocators,
            tex_slots: Default::default(),
            cleaner,
            color_blitter,
            depth_blitter,
            color_copy_buffer,
            depth_copy_buffer,
            color_copy_texture_pool: HashMap::default(),
            depth_copy_texture_pool: HashMap::default(),

            pipeline_cache,
            texture_cache,
            sampler_cache,
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
            Action::SetFramebufferFormat(fmt) => self.set_framebuffer_format(fmt),
            Action::SetViewport(viewport) => self.set_viewport(viewport),
            Action::SetScissor(scissor) => self.set_scissor(scissor),
            Action::SetCullingMode(mode) => self.set_culling_mode(mode),
            Action::SetClearColor(color) => self.set_clear_color(color),
            Action::SetClearDepth(depth) => self.clear_depth = depth,
            Action::SetBlendMode(mode) => self.set_blend_mode(mode),
            Action::SetDepthMode(mode) => self.set_depth_mode(mode),
            Action::SetAlphaFunction(func) => self.set_alpha_function(func),
            Action::SetConstantAlpha(mode) => self.set_constant_alpha_mode(mode),
            Action::SetProjectionMatrix(mat) => self.set_projection_mat(mat.value()),
            Action::SetTexEnvConfig(config) => self.set_texenv_config(config),
            Action::SetTexGenConfig(config) => self.set_texgen_config(config),
            Action::LoadTexture { id, texture } => self.load_texture(id, texture),
            Action::LoadClut { addr: id, clut } => self.load_clut(id, clut),
            Action::SetTextureSlot {
                slot,
                texture_id,
                sampler,
                scaling,
                clut_addr,
                clut_fmt,
            } => self.set_texture_slot(slot, texture_id, sampler, scaling, clut_addr, clut_fmt),
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
            Action::CopyColor { args, response } => self.copy_color(args, response),
            Action::CopyDepth { args, response } => self.copy_depth(args, response),
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

    fn insert_vertex(&mut self, vertex: &Vertex, matrices: &[(MatrixId, data::MatrixIdx)]) -> u32 {
        let get_matrix = |idx| matrices.iter().find_map(|(i, m)| (*i == idx).then_some(*m));
        let vertex = data::Vertex {
            position: vertex.position,
            config_idx: self.configs.len() as u32 - 1,
            normal: vertex.normal,
            _pad0: 0,

            position_mat: get_matrix(vertex.pos_norm_matrix).unwrap(),
            normal_mat: get_matrix(vertex.pos_norm_matrix.normal()).unwrap(),
            _pad1: 0,
            _pad2: 0,

            chan0: vertex.chan0,
            chan1: vertex.chan1,

            tex_coord: vertex.tex_coords,
            tex_coord_mat: seq! {
                N in 0..8 {
                    [#(get_matrix(vertex.tex_coords_matrix[N]).unwrap(),)*]
                }
            },
        };

        let idx = self.vertices.len();
        self.vertices.push(vertex);

        idx as u32
    }

    pub fn set_xfb_dimensions(&mut self, dims: Dimensions) {
        if dims == self.external_fb.dimensions() {
            return;
        }

        self.external_fb.resize(
            &self.device,
            wgpu::Extent3d {
                width: dims.width as u32,
                height: dims.height as u32,
                depth_or_array_layers: 1,
            },
        );

        let mut output = self.shared.output.lock().unwrap();
        *output = self.external_fb.framebuffer().clone();
    }

    pub fn set_framebuffer_format(&mut self, format: pix::BufferFormat) {
        self.flush(format_args!("framebuffer format changed to {format:?}"));

        match format {
            pix::BufferFormat::RGB8Z24 | pix::BufferFormat::RGB565Z16 => {
                self.pipeline_settings.has_alpha = false
            }
            pix::BufferFormat::RGBA6Z24 => self.pipeline_settings.has_alpha = true,
            _ => (),
        }
    }

    pub fn apply_scissor_and_viewport(&mut self) {
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

    pub fn set_viewport(&mut self, viewport: Viewport) {
        if self.viewport != viewport {
            self.flush(format_args!("changed viewport to {viewport:?}"));
            self.viewport = viewport;
        }
    }

    pub fn set_scissor(&mut self, scissor: Scissor) {
        if self.scissor != scissor {
            self.flush(format_args!("changed scissor to {scissor:?}"));
            self.scissor = scissor;
        }
    }

    pub fn set_culling_mode(&mut self, mode: CullingMode) {
        if self.pipeline_settings.culling != mode {
            self.flush(format_args!("changed culling mode to {mode:?}"));
            self.pipeline_settings.culling = mode;
        }
    }

    pub fn set_clear_color(&mut self, rgba: Rgba) {
        self.debug(format!("set clear color to {rgba:?}"));
        self.clear_color = rgba;
    }

    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        let src = match mode.src_factor() {
            SrcBlendFactor::Zero => wgpu::BlendFactor::Zero,
            SrcBlendFactor::One => wgpu::BlendFactor::One,
            SrcBlendFactor::DstColor => wgpu::BlendFactor::Dst,
            SrcBlendFactor::InverseDstColor => wgpu::BlendFactor::OneMinusDst,
            SrcBlendFactor::SrcAlpha => wgpu::BlendFactor::Src1Alpha,
            SrcBlendFactor::InverseSrcAlpha => wgpu::BlendFactor::OneMinusSrc1Alpha,
            SrcBlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
            SrcBlendFactor::InverseDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
        };

        let dst = match mode.dst_factor() {
            DstBlendFactor::Zero => wgpu::BlendFactor::Zero,
            DstBlendFactor::One => wgpu::BlendFactor::One,
            DstBlendFactor::SrcColor => wgpu::BlendFactor::Src1,
            DstBlendFactor::InverseSrcColor => wgpu::BlendFactor::OneMinusSrc1,
            DstBlendFactor::SrcAlpha => wgpu::BlendFactor::Src1Alpha,
            DstBlendFactor::InverseSrcAlpha => wgpu::BlendFactor::OneMinusSrc1Alpha,
            DstBlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
            DstBlendFactor::InverseDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
        };

        let op = if mode.blend_subtract() {
            wgpu::BlendOperation::Subtract
        } else {
            wgpu::BlendOperation::Add
        };

        let blend = pipeline::BlendSettings {
            enabled: mode.enable(),
            src,
            dst,
            op,
            color_write: mode.color_mask(),
            alpha_write: mode.alpha_mask(),
        };

        if self.pipeline_settings.blend != blend {
            self.flush(format_args!("set blend settings to {blend:?}"));
            self.pipeline_settings.blend = blend;
        }
    }

    pub fn set_depth_mode(&mut self, mode: DepthMode) {
        let compare = match mode.compare() {
            CompareMode::Never => wgpu::CompareFunction::Never,
            CompareMode::Less => wgpu::CompareFunction::Less,
            CompareMode::Equal => wgpu::CompareFunction::Equal,
            CompareMode::LessOrEqual => wgpu::CompareFunction::LessEqual,
            CompareMode::Greater => wgpu::CompareFunction::Greater,
            CompareMode::NotEqual => wgpu::CompareFunction::NotEqual,
            CompareMode::GreaterOrEqual => wgpu::CompareFunction::GreaterEqual,
            CompareMode::Always => wgpu::CompareFunction::Always,
        };

        let depth = pipeline::DepthSettings {
            enabled: mode.enable(),
            write: mode.update(),
            compare,
        };

        if self.pipeline_settings.depth != depth {
            self.flush(format_args!("set depth settings to {depth:?}"));
            self.pipeline_settings.depth = depth;
        }
    }

    pub fn set_alpha_function(&mut self, func: tev::alpha::Function) {
        let settings = pipeline::AlphaFuncSettings {
            comparison: func.comparison(),
            logic: func.logic(),
        };

        if self.pipeline_settings.shader.texenv.alpha_func != settings {
            self.flush(format_args!("set alpha function to {func:?}"));
            self.pipeline_settings.shader.texenv.alpha_func = settings;
        }

        self.current_config.alpha_refs = func.refs().map(|x| x as u32);
        self.current_config_dirty = true;
    }

    pub fn set_constant_alpha_mode(&mut self, mode: ConstantAlpha) {
        self.debug(format!("set constant alpha mode to {mode:?}"));
        self.current_config.constant_alpha = if mode.enabled() {
            mode.value() as u32
        } else {
            u32::MAX
        };
        self.current_config_dirty = true;
    }

    pub fn set_ambient(&mut self, idx: u8, color: Rgba) {
        self.current_config.ambient[idx as usize] = color;
        self.current_config_dirty = true;
    }

    pub fn set_material(&mut self, idx: u8, color: Rgba) {
        self.current_config.material[idx as usize] = color;
        self.current_config_dirty = true;
    }

    pub fn set_color_channel(&mut self, idx: u8, control: ChannelControl) {
        set_channel(
            &mut self.current_config.color_channels[idx as usize],
            control,
        );
        self.current_config_dirty = true;
    }

    pub fn set_alpha_channel(&mut self, idx: u8, control: ChannelControl) {
        set_channel(
            &mut self.current_config.alpha_channels[idx as usize],
            control,
        );
        self.current_config_dirty = true;
    }

    pub fn set_light(&mut self, idx: u8, light: Light) {
        let data = &mut self.current_config.lights[idx as usize];
        data.color = light.color.into();
        data.cos_attenuation = light.cos_attenuation;
        data.dist_attenuation = light.dist_attenuation;
        data.position = light.position;
        data.direction = light.direction;
        self.current_config_dirty = true;
    }

    pub fn set_projection_mat(&mut self, mat: Mat4) {
        self.current_config.projection_mat = mat;
        self.current_config_dirty = true;
    }

    pub fn set_texenv_config(&mut self, config: TexEnvConfig) {
        self.flush(format_args!("texenv changed"));
        self.pipeline_settings
            .shader
            .texenv
            .stages
            .clone_from(&config.stages);
        self.pipeline_settings.shader.texenv.depth_tex = config.depth_tex;
        self.current_config.consts = config.constants.map(Rgba::from);
        self.current_config_dirty = true;
    }

    pub fn set_texgen_config(&mut self, config: TexGenConfig) {
        self.flush(format_args!("texgen changed"));
        self.pipeline_settings.shader.texgen.stages = config
            .stages
            .iter()
            .map(|s| TexGenStageSettings {
                base: s.base.clone(),
                normalize: s.normalize,
            })
            .collect();

        for (setting, value) in self
            .current_config
            .post_transform_mat
            .iter_mut()
            .zip(config.stages.iter().map(|s| s.post_matrix))
        {
            *setting = value;
        }

        self.current_config_dirty = true;
    }

    pub fn load_texture(&mut self, id: TextureId, texture: Texture) {
        if self.texture_cache.update_raw(id, texture) {
            // HACK: avoid keeping old textures alive with a dependent bind group
            self.textures_group_cache.clear();
        }
    }

    pub fn load_clut(&mut self, id: ClutAddress, clut: Clut) {
        self.texture_cache.update_clut(id, clut);
    }

    pub fn set_texture_slot(
        &mut self,
        slot: usize,
        raw_id: TextureId,
        sampler: Sampler,
        scaling: Scaling,
        clut_addr: ClutAddress,
        clut_fmt: tex::ClutFormat,
    ) {
        let new = TexSlotSettings {
            settings: TextureSettings {
                raw_id,
                clut_addr,
                clut_fmt,
            },
            sampler,
            scaling,
        };

        if self.tex_slots[slot] == new {
            return;
        }

        self.flush(format_args!("texture slot changed"));
        self.tex_slots[slot] = new;
    }

    fn flush_config(&mut self) {
        if std::mem::take(&mut self.current_config_dirty) {
            self.debug("flushing config");
            self.configs.push(self.current_config.clone());
        }
    }

    fn create_matrix_indices(
        &mut self,
        matrices: &[(MatrixId, Mat4)],
    ) -> Vec<(MatrixId, data::MatrixIdx)> {
        let mut indices = Vec::with_capacity(matrices.len());

        for (id, mat) in matrices.iter().copied() {
            let idx = self.matrices.len();
            self.matrices.push(mat);
            indices.push((id, idx as u32));
        }

        indices
    }

    pub fn draw_quad_list(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
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

    pub fn draw_triangle_list(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
            return;
        }

        self.flush_config();
        let matrices = self.create_matrix_indices(matrices);
        for vertices in vertices.iter().array_chunks::<3>() {
            let vertices = vertices.map(|v| self.insert_vertex(v, &matrices));
            self.indices.extend_from_slice(&vertices);
        }
    }

    pub fn draw_triangle_strip(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
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

    pub fn draw_triangle_fan(&mut self, stream: &VertexStream) {
        let matrices = stream.matrices();
        let vertices = stream.vertices();

        if vertices.is_empty() {
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

    pub fn flush(&mut self, reason: std::fmt::Arguments) {
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
                .get(&self.device, &self.queue, s.settings)
                .clone()
        });

        let samplers = self
            .tex_slots
            .map(|s| self.sampler_cache.get(&self.device, s.sampler).clone());

        let textures_group = self.get_textures_group(TexturesGroupEntries { textures, samplers });

        self.apply_scissor_and_viewport();

        let pipeline = self
            .pipeline_cache
            .get(&self.device, &self.pipeline_settings);

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
    pub fn next_pass(&mut self) {
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

        self.shared.rendered_anything.store(true, Ordering::Relaxed);
    }

    pub fn get_color_data(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        half: bool,
    ) -> Vec<Rgba8> {
        let color = self.embedded_fb.color();

        let divisor = if half { 2 } else { 1 };
        let target_width = width as u32 / divisor;
        let target_height = height as u32 / divisor;
        let size = wgpu::Extent3d {
            width: target_width,
            height: target_height,
            depth_or_array_layers: 1,
        };

        let row_size = target_width * 4;
        let row_stride = row_size.next_multiple_of(256);

        let copy_target = match self.color_copy_texture_pool.entry(size) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("color copy texture"),
                    dimension: wgpu::TextureDimension::D2,
                    size,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                    mip_level_count: 1,
                    sample_count: 1,
                });

                v.insert(tex.create_view(&wgpu::TextureViewDescriptor::default()))
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.color_blitter.blit_to_texture(
            &self.device,
            color,
            wgpu::Origin3d {
                x: x as u32,
                y: y as u32,
                z: 0,
            },
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
            copy_target,
            &mut encoder,
        );

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: copy_target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.color_copy_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_stride),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: target_width,
                height: target_height,
                depth_or_array_layers: 1,
            },
        );

        let (sender, receiver) = oneshot::channel();
        encoder.map_buffer_on_submit(&self.color_copy_buffer, wgpu::MapMode::Read, .., |r| {
            sender.send(r).unwrap()
        });

        let cmd = encoder.finish();
        let submission = self.queue.submit([cmd]);
        self.device
            .poll(wgpu::wgt::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();

        let result = receiver.recv().unwrap();
        result.unwrap();

        let mapped = self.color_copy_buffer.get_mapped_range(..);
        let data = &*mapped;

        let mut pixels = Vec::with_capacity(target_width as usize * target_height as usize);
        for row in 0..target_height as usize {
            let row_data = &data[row * row_stride as usize..][..row_size as usize];
            pixels.extend(
                row_data
                    .chunks_exact(4)
                    .map(Rgba8::read_from_bytes)
                    .map(Result::unwrap),
            );
        }

        std::mem::drop(mapped);
        self.color_copy_buffer.unmap();

        pixels
    }

    pub fn get_depth_data(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        half: bool,
    ) -> Vec<u32> {
        let depth = self.embedded_fb.depth();

        let divisor = if half { 2 } else { 1 };
        let target_width = width as u32 / divisor;
        let target_height = height as u32 / divisor;
        let size = wgpu::Extent3d {
            width: target_width,
            height: target_height,
            depth_or_array_layers: 1,
        };

        let row_size = target_width * 4;
        let row_stride = row_size.next_multiple_of(256);

        let copy_target = match self.depth_copy_texture_pool.entry(size) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth copy texture"),
                    dimension: wgpu::TextureDimension::D2,
                    size,
                    format: wgpu::TextureFormat::R32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                    mip_level_count: 1,
                    sample_count: 1,
                });

                v.insert(tex.create_view(&wgpu::TextureViewDescriptor::default()))
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.depth_blitter.blit_to_texture(
            &self.device,
            depth,
            wgpu::Origin3d::ZERO,
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
            copy_target,
            &mut encoder,
        );

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: copy_target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: x as u32,
                    y: y as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.depth_copy_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_stride),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: target_width,
                height: target_height,
                depth_or_array_layers: 1,
            },
        );

        let (sender, receiver) = oneshot::channel();
        encoder.map_buffer_on_submit(&self.depth_copy_buffer, wgpu::MapMode::Read, .., |r| {
            sender.send(r).unwrap()
        });

        let cmd = encoder.finish();
        let submission = self.queue.submit([cmd]);
        self.device
            .poll(wgpu::wgt::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();

        let result = receiver.recv().unwrap();
        result.unwrap();

        let mapped = self.depth_copy_buffer.get_mapped_range(..);
        let data = &*mapped;

        let mut depth = Vec::with_capacity(target_width as usize * target_height as usize);
        for row in 0..target_height as usize {
            let row_data = &data[row * row_stride as usize..][..row_size as usize];
            depth.extend(row_data.chunks_exact(4).map(|c| {
                let value = f32::read_from_bytes(c).unwrap();

                assert!(value >= 0.0f32);
                assert!(value <= 1.0f32);

                (value * DEPTH_24_BIT_MAX as f32) as u32
            }));
        }

        std::mem::drop(mapped);
        self.depth_copy_buffer.unmap();

        depth
    }

    pub fn clear(&mut self, x: u32, y: u32, width: u32, height: u32) {
        let color = self
            .pipeline_settings
            .blend
            .color_write
            .then_some(self.clear_color);
        let depth = self
            .pipeline_settings
            .depth
            .write
            .then_some(self.clear_depth);

        self.current_pass.set_scissor_rect(x, y, width, height);
        self.current_pass
            .set_viewport(0.0, 0.0, 640.0, 528.0, 0.0, 1.0);
        self.cleaner
            .clear_target(color, depth, &mut self.current_pass);
    }

    pub fn copy_color(&mut self, args: CopyArgs, response: oneshot::Sender<Vec<Rgba8>>) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        self.debug(format!(
            "color copy requested: ({}, {}) [{}x{}] (mip: {})",
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half
        ));

        self.next_pass();
        let data = self.get_color_data(
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half,
        );
        response.send(data).unwrap();

        if clear {
            self.clear(
                src.x().value() as u32,
                src.y().value() as u32,
                dims.width() as u32,
                dims.height() as u32,
            );
        }
    }

    pub fn copy_depth(&mut self, args: CopyArgs, response: oneshot::Sender<Vec<u32>>) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        self.debug(format!(
            "depth copy requested: ({}, {}) [{}x{}] (mip: {})",
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half
        ));

        self.next_pass();
        let data = self.get_depth_data(
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half,
        );
        response.send(data).unwrap();

        if clear {
            self.clear(
                src.x().value() as u32,
                src.y().value() as u32,
                dims.width() as u32,
                dims.height() as u32,
            );
        }
    }

    pub fn copy_xfb(&mut self, args: CopyArgs, id: u32) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        assert!(!half);

        self.debug("XFB copy requested");
        self.next_pass();

        let x = src.x().value() as u32;
        let y = src.y().value() as u32;
        let width = dims.width() as u32;
        let height = dims.height() as u32;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let color = self.embedded_fb.color();
        let target = self.external_fb.create_copy(&self.device, id, size);

        self.current_transfer_encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: color.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::default(),
            },
            size,
        );

        if clear {
            self.clear(x, y, width, height);
        }
    }

    pub fn present_xfb(&mut self, parts: Vec<XfbPart>) {
        self.external_fb
            .build(&mut self.current_transfer_encoder, parts);

        self.next_pass();
    }
}
