//! Renderer module interface.

use color::{Abgr8, Rgba, Rgba16};
use glam::Mat4;
use oneshot::Sender;
use ordered_float::OrderedFloat;
use static_assertions::const_assert;

use crate::system::gx::pix::{
    BlendMode, BufferFormat, ColorCopyFormat, ConstantAlpha, CopyDims, CopySrc, DepthCopyFormat,
    DepthMode, Scissor,
};
use crate::system::gx::xform::{BaseTexGen, ChannelControl, Light, ProjectionMat};
use crate::system::gx::{CullingMode, EFB_HEIGHT, EFB_WIDTH, Topology, VertexStream, tev, tex};
use crate::system::vi::Dimensions;

#[rustfmt::skip]
pub use oneshot;

/// Wrapper around a [`Mat4`] that allows hashing through [`OrderedFloat`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HashableMat4([OrderedFloat<f32>; 16]);

impl From<Mat4> for HashableMat4 {
    #[inline(always)]
    fn from(value: Mat4) -> Self {
        // SAFETY: this is safe because OrderedFloat is repr(transparent)
        Self(unsafe {
            std::mem::transmute::<[f32; 16], [OrderedFloat<f32>; 16]>(value.to_cols_array())
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub top_left_x: f32,
    pub top_left_y: f32,
    pub near_depth: f32,
    pub far_depth: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: EFB_WIDTH as f32,
            height: EFB_HEIGHT as f32,
            top_left_x: 0.0,
            top_left_y: 0.0,
            near_depth: 0.0,
            far_depth: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexEnvStage {
    pub ops: tev::StageOps,
    pub refs: tev::StageRefs,
    pub color_const: tev::Constant,
    pub alpha_const: tev::Constant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexEnvConfig {
    pub stages: Vec<TexEnvStage>,
    pub constants: [Rgba16; 4],
    pub depth_tex: tev::depth::Texture,
}

#[derive(Debug, Clone, Default)]
pub struct TexGenStage {
    pub base: BaseTexGen,
    pub normalize: bool,
    pub post_matrix: Mat4,
}

impl PartialEq for TexGenStage {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.normalize == other.normalize
            && HashableMat4::from(self.post_matrix) == HashableMat4::from(other.post_matrix)
    }
}

impl Eq for TexGenStage {}

impl std::hash::Hash for TexGenStage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.base.hash(state);
        self.normalize.hash(state);
        HashableMat4::from(self.post_matrix).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexGenConfig {
    pub stages: Vec<TexGenStage>,
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub format: tex::Format,
    pub data: tex::TextureData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Sampler {
    pub mode: tex::SamplerMode,
    pub lods: tex::LodLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scaling {
    pub u: f32,
    pub v: f32,
}

#[derive(Debug, Clone)]
pub struct ClutData(pub Vec<u16>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClutId(pub u16);

impl ClutId {
    /// Returns the address of this CLUT in the high bank of TMEM, assuming 16-bit addressing.
    pub fn to_tmem_addr(&self) -> usize {
        // the offset is in multiples of the minimum CLUT length. since each CLUT has at least 16
        // entries that are replicated 16 times, the minimum CLUT length is 256 16-bit words
        self.0 as usize * 256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClutRef {
    pub id: ClutId,
    pub fmt: tex::ClutFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyArgs {
    pub src: CopySrc,
    pub dims: CopyDims,
    pub half: bool,
    pub clear: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct XfbPart {
    pub id: u32,
    pub offset_x: u32,
    pub offset_y: u32,
}

/// A vector of texture data (i.e. it's texels). For color textures, the data is encoded as
/// RGBA8. For depth textures, it's encoded as a F32 (little-endian).
pub type Texels = Vec<u32>;

pub enum Action {
    SetXfbDimensions(Dimensions),
    SetFramebufferFormat(BufferFormat),
    SetViewport(Viewport),
    SetScissor(Scissor),
    SetCullingMode(CullingMode),
    SetClearColor(Rgba),
    SetClearDepth(f32),
    SetDepthMode(DepthMode),
    SetBlendMode(BlendMode),
    SetConstantAlpha(ConstantAlpha),
    SetAlphaFunction(tev::alpha::Function),
    SetProjectionMatrix(ProjectionMat),
    SetTexEnvConfig(TexEnvConfig),
    SetTexGenConfig(TexGenConfig),
    SetAmbient(u8, Abgr8),
    SetMaterial(u8, Abgr8),
    SetColorChannel(u8, ChannelControl),
    SetAlphaChannel(u8, ChannelControl),
    SetLight(u8, Light),
    LoadTexture {
        texture: Texture,
        id: TextureId,
    },
    LoadClut {
        clut: ClutData,
        id: ClutId,
    },
    SetTextureSlot {
        slot: usize,
        texture_id: TextureId,
        clut_ref: ClutRef,
        sampler: Sampler,
        scaling: Scaling,
    },
    Draw(Topology, VertexStream),
    CopyColor {
        args: CopyArgs,
        format: ColorCopyFormat,
        response: Option<Sender<Texels>>,
        id: TextureId,
    },
    CopyDepth {
        args: CopyArgs,
        format: DepthCopyFormat,
        response: Option<Sender<Texels>>,
        id: TextureId,
    },
    CopyXfb {
        args: CopyArgs,
        id: u32,
    },
    PresentXfb(Vec<XfbPart>),
}

const_assert!(size_of::<Action>() <= 64);

pub trait RenderModule: Send {
    fn exec(&mut self, action: Action);
}

/// An implementation of [`RenderModule`] that does nothing.
#[derive(Debug, Clone, Copy)]
pub struct NopRenderModule;

impl RenderModule for NopRenderModule {
    fn exec(&mut self, _: Action) {}
}
