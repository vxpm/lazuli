use bitos::bitos;
use bitos::integer::u2;

use crate::system::gx::tev::{Bias, ComparisonOp, ComparisonTarget, OutputDst, Scale};

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSrc {
    R3Alpha   = 0x0,
    R0Alpha   = 0x1,
    R1Alpha   = 0x2,
    R2Alpha   = 0x3,
    TexAlpha  = 0x4,
    ChanAlpha = 0x5,
    Constant  = 0x6,
    Zero      = 0x7,
}

impl std::fmt::Display for InputSrc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::R3Alpha => "R3.A",
            Self::R0Alpha => "R0.A",
            Self::R1Alpha => "R1.A",
            Self::R2Alpha => "R2.A",
            Self::TexAlpha => "Tex.A",
            Self::ChanAlpha => "Channel.A",
            Self::Constant => "Constant",
            Self::Zero => "0",
        })
    }
}

#[bitos(32)]
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Stage {
    #[bits(0..2)]
    pub rasterizer_swap: u2,
    #[bits(2..4)]
    pub texture_swap: u2,
    #[bits(4..7)]
    pub input_d: InputSrc,
    #[bits(7..10)]
    pub input_c: InputSrc,
    #[bits(10..13)]
    pub input_b: InputSrc,
    #[bits(13..16)]
    pub input_a: InputSrc,
    #[bits(16..18)]
    pub bias: Bias,
    #[bits(18)]
    pub negate: bool,
    #[bits(18)]
    pub compare_op: ComparisonOp,
    #[bits(19)]
    pub clamp: bool,
    #[bits(20..22)]
    pub scale: Scale,
    #[bits(20..22)]
    pub compare_target: ComparisonTarget,
    #[bits(22..24)]
    pub output: OutputDst,
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_comparative() {
            let a = self.input_a();
            let b = self.input_b();
            let c = self.input_c();
            let d = self.input_d();
            let op = self.compare_op();
            let target = self.compare_target();
            let output = self.output();

            write!(
                f,
                "{output:?}.A = ({a}.{target:?} {op} {b}.{target:?}) ? {c} : {d}"
            )
        } else {
            let a = self.input_a();
            let b = self.input_b();
            let c = self.input_c();
            let d = self.input_d();
            let sign = if self.negate() { "-" } else { "" };
            let bias = self.bias();
            let scale = self.scale().value();
            let output = self.output();

            let d = if d != InputSrc::Zero {
                format!(" + {d}")
            } else {
                String::new()
            };

            let bias = if bias != Bias::Zero {
                format!(" + {}", bias.value())
            } else {
                String::new()
            };

            write!(
                f,
                "{output:?}.A = {scale} * ({sign}mix({a}, {b}, {c}){d}{bias})"
            )
        }
    }
}

impl Stage {
    pub fn is_comparative(&self) -> bool {
        self.bias() == Bias::Comparative
    }
}

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Compare {
    #[default]
    Never          = 0x0,
    Less           = 0x1,
    Equal          = 0x2,
    LessOrEqual    = 0x3,
    Greater        = 0x4,
    NotEqual       = 0x5,
    GreaterOrEqual = 0x6,
    Always         = 0x7,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CompareLogic {
    #[default]
    And  = 0b00,
    Or   = 0b01,
    Xor  = 0b10,
    Xnor = 0b11,
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Function {
    #[bits(0..16)]
    pub refs: [u8; 2],
    #[bits(16..22)]
    pub comparison: [Compare; 2],
    #[bits(22..24)]
    pub logic: CompareLogic,
}
