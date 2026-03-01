//! Transform unit (XF).
use bitos::integer::{u3, u6};
use bitos::{BitUtils, bitos};
use color::Abgr8;
use glam::{Mat3, Mat4, Vec3};
use strum::FromRepr;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

use crate::Primitive;
use crate::modules::render;
use crate::system::System;
use crate::system::gx::DEPTH_24_BIT_MAX;
use crate::system::gx::cmd::ArrayDescriptor;

/// A transform unit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub enum Reg {
    Error                = 0x00,
    Diagnostics          = 0x01,
    State0               = 0x02,
    State1               = 0x03,
    PowerSave            = 0x04,
    ClipDisable          = 0x05,
    Perf0                = 0x06,
    Perf1                = 0x07,
    InVertexSpec         = 0x08,
    NumColors            = 0x09,
    Ambient0             = 0x0A,
    Ambient1             = 0x0B,
    Material0            = 0x0C,
    Material1            = 0x0D,
    ColorControl0        = 0x0E,
    ColorControl1        = 0x0F,
    AlphaControl0        = 0x10,
    AlphaControl1        = 0x11,
    DualTextureTransform = 0x12,
    MatIndexLow          = 0x18,
    MatIndexHigh         = 0x19,
    ViewportScaleX       = 0x1A,
    ViewportScaleY       = 0x1B,
    ViewportScaleZ       = 0x1C,
    ViewportOffsetX      = 0x1D,
    ViewportOffsetY      = 0x1E,
    ViewportOffsetZ      = 0x1F,
    ProjectionParam0     = 0x20,
    ProjectionParam1     = 0x21,
    ProjectionParam2     = 0x22,
    ProjectionParam3     = 0x23,
    ProjectionParam4     = 0x24,
    ProjectionParam5     = 0x25,
    ProjectionOrthographic = 0x26,
    TexGenCount          = 0x3F,
    TexGen0              = 0x40,
    TexGen1              = 0x41,
    TexGen2              = 0x42,
    TexGen3              = 0x43,
    TexGen4              = 0x44,
    TexGen5              = 0x45,
    TexGen6              = 0x46,
    TexGen7              = 0x47,
    PostTexGen0          = 0x50,
    PostTexGen1          = 0x51,
    PostTexGen2          = 0x52,
    PostTexGen3          = 0x53,
    PostTexGen4          = 0x54,
    PostTexGen5          = 0x55,
    PostTexGen6          = 0x56,
    PostTexGen7          = 0x57,
}

impl Reg {
    pub fn is_viewport(&self) -> bool {
        matches!(
            self,
            Reg::ViewportScaleX
                | Reg::ViewportScaleY
                | Reg::ViewportScaleZ
                | Reg::ViewportOffsetX
                | Reg::ViewportOffsetY
                | Reg::ViewportOffsetZ
        )
    }

    pub fn is_projection_param(&self) -> bool {
        matches!(
            self,
            Reg::ProjectionParam0
                | Reg::ProjectionParam1
                | Reg::ProjectionParam2
                | Reg::ProjectionParam3
                | Reg::ProjectionParam4
                | Reg::ProjectionParam5
                | Reg::ProjectionOrthographic
        )
    }

    pub fn is_texgen(&self) -> bool {
        matches!(
            self,
            Reg::TexGenCount
                | Reg::TexGen0
                | Reg::TexGen1
                | Reg::TexGen2
                | Reg::TexGen3
                | Reg::TexGen4
                | Reg::TexGen5
                | Reg::TexGen6
                | Reg::TexGen7
                | Reg::PostTexGen0
                | Reg::PostTexGen1
                | Reg::PostTexGen2
                | Reg::PostTexGen3
                | Reg::PostTexGen4
                | Reg::PostTexGen5
                | Reg::PostTexGen6
                | Reg::PostTexGen7
        )
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, Default)]
pub enum TexGenOutputKind {
    #[default]
    Vec2 = 0,
    Vec3 = 1,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, Default)]
pub enum TexGenInputKind {
    #[default]
    AB11 = 0,
    ABC1 = 1,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, Default)]
pub enum TexGenKind {
    #[default]
    Transform     = 0b00,
    Emboss        = 0b01,
    ColorDiffuse  = 0b10,
    ColorSpecular = 0b11,
}

