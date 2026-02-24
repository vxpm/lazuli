mod settings;
mod shader;

use lazuli::modules::render::{TexEnvConfig, TexGenConfig};
use lazuli::system::gx::color::Rgba;
use lazuli::system::gx::pix::{
    BlendLogicOp, BlendMode, CompareMode, DepthMode, DstBlendFactor, SrcBlendFactor,
};
use lazuli::system::gx::{CullingMode, tev};

use crate::render::Renderer;

#[rustfmt::skip]
pub use settings::*;

mod cache {
    use std::borrow::Cow;
    use std::collections::hash_map::Entry;

    use lazuli::system::gx::CullingMode;
    use rustc_hash::FxHashMap;

    use super::{Settings, ShaderSettings};

    pub struct Cache {
        group0_layout: wgpu::BindGroupLayout,
        group1_layout: wgpu::BindGroupLayout,
        layout: wgpu::PipelineLayout,
        cached_pipelines: FxHashMap<Settings, wgpu::RenderPipeline>,
        cached_shaders: FxHashMap<ShaderSettings, wgpu::ShaderModule>,
    }

    fn split_factor(factor: wgpu::BlendFactor) -> (wgpu::BlendFactor, wgpu::BlendFactor) {
        match factor {
            wgpu::BlendFactor::Src1 => (wgpu::BlendFactor::Src1, wgpu::BlendFactor::Src1Alpha),
            wgpu::BlendFactor::Dst => (wgpu::BlendFactor::Dst, wgpu::BlendFactor::DstAlpha),
            wgpu::BlendFactor::OneMinusSrc1 => (
                wgpu::BlendFactor::OneMinusSrc1,
                wgpu::BlendFactor::OneMinusSrc1Alpha,
            ),
            wgpu::BlendFactor::OneMinusDst => (
                wgpu::BlendFactor::OneMinusDst,
                wgpu::BlendFactor::OneMinusDstAlpha,
            ),
            _ => (factor, factor),
        }
    }

    fn remove_dst_alpha(factor: wgpu::BlendFactor) -> wgpu::BlendFactor {
        match factor {
            wgpu::BlendFactor::DstAlpha => wgpu::BlendFactor::One,
            wgpu::BlendFactor::OneMinusDstAlpha => wgpu::BlendFactor::Zero,
            _ => factor,
        }
    }

    impl Cache {
        fn create_pipeline(
            cached_shaders: &mut FxHashMap<ShaderSettings, wgpu::ShaderModule>,
            device: &wgpu::Device,
            layout: &wgpu::PipelineLayout,
            settings: &Settings,
            id: u32,
        ) -> wgpu::RenderPipeline {
            let depth_stencil = if settings.depth.enabled {
                wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: settings.depth.write,
                    depth_compare: settings.depth.compare,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }
            } else {
                wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }
            };

            let (color_src, alpha_src) = split_factor(settings.blend.src);
            let (color_dst, alpha_dst) = split_factor(settings.blend.dst);

            let (color_blend, alpha_blend) = if settings.has_alpha {
                let color = wgpu::BlendComponent {
                    src_factor: color_src,
                    dst_factor: color_dst,
                    operation: settings.blend.op,
                };
                let alpha = wgpu::BlendComponent {
                    src_factor: alpha_src,
                    dst_factor: alpha_dst,
                    operation: settings.blend.op,
                };

                (color, alpha)
            } else {
                let color = wgpu::BlendComponent {
                    src_factor: remove_dst_alpha(color_src),
                    dst_factor: remove_dst_alpha(color_dst),
                    operation: settings.blend.op,
                };
                let alpha = wgpu::BlendComponent {
                    src_factor: remove_dst_alpha(alpha_src),
                    dst_factor: remove_dst_alpha(alpha_dst),
                    operation: settings.blend.op,
                };

                (color, alpha)
            };

            let blend = settings.blend.enabled.then_some(wgpu::BlendState {
                color: color_blend,
                alpha: alpha_blend,
            });

            let mut write_mask = wgpu::ColorWrites::empty();
            if settings.blend.color_write {
                write_mask |= wgpu::ColorWrites::COLOR;
            }
            if settings.blend.alpha_write && settings.has_alpha {
                write_mask |= wgpu::ColorWrites::ALPHA;
            }

