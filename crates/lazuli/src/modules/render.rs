//! Renderer module interface.

use color::{Abgr8, Rgba, Rgba8, Rgba16};
use glam::Mat4;
use oneshot::Sender;
use ordered_float::OrderedFloat;
use static_assertions::const_assert;

use crate::system::gx::pix::{
    BlendMode, BufferFormat, ConstantAlpha, CopyDims, CopySrc, DepthMode, Scissor,
};
use crate::system::gx::tev::{AlphaFunction, Constant, DepthTexture, StageOps, StageRefs};
use crate::system::gx::tex::{ClutFormat, Format, LodLimits, MipmapData, SamplerMode};
use crate::system::gx::xform::{BaseTexGen, ChannelControl, Light, ProjectionMat};
use crate::system::gx::{CullingMode, EFB_HEIGHT, EFB_WIDTH, Topology, VertexStream};

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
    pub ops: StageOps,
    pub refs: StageRefs,
    pub color_const: Constant,
    pub alpha_const: Constant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexEnvConfig {
    pub stages: Vec<TexEnvStage>,
    pub constants: [Rgba16; 4],
    pub depth_tex: DepthTexture,
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
    pub format: Format,
    pub data: MipmapData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Sampler {
    pub mode: SamplerMode,
    pub lods: LodLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scaling {
    pub u: f32,
    pub v: f32,
}

#[derive(Debug, Clone)]
pub struct Clut(pub Vec<u16>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClutAddress(pub u16);

impl ClutAddress {
    /// Returns the address of this CLUT in the high bank of TMEM, assuming 16-bit addressing.
    pub fn to_tmem_addr(&self) -> usize {
        // the offset is in multiples of the CLUT length. since each CLUT has 16 entries that are
        // replicated 16 times, the CLUT length is 256 16-bit words
        self.0 as usize * 256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyArgs {
    pub src: CopySrc,
    pub dims: CopyDims,
    pub half: bool,
    pub clear: bool,
}

pub enum Action {
    SetFramebufferFormat(BufferFormat),
    SetViewport(Viewport),
    SetScissor(Scissor),
    SetCullingMode(CullingMode),
    SetClearColor(Rgba),
    SetClearDepth(f32),
    SetDepthMode(DepthMode),
    SetBlendMode(BlendMode),
    SetConstantAlpha(ConstantAlpha),
    SetAlphaFunction(AlphaFunction),
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
        addr: ClutAddress,
        clut: Clut,
    },
    SetTextureSlot {
        slot: usize,
        texture_id: TextureId,
        sampler: Sampler,
        scaling: Scaling,
        clut_addr: ClutAddress,
        clut_fmt: ClutFormat,
    },
    Draw(Topology, VertexStream),
    ColorCopy {
        args: CopyArgs,
        response: Sender<Vec<Rgba8>>,
    },
    DepthCopy {
        args: CopyArgs,
        response: Sender<Vec<u32>>,
    },
    XfbCopy {
        clear: bool,
    },
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