#[bitos(4)]
#[derive(Debug, Clone, Copy, Default)]
pub enum TexGenSource {
    #[default]
    Position  = 0x0,
    Normal    = 0x1,
    Color     = 0x2,
    BinormalT = 0x3,
    BinormalB = 0x4,
    TexCoord0 = 0x5,
    TexCoord1 = 0x6,
    TexCoord2 = 0x7,
    TexCoord3 = 0x8,
    TexCoord4 = 0x9,
    TexCoord5 = 0xA,
    TexCoord6 = 0xB,
    TexCoord7 = 0xC,
    Reserved0 = 0xD,
    Reserved1 = 0xE,
    Reserved2 = 0xF,
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BaseTexGen {
    #[bits(1)]
    pub output_kind: TexGenOutputKind,
    #[bits(2)]
    pub input_kind: TexGenInputKind,
    #[bits(4..6)]
    pub kind: TexGenKind,
    #[bits(7..11)]
    pub source: TexGenSource,
    #[bits(12..15)]
    pub emboss_source: u3,
    #[bits(15..18)]
    pub emboss_light: u3,
}

#[bitos(32)]
#[derive(Debug, Clone, Default)]
pub struct PostTexGen {
    #[bits(0..6)]
    pub mat_index: u6,
    #[bits(8)]
    pub normalize: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TexGen {
    /// Base TexGen transform
    pub base: BaseTexGen,
    /// Post TexGen transform (Dual)
    pub post: PostTexGen,
}

#[derive(Debug, Clone, Copy, FromBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct Light {
    pub color: Abgr8,
    pub cos_attenuation: Vec3,
    pub dist_attenuation: Vec3,
    pub position: Vec3,
    pub direction: Vec3,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffuseAttenuation {
    One            = 0b00,
    Compute        = 0b01,
    ComputeClamped = 0b10,
    Reserved0      = 0b11,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Channel {
    #[bits(0)]
    pub material_from_vertex: bool,
    #[bits(1)]
    pub lighting_enabled: bool,
    #[bits(2..6)]
    pub lights0to3: [bool; 4],
    #[bits(6)]
    pub ambient_from_vertex: bool,
    #[bits(7..9)]
    pub diffuse_atten: DiffuseAttenuation,
    #[bits(9)]
    pub position_atten: bool,
    #[bits(10)]
    pub not_specular: bool,
    #[bits(11..15)]
    pub lights4to7: [bool; 4],
}

#[bitos(64)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultMatrices {
    #[bits(0..6)]
    pub view: u6,
    #[bits(6..54)]
    pub tex: [u6; 8],
}

#[derive(Debug, Default)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub far: f32,
    pub far_minus_near: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectionMtx {
    pub params: [f32; 6],
    pub orthographic: bool,
}

impl ProjectionMtx {
    pub fn value(&self) -> Mat4 {
        let p = &self.params;
        if self.orthographic {
            Mat4::from_cols_array_2d(&[
                [p[0], 0.0, 0.0, p[1]],
                [0.0, p[2], 0.0, p[3]],
                [0.0, 0.0, p[4], p[5]],
                [0.0, 0.0, 0.0, 1.0],
            ])
        } else {
            Mat4::from_cols_array_2d(&[
                [p[0], 0.0, p[1], 0.0],
                [0.0, p[2], p[3], 0.0],
                [0.0, 0.0, p[4], p[5]],
                [0.0, 0.0, -1.0, 0.0],
            ])
        }
        .transpose()
    }
}

#[derive(Debug, Default)]
pub struct Internal {
    pub ambient: [Abgr8; 2],
    pub material: [Abgr8; 2],
    pub color_control: [Channel; 2],
    pub alpha_control: [Channel; 2],
    pub viewport: Viewport,
    pub viewport_dirty: bool,
    pub default_matrices: DefaultMatrices,
    pub projection_mtx: ProjectionMtx,
    pub texgen: [TexGen; 8],
    pub post_texgen: [PostTexGen; 8],
    pub active_texgens: u8,
    pub stages_dirty: bool,
}

/// Transform unit
#[derive(Debug)]
pub struct Interface {
    pub ram: Box<[u32; 0x1000]>,
    pub internal: Internal,
}

impl Default for Interface {
    fn default() -> Self {
        Self {
            ram: util::boxed_array(0),
            internal: Default::default(),
        }
    }
}

impl Interface {
    /// Returns the matrix at `index` in internal memory.
    #[inline]
    pub fn matrix(&self, index: u8) -> Mat4 {
        let offset = 4 * index as usize;
        let data = &self.ram[offset..][..16];
        let m: &[f32] = zerocopy::transmute_ref!(data);

        Mat4::from_cols_array(&[
            m[0], m[4], m[8], 0.0, // col 0
            m[1], m[5], m[9], 0.0, // col 1
            m[2], m[6], m[10], 0.0, // col 2
            m[3], m[7], m[11], 1.0, // col 3
        ])
    }

    /// Returns the normal matrix at `index` in internal memory.
    #[inline]
    pub fn normal_matrix(&self, index: u8) -> Mat3 {
        let offset = 3 * index as usize;
        let data = &self.ram[0x400 + offset..][..9];
        let m: &[f32] = zerocopy::transmute_ref!(data);

        Mat3::from_cols_array(&[
            m[0], m[3], m[6], // col 0
            m[1], m[4], m[7], // col 1
            m[2], m[5], m[8], // col 2
        ])
    }

