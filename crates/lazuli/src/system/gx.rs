//! Graphics subsystem (GX).
pub mod cmd;
pub mod pix;
pub mod tev;
pub mod tex;
pub mod xform;

use std::num::NonZero;
use std::sync::{LazyLock, Mutex};

use bitos::integer::{UnsignedInt, u3, u4};
use bitos::{BitUtils, TryBits, bitos};
use bitvec::array::BitArray;
use color::Rgba;
use gekko::Address;
use glam::{Mat4, Vec2, Vec3};
use ring_arena::{Handle, RingArena};
use seq_macro::seq;
use strum::FromRepr;
use zerocopy::IntoBytes;

use crate::modules::{render, vertex};
use crate::system::gx::cmd::VertexAttributeStream;
use crate::system::pi;
use crate::{Primitive, System};

#[rustfmt::skip]
pub use color;
#[rustfmt::skip]
pub use glam;

/// Maximum value for the 24-bit depth.
pub const DEPTH_24_BIT_MAX: u32 = (1 << 24) - 1;
pub const EFB_WIDTH: u64 = 640;
pub const EFB_HEIGHT: u64 = 528;

/// An internal GX register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub enum Reg {
    GenMode             = 0x00,
    GenFilter0          = 0x01,
    GenFilter1          = 0x02,
    GenFilter2          = 0x03,
    GenFilter3          = 0x04,

    IndMtxA0            = 0x06,
    IndMtxB0            = 0x07,
    IndMtxC0            = 0x08,
    IndMtxA1            = 0x09,
    IndMtxB1            = 0x0A,
    IndMtxC1            = 0x0B,
    IndMtxA2            = 0x0C,
    IndMtxB2            = 0x0D,
    IndMtxC2            = 0x0E,

    BumpIMask           = 0x0F,

    IndCmd0             = 0x10,
    IndCmd1             = 0x11,
    IndCmd2             = 0x12,
    IndCmd3             = 0x13,
    IndCmd4             = 0x14,
    IndCmd5             = 0x15,
    IndCmd6             = 0x16,
    IndCmd7             = 0x17,
    IndCmd8             = 0x18,
    IndCmd9             = 0x19,
    IndCmd10            = 0x1A,
    IndCmd11            = 0x1B,
    IndCmd12            = 0x1C,
    IndCmd13            = 0x1D,
    IndCmd14            = 0x1E,
    IndCmd15            = 0x1F,

    ScissorTopLeft      = 0x20,
    ScissorBottomRight  = 0x21,

    SetupLpSize         = 0x22,
    SetupPerf           = 0x23,
    RasterPerf          = 0x24,
    RasterSs0           = 0x25,
    RasterSs1           = 0x26,
    RasterIRef          = 0x27,

    TevRefs01           = 0x28,
    TevRefs23           = 0x29,
    TevRefs45           = 0x2A,
    TevRefs67           = 0x2B,
    TevRefs89           = 0x2C,
    TevRefsAB           = 0x2D,
    TevRefsCD           = 0x2E,
    TevRefsEF           = 0x2F,

    TexScaleU0          = 0x30,
    TexScaleV0          = 0x31,
    TexScaleU1          = 0x32,
    TexScaleV1          = 0x33,
    TexScaleU2          = 0x34,
    TexScaleV2          = 0x35,
    TexScaleU3          = 0x36,
    TexScaleV3          = 0x37,
    TexScaleU4          = 0x38,
    TexScaleV4          = 0x39,
    TexScaleU5          = 0x3A,
    TexScaleV5          = 0x3B,
    TexScaleU6          = 0x3C,
    TexScaleV6          = 0x3D,
    TexScaleU7          = 0x3E,
    TexScaleV7          = 0x3F,

    PixelZMode          = 0x40,
    PixelBlendMode      = 0x41,
    PixelConstantAlpha  = 0x42,
    PixelControl        = 0x43,
    PixelFieldMask      = 0x44,
    PixelDone           = 0x45,
    PixelRefresh        = 0x46,
    PixelToken          = 0x47,
    PixelTokenInt       = 0x48,
    PixelCopySrc        = 0x49, // texCopyTL
    PixelCopyDimensions = 0x4A, // texCopyHW
    PixelCopyDst        = 0x4B, // not a register in libogc, set manually
    PixelCopyDstStride  = 0x4D, // texCopyDst
    PixelCopyScale      = 0x4E,
    PixelCopyClearAr    = 0x4F,
    PixelCopyClearGb    = 0x50,
    PixelCopyClearZ     = 0x51,
    PixelCopyCmd        = 0x52, // texCopyCtrl
    PixelCopyFilter0    = 0x53,
    PixelCopyFilter1    = 0x54,
    PixelXBound         = 0x55,
    PixelYBound         = 0x56,
    PixelPerfMode       = 0x57,
    PixelChicken        = 0x58,
    ScissorOffset       = 0x59,

    TexLoadBlock0       = 0x60,
    TexLoadBlock1       = 0x61,
    TexLoadBlock2       = 0x62,
    TexLoadBlock3       = 0x63,
    TexLutAddress       = 0x64,
    TexLutLoad          = 0x65,
    TexInvTags          = 0x66,
    TexPerfMode         = 0x67,
    TexFieldMode        = 0x68,
    TexRefresh          = 0x69,

    TexSampler0         = 0x80,
    TexSampler1         = 0x81,
    TexSampler2         = 0x82,
    TexSampler3         = 0x83,
    TexLod0             = 0x84,
    TexLod1             = 0x85,
    TexLod2             = 0x86,
    TexLod3             = 0x87,
    TexFormat0          = 0x88,
    TexFormat1          = 0x89,
    TexFormat2          = 0x8A,
    TexFormat3          = 0x8B,
    TexEvenLodAddress0  = 0x8C,
    TexEvenLodAddress1  = 0x8D,
    TexEvenLodAddress2  = 0x8E,
    TexEvenLodAddress3  = 0x8F,
    TexOddLodAddress0   = 0x90,
    TexOddLodAddress1   = 0x91,
    TexOddLodAddress2   = 0x92,
    TexOddLodAddress3   = 0x93,
    TexAddress0         = 0x94,
    TexAddress1         = 0x95,
    TexAddress2         = 0x96,
    TexAddress3         = 0x97,
    TexLutRef0          = 0x98,
    TexLutRef1          = 0x99,
    TexLutRef2          = 0x9A,
    TexLutRef3          = 0x9B,

    TexSampler4         = 0xA0,
    TexSampler5         = 0xA1,
    TexSampler6         = 0xA2,
    TexSampler7         = 0xA3,
    TexLod4             = 0xA4,
    TexLod5             = 0xA5,
    TexLod6             = 0xA6,
    TexLod7             = 0xA7,
    TexFormat4          = 0xA8,
    TexFormat5          = 0xA9,
    TexFormat6          = 0xAA,
    TexFormat7          = 0xAB,
    TexEvenLodAddress4  = 0xAC,
    TexEvenLodAddress5  = 0xAD,
    TexEvenLodAddress6  = 0xAE,
    TexEvenLodAddress7  = 0xAF,
    TexOddLodAddress4   = 0xB0,
    TexOddLodAddress5   = 0xB1,
    TexOddLodAddress6   = 0xB2,
    TexOddLodAddress7   = 0xB3,
    TexAddress4         = 0xB4,
    TexAddress5         = 0xB5,
    TexAddress6         = 0xB6,
    TexAddress7         = 0xB7,
    TexLutRef4          = 0xB8,
    TexLutRef5          = 0xB9,
    TexLutRef6          = 0xBA,
    TexLutRef7          = 0xBB,

    TevColor0           = 0xC0,
    TevAlpha0           = 0xC1,
    TevColor1           = 0xC2,
    TevAlpha1           = 0xC3,
    TevColor2           = 0xC4,
    TevAlpha2           = 0xC5,
    TevColor3           = 0xC6,
    TevAlpha3           = 0xC7,
    TevColor4           = 0xC8,
    TevAlpha4           = 0xC9,
    TevColor5           = 0xCA,
    TevAlpha5           = 0xCB,
    TevColor6           = 0xCC,
    TevAlpha6           = 0xCD,
    TevColor7           = 0xCE,
    TevAlpha7           = 0xCF,
    TevColor8           = 0xD0,
    TevAlpha8           = 0xD1,
    TevColor9           = 0xD2,
    TevAlpha9           = 0xD3,
    TevColor10          = 0xD4,
    TevAlpha10          = 0xD5,
    TevColor11          = 0xD6,
    TevAlpha11          = 0xD7,
    TevColor12          = 0xD8,
    TevAlpha12          = 0xD9,
    TevColor13          = 0xDA,
    TevAlpha13          = 0xDB,
    TevColor14          = 0xDC,
    TevAlpha14          = 0xDD,
    TevColor15          = 0xDE,
    TevAlpha15          = 0xDF,

    TevConstant3AR      = 0xE0,
    TevConstant3GB      = 0xE1,
    TevConstant0AR      = 0xE2,
    TevConstant0GB      = 0xE3,
    TevConstant1AR      = 0xE4,
    TevConstant1GB      = 0xE5,
    TevConstant2AR      = 0xE6,
    TevConstant2GB      = 0xE7,

    TevRangeAdjC        = 0xE8,
    TevRangeAdj0        = 0xE9,
    TevRangeAdj1        = 0xEA,
    TevRangeAdj2        = 0xEB,
    TevRangeAdj3        = 0xEC,
    TevRangeAdj4        = 0xED,

    TevFog0             = 0xEE,
    TevFog1             = 0xEF,
    TevFog2             = 0xF0,
    TevFog3             = 0xF1,
    TevFogColor         = 0xF2,

    TevAlphaFunc        = 0xF3,
    TevDepthTexBias     = 0xF4,
    TevDepthTexMode     = 0xF5,
    TevKSel0            = 0xF6,
    TevKSel1            = 0xF7,
    TevKSel2            = 0xF8,
    TevKSel3            = 0xF9,
    TevKSel4            = 0xFA,
    TevKSel5            = 0xFB,
    TevKSel6            = 0xFC,
    TevKSel7            = 0xFD,

    WriteMask           = 0xFE,
}