            let label = format!("Shader {}", id);
            let shader = match cached_shaders.entry(settings.shader.clone()) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => {
                    let shader = super::shader::compile(&settings.shader);
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(&label),
                        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
                    });

                    v.insert(module)
                }
            };

            let cull_mode = match settings.culling {
                CullingMode::None => None,
                CullingMode::Back => Some(wgpu::Face::Back),
                CullingMode::Front => Some(wgpu::Face::Front),
                CullingMode::All => {
                    tracing::warn!("culling mode all is not supported - culling back faces only");
                    Some(wgpu::Face::Back)
                }
            };

            let label = format!("Pipeline {}", id);
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&label),
                layout: Some(layout),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend,
                        write_mask,
                    })],
                }),
                multisample: wgpu::MultisampleState {
                    count: 4,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                depth_stencil: Some(depth_stencil),
                multiview: None,
                cache: None,
            })
        }

        pub fn new(device: &wgpu::Device) -> Self {
            let storage_buffer = |binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            };
            let group0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    // vertices
                    storage_buffer(0),
                    // matrices
                    storage_buffer(1),
                    // configs
                    storage_buffer(2),
                ],
            });

            let tex = |binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            };

            let sampler = |binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            };

            let mut current_binding = 0;
            let mut entries = Vec::with_capacity(2 * 8);
            for _ in 0..8 {
                entries.push(tex(current_binding));
                entries.push(sampler(current_binding + 1));
                current_binding += 2;
            }

            let group1_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &entries,
            });

            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&group0_layout, &group1_layout],
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 0..96,
                }],
            });

            Self {
                group0_layout,
                group1_layout,
                layout,
                cached_pipelines: Default::default(),
                cached_shaders: Default::default(),
            }
        }

        pub fn data_group_layout(&self) -> &wgpu::BindGroupLayout {
            &self.group0_layout
        }

        pub fn textures_group_layout(&self) -> &wgpu::BindGroupLayout {
            &self.group1_layout
        }

        pub fn get(&mut self, device: &wgpu::Device, settings: &Settings) -> &wgpu::RenderPipeline {
            let len = self.cached_pipelines.len() as u32;
            match self.cached_pipelines.entry(settings.clone()) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(Self::create_pipeline(
                    &mut self.cached_shaders,
                    device,
                    &self.layout,
                    settings,
                    len,
                )),
            }
        }
    }
}

pub use cache::Cache;

fn logic_blend_approx(
    logic: BlendLogicOp,
) -> (wgpu::BlendFactor, wgpu::BlendFactor, wgpu::BlendOperation) {
    use wgpu::{BlendFactor as Factor, BlendOperation as Op};

    match logic {
        BlendLogicOp::Clear => (Factor::Zero, Factor::Zero, Op::Add),
        BlendLogicOp::And => (Factor::Zero, Factor::Src1, Op::Add),
        BlendLogicOp::ReverseAnd => (Factor::OneMinusSrc1, Factor::Zero, Op::Add),
        BlendLogicOp::Copy => (Factor::One, Factor::Zero, Op::Add),
        BlendLogicOp::InverseAnd => (Factor::Zero, Factor::OneMinusSrc1, Op::Add),
        BlendLogicOp::Noop => (Factor::Zero, Factor::One, Op::Add),
        BlendLogicOp::Xor => (Factor::OneMinusDst, Factor::OneMinusSrc1, Op::Max),
        BlendLogicOp::Or => (Factor::One, Factor::OneMinusSrc1, Op::Add),
        BlendLogicOp::Nor => (Factor::OneMinusSrc1, Factor::OneMinusDst, Op::Max),
        BlendLogicOp::Equiv => (Factor::OneMinusSrc1, Factor::Src1, Op::Max),
        BlendLogicOp::Inverse => (Factor::OneMinusDst, Factor::OneMinusDst, Op::Add),
        BlendLogicOp::ReverseOr => (Factor::One, Factor::OneMinusDst, Op::Add),
        BlendLogicOp::InverseCopy => (Factor::OneMinusSrc1, Factor::OneMinusSrc1, Op::Add),
        BlendLogicOp::InverseOr => (Factor::OneMinusSrc1, Factor::One, Op::Add),
        BlendLogicOp::Nand => (Factor::OneMinusDst, Factor::OneMinusSrc1, Op::Add),
        BlendLogicOp::Set => (Factor::One, Factor::One, Op::Add),
    }
}

impl Renderer {
    pub fn set_texenv_config(&mut self, config: TexEnvConfig) {
        self.flush(format_args!("texenv changed"));
        self.pipeline_settings
            .shader
            .texenv
            .stages
            .clone_from(&config.stages);
        self.pipeline_settings.shader.texenv.depth_tex = config.depth_tex;
        self.current_config.regs = config.regs.map(Rgba::from);
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

    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        let (src, dst, op) = if mode.logic_op_enable() {
            logic_blend_approx(mode.logic_op())
        } else if mode.blend_subtract() {
            (
                wgpu::BlendFactor::One,
                wgpu::BlendFactor::One,
                wgpu::BlendOperation::ReverseSubtract,
            )
        } else {
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

            (src, dst, wgpu::BlendOperation::Add)
        };

        let blend = BlendSettings {
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

        let depth = DepthSettings {
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
        let settings = AlphaFuncSettings {
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

    pub fn set_culling_mode(&mut self, mode: CullingMode) {
        if self.pipeline_settings.culling != mode {
            self.flush(format_args!("changed culling mode to {mode:?}"));
            self.pipeline_settings.culling = mode;
        }
    }
}