    /// Returns the projection matrix.
    #[inline]
    pub fn projection_matrix(&self) -> Mat4 {
        self.internal.projection_mtx.value()
    }

    /// Returns the post matrix at `index` in internal memory.
    #[inline]
    pub fn post_matrix(&self, index: u8) -> Mat4 {
        let offset = 4 * index as usize;
        let data = &self.ram[0x500 + offset..][..16];
        let m: &[f32] = zerocopy::transmute_ref!(data);

        Mat4::from_cols_array(&[
            m[0], m[4], m[8], 0.0, // col 0
            m[1], m[5], m[9], 0.0, // col 1
            m[2], m[6], m[10], 0.0, // col 2
            m[3], m[7], m[11], 1.0, // col 3
        ])
    }

    /// Returns the post matrix at `index` in internal memory.
    #[inline]
    pub fn light(&self, index: u8) -> &Light {
        let stride = 0x10;
        let offset = stride * index as usize;
        let data = &self.ram[0x603 + offset..][..size_of::<Light>() / 4];
        Light::try_ref_from_bytes(data.as_bytes()).unwrap()
    }
}

pub fn update_texgen(sys: &mut System) {
    let mut stages = Vec::with_capacity(sys.gpu.xform.internal.active_texgens as usize);
    for texgen in sys
        .gpu
        .xform
        .internal
        .texgen
        .iter()
        .take(sys.gpu.xform.internal.active_texgens as usize)
        .cloned()
    {
        let stage = render::TexGenStage {
            base: texgen.base,
            normalize: texgen.post.normalize(),
            post_matrix: sys.gpu.xform.post_matrix(texgen.post.mat_index().value()),
        };

        stages.push(stage);
    }

    let config = render::TexGenConfig { stages };
    sys.modules
        .render
        .exec(render::Action::SetTexGenConfig(config));
}

/// Sets the value of an internal transform unit register.
pub fn set_register(sys: &mut System, reg: Reg, value: u32) {
    tracing::debug!("wrote {value:02X} to internal XF register {reg:?}");

    let xf = &mut sys.gpu.xform.internal;
    match reg {
        Reg::MatIndexLow => value.write_ne_bytes(&mut xf.default_matrices.as_mut_bytes()[0..4]),
        Reg::MatIndexHigh => value.write_ne_bytes(&mut xf.default_matrices.as_mut_bytes()[4..8]),

        Reg::Ambient0 => {
            xf.ambient[0] = zerocopy::transmute!(value);
            sys.modules
                .render
                .exec(render::Action::SetAmbient(0, xf.ambient[0]));
        }
        Reg::Ambient1 => {
            xf.ambient[1] = zerocopy::transmute!(value);
            sys.modules
                .render
                .exec(render::Action::SetAmbient(1, xf.ambient[1]));
        }
        Reg::Material0 => {
            xf.material[0] = zerocopy::transmute!(value);
            sys.modules
                .render
                .exec(render::Action::SetMaterial(0, xf.material[0]));
        }
        Reg::Material1 => {
            xf.material[1] = zerocopy::transmute!(value);
            sys.modules
                .render
                .exec(render::Action::SetMaterial(1, xf.material[1]));
        }
        Reg::ColorControl0 => {
            xf.color_control[0] = Channel::from_bits(value);
            sys.modules
                .render
                .exec(render::Action::SetColorChannel(0, xf.color_control[0]));
        }
        Reg::ColorControl1 => {
            xf.color_control[1] = Channel::from_bits(value);
            sys.modules
                .render
                .exec(render::Action::SetColorChannel(1, xf.color_control[1]));
        }
        Reg::AlphaControl0 => {
            xf.alpha_control[0] = Channel::from_bits(value);
            sys.modules
                .render
                .exec(render::Action::SetAlphaChannel(0, xf.alpha_control[0]));
        }
        Reg::AlphaControl1 => {
            xf.alpha_control[1] = Channel::from_bits(value);
            sys.modules
                .render
                .exec(render::Action::SetAlphaChannel(1, xf.alpha_control[1]));
        }

        Reg::ViewportScaleX => xf.viewport.width = f32::from_bits(value) * 2.0,
        Reg::ViewportScaleY => xf.viewport.height = f32::from_bits(value) * -2.0,
        Reg::ViewportScaleZ => {
            xf.viewport.far_minus_near = f32::from_bits(value) / DEPTH_24_BIT_MAX as f32
        }
        Reg::ViewportOffsetX => xf.viewport.center_x = f32::from_bits(value) - 342.0,
        Reg::ViewportOffsetY => xf.viewport.center_y = f32::from_bits(value) - 342.0,
        Reg::ViewportOffsetZ => xf.viewport.far = f32::from_bits(value) / DEPTH_24_BIT_MAX as f32,

        Reg::ProjectionParam0 => xf.projection_mtx.params[0] = f32::from_bits(value),
        Reg::ProjectionParam1 => xf.projection_mtx.params[1] = f32::from_bits(value),
        Reg::ProjectionParam2 => xf.projection_mtx.params[2] = f32::from_bits(value),
        Reg::ProjectionParam3 => xf.projection_mtx.params[3] = f32::from_bits(value),
        Reg::ProjectionParam4 => xf.projection_mtx.params[4] = f32::from_bits(value),
        Reg::ProjectionParam5 => xf.projection_mtx.params[5] = f32::from_bits(value),
        Reg::ProjectionOrthographic => xf.projection_mtx.orthographic = value != 0,

        Reg::TexGenCount => xf.active_texgens = value as u8,
        Reg::TexGen0 => xf.texgen[0].base = BaseTexGen::from_bits(value),
        Reg::TexGen1 => xf.texgen[1].base = BaseTexGen::from_bits(value),
        Reg::TexGen2 => xf.texgen[2].base = BaseTexGen::from_bits(value),
        Reg::TexGen3 => xf.texgen[3].base = BaseTexGen::from_bits(value),
        Reg::TexGen4 => xf.texgen[4].base = BaseTexGen::from_bits(value),
        Reg::TexGen5 => xf.texgen[5].base = BaseTexGen::from_bits(value),
        Reg::TexGen6 => xf.texgen[6].base = BaseTexGen::from_bits(value),
        Reg::TexGen7 => xf.texgen[7].base = BaseTexGen::from_bits(value),
        Reg::PostTexGen0 => xf.texgen[0].post = PostTexGen::from_bits(value),
        Reg::PostTexGen1 => xf.texgen[1].post = PostTexGen::from_bits(value),
        Reg::PostTexGen2 => xf.texgen[2].post = PostTexGen::from_bits(value),
        Reg::PostTexGen3 => xf.texgen[3].post = PostTexGen::from_bits(value),
        Reg::PostTexGen4 => xf.texgen[4].post = PostTexGen::from_bits(value),
        Reg::PostTexGen5 => xf.texgen[5].post = PostTexGen::from_bits(value),
        Reg::PostTexGen6 => xf.texgen[6].post = PostTexGen::from_bits(value),
        Reg::PostTexGen7 => xf.texgen[7].post = PostTexGen::from_bits(value),

        _ => tracing::warn!("unimplemented write to internal XF register {reg:?}: {value:08X}"),
    }

    if reg.is_texgen() {
        sys.gpu.xform.internal.stages_dirty = true;
    }

    if reg.is_viewport() {
        sys.gpu.xform.internal.viewport_dirty = true;
    }

    if reg.is_projection_param() {
        sys.modules.render.exec(render::Action::SetProjectionMatrix(
            sys.gpu.xform.internal.projection_mtx,
        ));
    }
}

/// Writes to transform unit memory.
pub fn write(sys: &mut System, addr: u16, value: u32) {
    match addr {
        0x0000..0x0400 => sys.gpu.xform.ram[addr as usize] = value,
        0x0400..0x0460 => sys.gpu.xform.ram[addr as usize] = value.with_bits(0, 12, 0),
        0x0500..0x0600 => {
            sys.gpu.xform.ram[addr as usize] = value;
            sys.gpu.xform.internal.stages_dirty = true;
        }
        0x0600..0x0680 => {
            if matches!(
                addr,
                0x603 | 0x613 | 0x623 | 0x633 | 0x643 | 0x653 | 0x663 | 0x673
            ) {
                sys.gpu.xform.ram[addr as usize] = value;
            } else {
                sys.gpu.xform.ram[addr as usize] = value.with_bits(0, 12, 0);
            }

            if let Some(light_offset) = addr.checked_sub(0x0600) {
                let index = light_offset / 0x10;
                if index < 7 {
                    sys.modules.render.exec(render::Action::SetLight(
                        index as u8,
                        *sys.gpu.xform.light(index as u8),
                    ));
                }
            }
        }
        0x1000..=0x1057 => {
            let register = addr as u8;
            let Some(register) = Reg::from_repr(register) else {
                panic!("unknown XF register {register:02X}");
            };

            self::set_register(sys, register, value);
        }
        _ => tracing::error!("writing to unknown XF memory: {addr:04X}"),
    }
}

/// Writes the contents of an array to transform unit memory.
pub fn write_indexed(sys: &mut System, array: ArrayDescriptor, base: u16, length: u8, index: u16) {
    for offset in 0..length {
        let current = array.address + index as u32 * array.stride + 4 * offset as u32;
        let value = sys.read_phys_slow::<u32>(current);
        self::write(sys, base + offset as u16, value);
    }
}