impl Reg {
    #[inline]
    pub fn texmap(&self) -> Option<u8> {
        seq! {
            N in 0..8 {
                match self {
                    #(
                          Self::TexScaleU~N
                        | Self::TexScaleV~N
                        | Self::TexSampler~N
                        | Self::TexLod~N
                        | Self::TexFormat~N
                        | Self::TexEvenLodAddress~N
                        | Self::TexOddLodAddress~N
                        | Self::TexAddress~N
                        | Self::TexLutRef~N
                        => Some(N),
                    )*
                    _ => None
                }
            }
        }
    }

    #[inline]
    pub fn is_tev(&self) -> bool {
        matches!(
            self,
            Self::TevColor0
                | Self::TevAlpha0
                | Self::TevColor1
                | Self::TevAlpha1
                | Self::TevColor2
                | Self::TevAlpha2
                | Self::TevColor3
                | Self::TevAlpha3
                | Self::TevColor4
                | Self::TevAlpha4
                | Self::TevColor5
                | Self::TevAlpha5
                | Self::TevColor6
                | Self::TevAlpha6
                | Self::TevColor7
                | Self::TevAlpha7
                | Self::TevColor8
                | Self::TevAlpha8
                | Self::TevColor9
                | Self::TevAlpha9
                | Self::TevColor10
                | Self::TevAlpha10
                | Self::TevColor11
                | Self::TevAlpha11
                | Self::TevColor12
                | Self::TevAlpha12
                | Self::TevColor13
                | Self::TevAlpha13
                | Self::TevColor14
                | Self::TevAlpha14
                | Self::TevColor15
                | Self::TevAlpha15
                | Self::TevConstant3AR
                | Self::TevConstant3GB
                | Self::TevConstant0AR
                | Self::TevConstant0GB
                | Self::TevConstant1AR
                | Self::TevConstant1GB
                | Self::TevConstant2AR
                | Self::TevConstant2GB
                | Self::TevKSel0
                | Self::TevKSel1
                | Self::TevKSel2
                | Self::TevKSel3
                | Self::TevKSel4
                | Self::TevKSel5
                | Self::TevKSel6
                | Self::TevKSel7
                | Self::TevDepthTexBias
                | Self::TevDepthTexMode
        )
    }

    #[inline]
    pub fn is_pixel_clear(&self) -> bool {
        matches!(
            self,
            Self::PixelCopyClearAr | Self::PixelCopyClearGb | Self::PixelCopyClearZ
        )
    }

    #[inline]
    pub fn is_scissor(&self) -> bool {
        matches!(
            self,
            Self::ScissorTopLeft | Self::ScissorBottomRight | Self::ScissorOffset
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    QuadList,
    TriangleList,
    TriangleStrip,
    TriangleFan,
    LineList,
    LineStrip,
    PointList,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CullingMode {
    #[default]
    None  = 0b00,
    Back  = 0b01,
    Front = 0b10,
    All   = 0b11,
}

#[bitos(32)]
#[derive(Debug, Default)]
pub struct GenMode {
    #[bits(0..4)]
    pub tex_coords_count: u4,
    #[bits(4..8)]
    pub color_channels_count: u4,
    #[bits(9)]
    pub multisampling: bool,
    #[bits(10..14)]
    pub tev_stages_minus_one: u4,
    #[bits(14..16)]
    pub culling_mode: CullingMode,
    #[bits(16..19)]
    pub bumpmap_count: u3,
    #[bits(19)]
    pub z_freeze: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MatrixId(u8);

impl MatrixId {
    const MAX: u8 = 64 + 32;

    #[inline(always)]
    pub fn from_raw(id: u8) -> Self {
        if id < Self::MAX {
            Self(id)
        } else if id < 2 * 64 {
            Self(id - Self::MAX)
        } else {
            panic!("id out of range")
        }
    }

    #[inline(always)]
    pub fn from_position_idx(index: u8) -> Self {
        assert!(index < 64);
        Self(index)
    }

    #[inline(always)]
    pub fn from_normal_idx(index: u8) -> Self {
        assert!(index < 32);
        Self(index + 64)
    }

    #[inline(always)]
    pub fn get(self) -> u8 {
        self.0
    }

    #[inline(always)]
    pub fn normal(self) -> Self {
        assert!(!self.is_normal());
        Self((self.0 % 32) + 64)
    }

    #[inline(always)]
    pub fn index(&self) -> u8 {
        if self.is_normal() {
            self.0 - 64
        } else {
            self.0
        }
    }

    #[inline(always)]
    pub fn is_normal(&self) -> bool {
        self.0 >= 64
    }
}

/// A vertex extracted from a [`VertexAttributeStream`].
#[derive(Debug, PartialEq, Default)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub pos_norm_matrix: MatrixId,

    pub chan0: Rgba,
    pub chan1: Rgba,

    pub tex_coords: [Vec2; 8],
    pub tex_coords_matrix: [MatrixId; 8],
}

/// A stream of [`Vertex`] elements and their associated matrices.
pub struct VertexStream {
    vertices: Handle<Vertex>,
    matrices: Handle<(MatrixId, Mat4)>,
}

impl VertexStream {
    pub fn vertices(&self) -> &[Vertex] {
        // SAFETY: this struct is only created inside `extract_vertices`, which mantains
        // a static arena
        unsafe { self.vertices.as_slice().assume_init_ref() }
    }

    pub fn matrices(&self) -> &[(MatrixId, Mat4)] {
        // SAFETY: this struct is only created inside `extract_vertices`, which mantains
        // a static arena
        unsafe { self.matrices.as_slice().assume_init_ref() }
    }
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct MatrixSet(BitArray<[u64; 2]>);

impl Default for MatrixSet {
    fn default() -> Self {
        Self(BitArray::new([0; 2]))
    }
}

impl MatrixSet {
    #[inline(always)]
    pub fn include(&mut self, id: MatrixId) {
        self.0.set(id.get() as usize, true);
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.fill(false);
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = MatrixId> {
        self.0.iter_ones().map(|x| MatrixId::from_raw(x as u8))
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.count_ones()
    }
}

#[derive(Debug)]
pub struct XfbCopy {
    pub addr: Address,
    pub args: render::CopyArgs,
}

pub struct Gpu {
    pub mode: GenMode,
    pub cmd: cmd::Interface,
    pub xform: xform::Interface,
    pub env: tev::Interface,
    pub tex: tex::Interface,
    pub pix: pix::Interface,
    pub write_mask: u32,
    pub xfb_copies: Vec<XfbCopy>,
    matrix_set: Box<MatrixSet>,
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            mode: Default::default(),
            cmd: Default::default(),
            xform: Default::default(),
            env: Default::default(),
            tex: Default::default(),
            pix: Default::default(),
            write_mask: 0x00FF_FFFF,
            matrix_set: Box::default(),
            xfb_copies: Vec::with_capacity(4),
        }
    }
}

pub fn update_texenv(sys: &mut System) {
    let stages = sys
        .gpu
        .env
        .stage_ops
        .iter()
        .take(sys.gpu.env.active_stages as usize)
        .cloned()
        .enumerate()
        .map(|(i, ops)| {
            let ref_pair = &sys.gpu.env.stage_refs[i / 2];
            let const_pair = &sys.gpu.env.stage_consts[i / 2];

            let (refs, color_const, alpha_const) = if i % 2 == 0 {
                (ref_pair.a(), const_pair.color_a(), const_pair.alpha_a())
            } else {
                (ref_pair.b(), const_pair.color_b(), const_pair.alpha_b())
            };

            render::TexEnvStage {
                ops,
                refs,
                color_const,
                alpha_const,
            }
        })
        .collect::<Vec<_>>();

    let config = render::TexEnvConfig {
        stages,
        constants: sys.gpu.env.constants,
        depth_tex: sys.gpu.env.depth_tex,
    };

    sys.modules
        .render
        .exec(render::Action::SetTexEnvConfig(config));
}

pub fn set_register(sys: &mut System, reg: Reg, value: u32) {
    let mask = std::mem::replace(&mut sys.gpu.write_mask, 0x00FF_FFFF);
    let masked = value & mask;

    macro_rules! write_masked {
        ($value:expr) => {{
            let old = $value.to_bits() & !mask;
            let new = old | masked;
            new.write_ne_bytes($value.as_mut_bytes());
        }};
        ($extra_mask:expr; $value:expr) => {{
            let masked = masked & $extra_mask;
            let old = $value.to_bits() & !mask;
            let new = old | masked;
            new.write_ne_bytes($value.as_mut_bytes());
        }};
    }

    match reg {
        Reg::GenMode => {
            write_masked!(sys.gpu.mode);
            let mode = &sys.gpu.mode;
            sys.gpu.env.active_stages = mode.tev_stages_minus_one().value() + 1;
            sys.gpu.env.active_channels = mode.color_channels_count().value();
        }

        Reg::ScissorTopLeft => write_masked!(sys.gpu.pix.scissor.top_left),
        Reg::ScissorBottomRight => write_masked!(sys.gpu.pix.scissor.bottom_right),
        Reg::ScissorOffset => write_masked!(sys.gpu.pix.scissor.offset),

        Reg::TevRefs01 => write_masked!(sys.gpu.env.stage_refs[0]),
        Reg::TevRefs23 => write_masked!(sys.gpu.env.stage_refs[1]),
        Reg::TevRefs45 => write_masked!(sys.gpu.env.stage_refs[2]),
        Reg::TevRefs67 => write_masked!(sys.gpu.env.stage_refs[3]),
        Reg::TevRefs89 => write_masked!(sys.gpu.env.stage_refs[4]),
        Reg::TevRefsAB => write_masked!(sys.gpu.env.stage_refs[5]),
        Reg::TevRefsCD => write_masked!(sys.gpu.env.stage_refs[6]),
        Reg::TevRefsEF => write_masked!(sys.gpu.env.stage_refs[7]),
        Reg::TexScaleU0 => write_masked!(sys.gpu.tex.maps[0].scaling.u),
        Reg::TexScaleV0 => write_masked!(sys.gpu.tex.maps[0].scaling.v),
        Reg::TexScaleU1 => write_masked!(sys.gpu.tex.maps[1].scaling.u),
        Reg::TexScaleV1 => write_masked!(sys.gpu.tex.maps[1].scaling.v),
        Reg::TexScaleU2 => write_masked!(sys.gpu.tex.maps[2].scaling.u),
        Reg::TexScaleV2 => write_masked!(sys.gpu.tex.maps[2].scaling.v),
        Reg::TexScaleU3 => write_masked!(sys.gpu.tex.maps[3].scaling.u),
        Reg::TexScaleV3 => write_masked!(sys.gpu.tex.maps[3].scaling.v),
        Reg::TexScaleU4 => write_masked!(sys.gpu.tex.maps[4].scaling.u),
        Reg::TexScaleV4 => write_masked!(sys.gpu.tex.maps[4].scaling.v),
        Reg::TexScaleU5 => write_masked!(sys.gpu.tex.maps[5].scaling.u),
        Reg::TexScaleV5 => write_masked!(sys.gpu.tex.maps[5].scaling.v),
        Reg::TexScaleU6 => write_masked!(sys.gpu.tex.maps[6].scaling.u),
        Reg::TexScaleV6 => write_masked!(sys.gpu.tex.maps[6].scaling.v),
        Reg::TexScaleU7 => write_masked!(sys.gpu.tex.maps[7].scaling.u),
        Reg::TexScaleV7 => write_masked!(sys.gpu.tex.maps[7].scaling.v),

        Reg::PixelZMode => {
            write_masked!(sys.gpu.pix.depth_mode);
            sys.modules
                .render
                .exec(render::Action::SetDepthMode(sys.gpu.pix.depth_mode));
        }
        Reg::PixelBlendMode => {
            write_masked!(sys.gpu.pix.blend_mode);
            sys.modules
                .render
                .exec(render::Action::SetBlendMode(sys.gpu.pix.blend_mode));
        }
        Reg::PixelConstantAlpha => {
            write_masked!(sys.gpu.pix.constant_alpha);
            sys.modules
                .render
                .exec(render::Action::SetConstantAlpha(sys.gpu.pix.constant_alpha));
        }
        Reg::PixelControl => {
            write_masked!(sys.gpu.pix.control);
            sys.modules
                .render
                .exec(render::Action::SetFramebufferFormat(
                    sys.gpu.pix.control.format(),
                ));
        }
        Reg::PixelDone => {
            sys.gpu.pix.interrupt.set_finish(true);
            sys.scheduler.schedule_now(pi::check_interrupts);
        }
        Reg::PixelToken => write_masked!(0xFFFF; sys.gpu.pix.token),
        Reg::PixelTokenInt => {
            write_masked!(0xFFFF; sys.gpu.pix.token);
            sys.gpu.pix.interrupt.set_token(true);
            sys.scheduler.schedule_now(pi::check_interrupts);
        }
        Reg::PixelCopySrc => write_masked!(sys.gpu.pix.copy_src),
        Reg::PixelCopyDimensions => write_masked!(sys.gpu.pix.copy_dims),
        Reg::PixelCopyDst => {
            let mut value = sys.gpu.pix.copy_dst.value() >> 5;
            write_masked!(value);
            sys.gpu.pix.copy_dst = Address((value << 5).with_bits(26, 32, 0));
        }
        Reg::PixelCopyDstStride => write_masked!(sys.gpu.pix.copy_stride),
        Reg::PixelCopyClearAr => {
            let mut value = 0
                .with_bits(0, 8, sys.gpu.pix.clear_color.r as u32)
                .with_bits(8, 16, sys.gpu.pix.clear_color.a as u32);
            write_masked!(value);
            sys.gpu.pix.clear_color.r = value.bits(0, 8) as u8;
            sys.gpu.pix.clear_color.a = value.bits(8, 16) as u8;
        }
        Reg::PixelCopyClearGb => {
            let mut value = 0
                .with_bits(0, 8, sys.gpu.pix.clear_color.b as u32)
                .with_bits(8, 16, sys.gpu.pix.clear_color.g as u32);
            write_masked!(value);
            sys.gpu.pix.clear_color.b = value.bits(0, 8) as u8;
            sys.gpu.pix.clear_color.g = value.bits(8, 16) as u8;
        }
        Reg::PixelCopyClearZ => {
            write_masked!(sys.gpu.pix.clear_depth);
            sys.modules.render.exec(render::Action::SetClearDepth(
                sys.gpu.pix.clear_depth as f32 / DEPTH_24_BIT_MAX as f32,
            ));
        }
        Reg::PixelCopyCmd => {
            // TODO: proper masked
            let cmd = pix::CopyCmd::from_bits(value);
            efb_copy(sys, cmd);
        }

        Reg::TexLutAddress => {
            let mut value = sys.gpu.tex.clut_addr.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.clut_addr = Address((value << 5).with_bits(26, 32, 0));
        }
        Reg::TexLutLoad => {
            write_masked!(sys.gpu.tex.clut_load);
            tex::update_clut(sys);
        }

        Reg::TexSampler0 => write_masked!(sys.gpu.tex.maps[0].sampler),
        Reg::TexSampler1 => write_masked!(sys.gpu.tex.maps[1].sampler),
        Reg::TexSampler2 => write_masked!(sys.gpu.tex.maps[2].sampler),
        Reg::TexSampler3 => write_masked!(sys.gpu.tex.maps[3].sampler),
        Reg::TexSampler4 => write_masked!(sys.gpu.tex.maps[4].sampler),
        Reg::TexSampler5 => write_masked!(sys.gpu.tex.maps[5].sampler),
        Reg::TexSampler6 => write_masked!(sys.gpu.tex.maps[6].sampler),
        Reg::TexSampler7 => write_masked!(sys.gpu.tex.maps[7].sampler),

        Reg::TexLod0 => write_masked!(sys.gpu.tex.maps[0].lods.limits),
        Reg::TexLod1 => write_masked!(sys.gpu.tex.maps[1].lods.limits),
        Reg::TexLod2 => write_masked!(sys.gpu.tex.maps[2].lods.limits),
        Reg::TexLod3 => write_masked!(sys.gpu.tex.maps[3].lods.limits),
        Reg::TexLod4 => write_masked!(sys.gpu.tex.maps[4].lods.limits),
        Reg::TexLod5 => write_masked!(sys.gpu.tex.maps[5].lods.limits),
        Reg::TexLod6 => write_masked!(sys.gpu.tex.maps[6].lods.limits),
        Reg::TexLod7 => write_masked!(sys.gpu.tex.maps[7].lods.limits),
        Reg::TexFormat0 => write_masked!(sys.gpu.tex.maps[0].encoding),
        Reg::TexFormat1 => write_masked!(sys.gpu.tex.maps[1].encoding),
        Reg::TexFormat2 => write_masked!(sys.gpu.tex.maps[2].encoding),
        Reg::TexFormat3 => write_masked!(sys.gpu.tex.maps[3].encoding),
        Reg::TexFormat4 => write_masked!(sys.gpu.tex.maps[4].encoding),
        Reg::TexFormat5 => write_masked!(sys.gpu.tex.maps[5].encoding),
        Reg::TexFormat6 => write_masked!(sys.gpu.tex.maps[6].encoding),
        Reg::TexFormat7 => write_masked!(sys.gpu.tex.maps[7].encoding),
        Reg::TexOddLodAddress0 => write_masked!(sys.gpu.tex.maps[0].lods.odd),
        Reg::TexOddLodAddress1 => write_masked!(sys.gpu.tex.maps[1].lods.odd),
        Reg::TexOddLodAddress2 => write_masked!(sys.gpu.tex.maps[2].lods.odd),
        Reg::TexOddLodAddress3 => write_masked!(sys.gpu.tex.maps[3].lods.odd),
        Reg::TexOddLodAddress4 => write_masked!(sys.gpu.tex.maps[4].lods.odd),
        Reg::TexOddLodAddress5 => write_masked!(sys.gpu.tex.maps[5].lods.odd),
        Reg::TexOddLodAddress6 => write_masked!(sys.gpu.tex.maps[6].lods.odd),
        Reg::TexOddLodAddress7 => write_masked!(sys.gpu.tex.maps[7].lods.odd),

        Reg::TexAddress0 => {
            let mut value = sys.gpu.tex.maps[0].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[0].address = Address(value << 5);
        }
        Reg::TexAddress1 => {
            let mut value = sys.gpu.tex.maps[1].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[1].address = Address(value << 5);
        }
        Reg::TexAddress2 => {
            let mut value = sys.gpu.tex.maps[2].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[2].address = Address(value << 5);
        }
        Reg::TexAddress3 => {
            let mut value = sys.gpu.tex.maps[3].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[3].address = Address(value << 5);
        }
        Reg::TexAddress4 => {
            let mut value = sys.gpu.tex.maps[4].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[4].address = Address(value << 5);
        }
        Reg::TexAddress5 => {
            let mut value = sys.gpu.tex.maps[5].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[5].address = Address(value << 5);
        }
        Reg::TexAddress6 => {
            let mut value = sys.gpu.tex.maps[6].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[6].address = Address(value << 5);
        }
        Reg::TexAddress7 => {
            let mut value = sys.gpu.tex.maps[7].address.value() >> 5;
            write_masked!(value);
            sys.gpu.tex.maps[7].address = Address(value << 5);
        }

        Reg::TexLutRef0 => write_masked!(sys.gpu.tex.maps[0].clut),
        Reg::TexLutRef1 => write_masked!(sys.gpu.tex.maps[1].clut),
        Reg::TexLutRef2 => write_masked!(sys.gpu.tex.maps[2].clut),
        Reg::TexLutRef3 => write_masked!(sys.gpu.tex.maps[3].clut),
        Reg::TexLutRef4 => write_masked!(sys.gpu.tex.maps[4].clut),
        Reg::TexLutRef5 => write_masked!(sys.gpu.tex.maps[5].clut),
        Reg::TexLutRef6 => write_masked!(sys.gpu.tex.maps[6].clut),
        Reg::TexLutRef7 => write_masked!(sys.gpu.tex.maps[7].clut),

        Reg::TevColor0 => write_masked!(sys.gpu.env.stage_ops[0].color),
        Reg::TevAlpha0 => write_masked!(sys.gpu.env.stage_ops[0].alpha),
        Reg::TevColor1 => write_masked!(sys.gpu.env.stage_ops[1].color),
        Reg::TevAlpha1 => write_masked!(sys.gpu.env.stage_ops[1].alpha),
        Reg::TevColor2 => write_masked!(sys.gpu.env.stage_ops[2].color),
        Reg::TevAlpha2 => write_masked!(sys.gpu.env.stage_ops[2].alpha),
        Reg::TevColor3 => write_masked!(sys.gpu.env.stage_ops[3].color),
        Reg::TevAlpha3 => write_masked!(sys.gpu.env.stage_ops[3].alpha),
        Reg::TevColor4 => write_masked!(sys.gpu.env.stage_ops[4].color),
        Reg::TevAlpha4 => write_masked!(sys.gpu.env.stage_ops[4].alpha),
        Reg::TevColor5 => write_masked!(sys.gpu.env.stage_ops[5].color),
        Reg::TevAlpha5 => write_masked!(sys.gpu.env.stage_ops[5].alpha),
        Reg::TevColor6 => write_masked!(sys.gpu.env.stage_ops[6].color),
        Reg::TevAlpha6 => write_masked!(sys.gpu.env.stage_ops[6].alpha),
        Reg::TevColor7 => write_masked!(sys.gpu.env.stage_ops[7].color),
        Reg::TevAlpha7 => write_masked!(sys.gpu.env.stage_ops[7].alpha),
        Reg::TevColor8 => write_masked!(sys.gpu.env.stage_ops[8].color),
        Reg::TevAlpha8 => write_masked!(sys.gpu.env.stage_ops[8].alpha),
        Reg::TevColor9 => write_masked!(sys.gpu.env.stage_ops[9].color),
        Reg::TevAlpha9 => write_masked!(sys.gpu.env.stage_ops[9].alpha),
        Reg::TevColor10 => write_masked!(sys.gpu.env.stage_ops[10].color),
        Reg::TevAlpha10 => write_masked!(sys.gpu.env.stage_ops[10].alpha),
        Reg::TevColor11 => write_masked!(sys.gpu.env.stage_ops[11].color),
        Reg::TevAlpha11 => write_masked!(sys.gpu.env.stage_ops[11].alpha),
        Reg::TevColor12 => write_masked!(sys.gpu.env.stage_ops[12].color),
        Reg::TevAlpha12 => write_masked!(sys.gpu.env.stage_ops[12].alpha),
        Reg::TevColor13 => write_masked!(sys.gpu.env.stage_ops[13].color),
        Reg::TevAlpha13 => write_masked!(sys.gpu.env.stage_ops[13].alpha),
        Reg::TevColor14 => write_masked!(sys.gpu.env.stage_ops[14].color),
        Reg::TevAlpha14 => write_masked!(sys.gpu.env.stage_ops[14].alpha),
        Reg::TevColor15 => write_masked!(sys.gpu.env.stage_ops[15].color),
        Reg::TevAlpha15 => write_masked!(sys.gpu.env.stage_ops[15].alpha),
        Reg::TevConstant3AR => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let r = ((value.bits(0, 11) as i16) << 5) >> 5;
            let a = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[3].a = a;
            sys.gpu.env.constants[3].r = r;
        }
        Reg::TevConstant3GB => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let b = ((value.bits(0, 11) as i16) << 5) >> 5;
            let g = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[3].b = b;
            sys.gpu.env.constants[3].g = g;
        }
        Reg::TevConstant0AR => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let r = ((value.bits(0, 11) as i16) << 5) >> 5;
            let a = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[0].a = a;
            sys.gpu.env.constants[0].r = r;
        }
        Reg::TevConstant0GB => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let b = ((value.bits(0, 11) as i16) << 5) >> 5;
            let g = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[0].b = b;
            sys.gpu.env.constants[0].g = g;
        }
        Reg::TevConstant1AR => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let r = ((value.bits(0, 11) as i16) << 5) >> 5;
            let a = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[1].a = a;
            sys.gpu.env.constants[1].r = r;
        }
        Reg::TevConstant1GB => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let b = ((value.bits(0, 11) as i16) << 5) >> 5;
            let g = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[1].b = b;
            sys.gpu.env.constants[1].g = g;
        }
        Reg::TevConstant2AR => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let r = ((value.bits(0, 11) as i16) << 5) >> 5;
            let a = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[2].a = a;
            sys.gpu.env.constants[2].r = r;
        }
        Reg::TevConstant2GB => {
            if mask != 0x00FF_FFFF {
                todo!();
            }

            let b = ((value.bits(0, 11) as i16) << 5) >> 5;
            let g = ((value.bits(12, 23) as i16) << 5) >> 5;
            sys.gpu.env.constants[2].b = b;
            sys.gpu.env.constants[2].g = g;
        }
        Reg::TevAlphaFunc => {
            write_masked!(sys.gpu.env.alpha_func);
            sys.modules.render.exec(render::Action::SetAlphaFunction(
                sys.gpu.env.alpha_func.clone(),
            ));
        }

        Reg::TevDepthTexBias => write_masked!(sys.gpu.env.depth_tex.bias),
        Reg::TevDepthTexMode => write_masked!(sys.gpu.env.depth_tex.mode),

        Reg::TevKSel0 => write_masked!(sys.gpu.env.stage_consts[0]),
        Reg::TevKSel1 => write_masked!(sys.gpu.env.stage_consts[1]),
        Reg::TevKSel2 => write_masked!(sys.gpu.env.stage_consts[2]),
        Reg::TevKSel3 => write_masked!(sys.gpu.env.stage_consts[3]),
        Reg::TevKSel4 => write_masked!(sys.gpu.env.stage_consts[4]),
        Reg::TevKSel5 => write_masked!(sys.gpu.env.stage_consts[5]),
        Reg::TevKSel6 => write_masked!(sys.gpu.env.stage_consts[6]),
        Reg::TevKSel7 => write_masked!(sys.gpu.env.stage_consts[7]),
        Reg::WriteMask => {
            sys.gpu.write_mask = value;
        }
        _ => {
            tracing::warn!("unimplemented write to internal GX register {reg:?}: 0x{value:06X}")
        }
    }

    if reg == Reg::GenMode {
        sys.gpu.env.stages_dirty = true;
        sys.gpu.xform.internal.stages_dirty = true;
        sys.modules
            .render
            .exec(render::Action::SetCullingMode(sys.gpu.mode.culling_mode()));
    }

    if let Some(map) = reg.texmap() {
        sys.gpu.tex.maps[map as usize].dirty = true;
    }

    if reg.is_tev() {
        sys.gpu.env.stages_dirty = true;
    }

    if reg.is_pixel_clear() {
        sys.modules.render.exec(render::Action::SetClearColor(
            sys.gpu.pix.clear_color.into(),
        ));
    }

    if reg.is_scissor() {
        sys.modules
            .render
            .exec(render::Action::SetScissor(sys.gpu.pix.scissor));
    }
}

