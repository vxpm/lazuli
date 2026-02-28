use lazuli::system::gx::CullingMode;

use crate::render::pipeline::shader;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlendConfig {
    pub enabled: bool,
    pub src: wgpu::BlendFactor,
    pub dst: wgpu::BlendFactor,
    pub op: wgpu::BlendOperation,

    pub color_write: bool,
    pub alpha_write: bool,
}

impl Default for BlendConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            src: wgpu::BlendFactor::Src,
            dst: wgpu::BlendFactor::Dst,
            op: wgpu::BlendOperation::Add,

            color_write: true,
            alpha_write: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DepthConfig {
    pub enabled: bool,
    pub compare: wgpu::CompareFunction,
    pub write: bool,
}

impl Default for DepthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compare: wgpu::CompareFunction::Less,
            write: true,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Config {
    pub has_alpha: bool,
    pub culling: CullingMode,
    pub blend: BlendConfig,
    pub depth: DepthConfig,
    pub shader: shader::Config,
}
