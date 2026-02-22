//! Texture Environment (TEV).
pub mod alpha;
pub mod color;
pub mod depth;

use ::color::Rgba16;
use bitos::bitos;
use bitos::integer::u3;

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
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
    pub input: Input,
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

#[derive(Debug, Default)]
pub struct Interface {
    pub active_stages: u8,
    pub active_channels: u8,
    pub stage_ops: [StageOps; 16],
    pub stage_refs: [StageRefsPair; 8],
    pub stage_consts: [StageConstsPair; 8],
    pub constants: [Rgba16; 4],
    pub alpha_func: alpha::Function,
    pub depth_tex: depth::Texture,
    pub stages_dirty: bool,
}