#[inline]
fn alloc_vertices_handle(length: usize) -> Handle<Vertex> {
    const CHUNK_SIZE: usize = bytesize::MIB as usize;
    const CHUNK_CAPACITY: NonZero<usize> = NonZero::new(CHUNK_SIZE / size_of::<Vertex>()).unwrap();

    static ARENA: LazyLock<Mutex<RingArena<Vertex>>> =
        LazyLock::new(|| Mutex::new(RingArena::new(CHUNK_CAPACITY)));

    ARENA.lock().unwrap().allocate(length)
}

#[inline]
fn alloc_matrices_handle(length: usize) -> Handle<(MatrixId, Mat4)> {
    const CHUNK_SIZE: usize = 2 * bytesize::MIB as usize;
    const CHUNK_CAPACITY: NonZero<usize> = NonZero::new(CHUNK_SIZE / size_of::<Mat4>()).unwrap();

    static ARENA: LazyLock<Mutex<RingArena<(MatrixId, Mat4)>>> =
        LazyLock::new(|| Mutex::new(RingArena::new(CHUNK_CAPACITY)));

    ARENA.lock().unwrap().allocate(length)
}

fn extract_vertices(sys: &mut System, stream: &VertexAttributeStream) -> VertexStream {
    let mut vertices = alloc_vertices_handle(stream.count() as usize);
    let vertices_slice = unsafe { vertices.as_mut_slice() };

    sys.gpu.matrix_set.clear();

    let ctx = vertex::Ctx {
        ram: sys.mem.ram(),
        arrays: &sys.gpu.cmd.internal.arrays,
        default_matrices: &sys.gpu.xform.internal.default_matrices,
    };

    let vcd = &sys.gpu.cmd.internal.vertex_descriptor;
    let vat = &sys.gpu.cmd.internal.vertex_attr_tables[stream.table_index()];
    assert!(vcd.position().is_present());

    sys.modules.vertex.parse(
        ctx,
        vcd,
        vat,
        stream,
        vertices_slice,
        &mut sys.gpu.matrix_set,
    );

    let mut matrices = alloc_matrices_handle(sys.gpu.matrix_set.len());
    let matrices_slice = unsafe { matrices.as_mut_slice() };

    for (i, mat_id) in sys.gpu.matrix_set.iter().enumerate() {
        let mat = if mat_id.is_normal() {
            Mat4::from_mat3(sys.gpu.xform.normal_matrix(mat_id.index()))
        } else {
            sys.gpu.xform.matrix(mat_id.index())
        };

        matrices_slice[i].write((mat_id, mat));
    }

    VertexStream { vertices, matrices }
}

