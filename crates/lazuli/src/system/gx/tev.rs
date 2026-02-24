//! Texture Environment (TEV).
pub mod alpha;
pub mod color;
pub mod depth;

use ::color::{Rgba8, Rgba16};
use bitos::integer::{u3, u5, u11, u24};
use bitos::{BitUtils, bitos};

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputChannel {
    Channel0            = 0x0,
    Channel1            = 0x1,
    Reserved0           = 0x2,
    Reserved1           = 0x3,
    Reserved2           = 0x4,
    AlphaBump           = 0x5,
    AlphaBumpNormalized = 0x6,
    Zero                = 0x7,
}

#[bitos(10)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StageRefs {
    #[bits(0..3)]
    pub map: u3,
    #[bits(3..6)]
    pub coord: u3,
    #[bits(6)]
    pub map_enable: bool,
    #[bits(7..10)]
    pub input: InputChannel,
}

#[bitos(32)]
#[derive(Debug, Default)]
pub struct StageRefsPair {
    #[bits(0..10)]
    pub a: StageRefs,
    #[bits(12..22)]
    pub b: StageRefs,
}

#[bitos(5)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Constant {
    #[default]
    One         = 0x00,
    SevenEights = 0x01,
    SixEights   = 0x02,
    FiveEights  = 0x03,
    FourEights  = 0x04,
    ThreeEights = 0x05,
    TwoEights   = 0x06,
    OneEight    = 0x07,
    Reserved0   = 0x08,
    Reserved1   = 0x09,
    Reserved2   = 0x0A,
    Reserved3   = 0x0B,
    Const0      = 0x0C,
    Const1      = 0x0D,
    Const2      = 0x0E,
    Const3      = 0x0F,
    Const0R     = 0x10,
    Const1R     = 0x11,
    Const2R     = 0x12,
    Const3R     = 0x13,
    Const0G     = 0x14,
    Const1G     = 0x15,
    Const2G     = 0x16,
    Const3G     = 0x17,
    Const0B     = 0x18,
    Const1B     = 0x19,
    Const2B     = 0x1A,
    Const3B     = 0x1B,
    Const0A     = 0x1C,
    Const1A     = 0x1D,
    Const2A     = 0x1E,
    Const3A     = 0x1F,
}

#[bitos(32)]
#[derive(Debug, Default)]
pub struct StageConstsPair {
    #[bits(4..9)]
    pub color_a: Constant,
    #[bits(9..14)]
    pub alpha_a: Constant,
    #[bits(14..19)]
    pub color_b: Constant,
    #[bits(19..24)]
    pub alpha_b: Constant,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    Zero         = 0b00,
    PositiveHalf = 0b01,
    NegativeHalf = 0b10,
    Comparative  = 0b11,
}

impl Bias {
    pub fn value(self) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::PositiveHalf => 0.5,
            Self::NegativeHalf => -0.5,
            _ => panic!("comparative tev stage"),
        }
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    One  = 0b00,
    Two  = 0b01,
    Four = 0b10,
    Half = 0b11,
}

impl Scale {
    pub fn value(self) -> f32 {
        match self {
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
            Self::Half => 0.5,
        }
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    GreaterThan = 0b0,
    Equal       = 0b1,
}

impl std::fmt::Display for CompareOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GreaterThan => f.write_str(">"),
            Self::Equal => f.write_str("=="),
        }
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareTarget {
    R8        = 0b00,
    GR16      = 0b01,
    BGR16     = 0b10,
    Component = 0b11,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputDst {
    R3 = 0b00,
    R0 = 0b01,
    R1 = 0b10,
    R2 = 0b11,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StageOps {
    pub color: color::Stage,
    pub alpha: alpha::Stage,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FogParamA {
    #[bits(0..11)]
    pub mantissa: u11,
    #[bits(11..19)]
    pub exponent: u8,
    #[bits(19)]
    pub negative: bool,
}

impl FogParamA {
    pub fn value(self) -> f32 {
        let mut value = 0;
        value.set_bits(23 - 11, 23, self.mantissa().value() as u32);
        value.set_bits(23, 31, self.exponent() as u32);
        value.set_bit(31, self.negative());

        f32::from_bits(value)
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FogParamB0 {
    #[bits(0..24)]
    pub magnitude: u24,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FogParamB1 {
    #[bits(0..5)]
    pub shift: u5,
}

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum FogMode {
    #[default]
    None               = 0x0,
    Reserved0          = 0x1,
    Linear             = 0x2,
    Reserved1          = 0x3,
    Exponential        = 0x4,
    ExponentialSquared = 0x5,
    InverseExponential = 0x6,
    InverseExponentialSquared = 0x7,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FogParamC {
    #[bits(0..11)]
    pub mantissa: u11,
    #[bits(11..19)]
    pub exponent: u8,
    #[bits(19)]
    pub negative: bool,
    #[bits(20)]
    pub orthographic: bool,
    #[bits(21..24)]
    pub mode: FogMode,
}

impl FogParamC {
    pub fn value(self) -> f32 {
        let mut value = 0;
        value.set_bits(23 - 11, 23, self.mantissa().value() as u32);
        value.set_bits(23, 31, self.exponent() as u32);
        value.set_bit(31, self.negative());

        f32::from_bits(value)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Fog {
    pub a: FogParamA,
    pub b0: FogParamB0,
    pub b1: FogParamB1,
    pub c: FogParamC,
    pub color: Rgba8,
}

impl Fog {
    pub fn value_a(&self) -> f32 {
        self.a.value()
    }

    pub fn value_b(&self) -> f32 {
        let mantissa = self.b0.magnitude().value() as f32 / (((1 << 23) - 1) as f32);
        let exp = 2f32.powi(self.b1.shift().value() as i32 - 1);
        mantissa * exp
    }

    pub fn value_c(&self) -> f32 {
        self.c.value()
    }
}

#[derive(Debug, Default)]
pub struct Interface {
    pub stage_ops: [StageOps; 16],
    pub stage_refs: [StageRefsPair; 8],
    pub stage_consts: [StageConstsPair; 8],
    pub constants: [Rgba16; 4],
    pub alpha_func: alpha::Function,
    pub depth_tex: depth::Texture,
    pub fog: Fog,
    pub stages_dirty: bool,
}