fn draw(sys: &mut System, topology: Topology, stream: &VertexAttributeStream) {
    if std::mem::take(&mut sys.gpu.xform.internal.viewport_dirty) {
        let viewport = &sys.gpu.xform.internal.viewport;
        let viewport = render::Viewport {
            width: viewport.width,
            height: viewport.height,
            top_left_x: viewport.center_x - viewport.width / 2.0,
            top_left_y: viewport.center_y - viewport.height / 2.0,
            near_depth: viewport.far - viewport.far_minus_near,
            far_depth: viewport.far,
        };

        sys.modules
            .render
            .exec(render::Action::SetViewport(viewport));
    }

    if std::mem::take(&mut sys.gpu.xform.internal.stages_dirty) {
        xform::update_texgen(sys);
    }

    if std::mem::take(&mut sys.gpu.env.stages_dirty) {
        self::update_texenv(sys);
    }

    for map in 0..8 {
        if std::mem::take(&mut sys.gpu.tex.maps[map].dirty) {
            tex::update_texture(sys, map);
        }
    }

    let vertices = self::extract_vertices(sys, stream);
    sys.modules
        .render
        .exec(render::Action::Draw(topology, vertices));
}

fn call(sys: &mut System, address: Address, length: u32) {
    tracing::debug!("called {} with length 0x{:08X}", address, length);
    let address = address.value().with_bits(26, 32, 0) & !0x1F;
    // TODO: consider this
    // let length = length.value().with_bit(31, false) & !0x1F;
    let data = &sys.mem.ram()[address.value() as usize..][..length as usize];
    sys.gpu.cmd.queue.push_front_bytes(data);
}

fn efb_copy(sys: &mut System, cmd: pix::CopyCmd) {
    let args = render::CopyArgs {
        src: sys.gpu.pix.copy_src,
        dims: sys.gpu.pix.copy_dims,
        half: cmd.half(),
        clear: cmd.clear(),
    };

    let divisor = if args.half { 2 } else { 1 };
    let width = args.dims.width() as u32 / divisor;
    let height = args.dims.height() as u32 / divisor;
    let dst = sys.gpu.pix.copy_dst;
    let stride = sys.gpu.pix.copy_stride;

    if cmd.to_xfb() {
        let id = sys.gpu.xfb_copies.len() as u32;
        sys.gpu.xfb_copies.push(XfbCopy { addr: dst, args });

        sys.modules
            .render
            .exec(render::Action::CopyXfb { args, id });

        return;
    }

    let id = render::TextureId(dst.value());
    let format = if sys.gpu.pix.control.format().is_depth() {
        let (sender, receiver) = if sys.config.perform_efb_copies {
            let (sender, receiver) = oneshot::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };

        sys.modules.render.exec(render::Action::CopyDepth {
            args,
            format: cmd.depth_format(),
            response: sender,
            id,
        });

        if let Some(receiver) = receiver {
            let Ok(texels) = receiver.recv() else {
                tracing::error!("render module did not answer depth copy request");
                return;
            };

            let output = &mut sys.mem.ram_mut()[dst.value() as usize..];
            tex::encode_depth_texture(texels, cmd.depth_format(), stride, width, height, output);
        }

        cmd.depth_format().texture_format()
    } else {
        let (sender, receiver) = if sys.config.perform_efb_copies {
            let (sender, receiver) = oneshot::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };

        sys.modules.render.exec(render::Action::CopyColor {
            args,
            format: cmd.color_format(),
            response: sender,
            id,
        });

        if let Some(receiver) = receiver {
            let Ok(texels) = receiver.recv() else {
                tracing::error!("render module did not answer color copy request");
                return;
            };

            let output = &mut sys.mem.ram_mut()[dst.value() as usize..];
            tex::encode_color_texture(texels, cmd.color_format(), stride, width, height, output);
        }

        cmd.color_format().texture_format()
    };

    if !sys.config.perform_efb_copies {
        let len = tex::Encoding::length_for(width, height, format) as usize;
        let data = &sys.mem.ram()[dst.value() as usize..][..len];
        sys.gpu.tex.update_tex_hash(dst, data);
    }
}
