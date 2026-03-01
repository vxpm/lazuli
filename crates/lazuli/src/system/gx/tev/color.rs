use bitos::bitos;

use crate::system::gx::tev::{Bias, ComparisonOp, ComparisonTarget, OutputDst, Scale};

#[bitos(4)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSrc {
    R3Color   = 0x0,
    R3Alpha   = 0x1,
    R0Color   = 0x2,
    R0Alpha   = 0x3,
    R1Color   = 0x4,
    R1Alpha   = 0x5,
    R2Color   = 0x6,
    R2Alpha   = 0x7,
    TexColor  = 0x8,
    TexAlpha  = 0x9,
    ChanColor = 0xA,
    ChanAlpha = 0xB,
    One       = 0xC,
    Half      = 0xD,
    Constant  = 0xE,
    Zero      = 0xF,
}

impl std::fmt::Display for InputSrc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::R3Color => "R3.C",
            Self::R3Alpha => "R3.A",
            Self::R0Color => "R0.C",
            Self::R0Alpha => "R0.A",
            Self::R1Color => "R1.C",
            Self::R1Alpha => "R1.A",
            Self::R2Color => "R2.C",
            Self::R2Alpha => "R2.A",
            Self::TexColor => "Tex.C",
            Self::TexAlpha => "Tex.A",
            Self::ChanColor => "Channel.C",
            Self::ChanAlpha => "Channel.A",
            Self::One => "1",
            Self::Half => "0.5",
            Self::Constant => "Constant",
            Self::Zero => "0",
        })
    }
}

#[bitos(32)]
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Stage {
    #[bits(0..4)]
    pub input_d: InputSrc,
    #[bits(4..8)]
    pub input_c: InputSrc,
    #[bits(8..12)]
    pub input_b: InputSrc,
    #[bits(12..16)]
    pub input_a: InputSrc,
    #[bits(16..18)]
    pub bias: Bias,
    #[bits(18)]
    pub negate: bool,
    #[bits(18)]
    pub comparison_op: ComparisonOp,
    #[bits(19)]
    pub clamp: bool,
    #[bits(20..22)]
    pub scale: Scale,
    #[bits(20..22)]
    pub comparison_target: ComparisonTarget,
    #[bits(22..24)]
    pub output: OutputDst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagePattern {
    PassChanColor,
    PassChanAlpha,
    PassTexColor,
    PassTexAlpha,
    Modulate,
    ModulateDouble,
    Add,
    SubTexFromColor,
    SubColorFromTex,
    Mix,
}

impl Stage {
    pub fn is_comparative(&self) -> bool {
        self.bias() == Bias::Comparative
    }

    pub fn pattern(&self) -> Option<StagePattern> {
        use {InputSrc as Input, StagePattern as Pattern};

        if self.is_comparative() {
            return None;
        }

        let inputs = (
            self.input_a(),
            self.input_b(),
            self.input_c(),
            self.input_d(),
        );
        let positive = !self.negate();
        let scale = self.scale();
        let bias = self.bias();

        let no_scale_no_bias = scale == Scale::One && bias == Bias::Zero;
        let simple = positive && no_scale_no_bias;

        Some(match inputs {
            (Input::Zero, Input::Zero, Input::Zero, Input::ChanColor) if simple => {
                Pattern::PassChanColor
            }
            (Input::Zero, Input::Zero, Input::Zero, Input::ChanAlpha) if simple => {
                Pattern::PassChanAlpha
            }
            (Input::Zero, Input::Zero, Input::Zero, Input::TexColor) if simple => {
                Pattern::PassTexColor
            }
            (Input::Zero, Input::Zero, Input::Zero, Input::TexAlpha) if simple => {
                Pattern::PassTexAlpha
            }
            (Input::Zero, Input::TexColor, Input::ChanColor, Input::Zero) if simple => {
                Pattern::Modulate
            }
            (Input::Zero, Input::ChanColor, Input::TexColor, Input::Zero) if simple => {
                Pattern::Modulate
            }
            (Input::Zero, Input::TexColor, Input::ChanColor, Input::Zero)
                if positive && scale == Scale::Two && bias == Bias::Zero =>
            {
                Pattern::ModulateDouble
            }
            (Input::Zero, Input::ChanColor, Input::TexColor, Input::Zero)
                if positive && scale == Scale::Two && bias == Bias::Zero =>
            {
                Pattern::ModulateDouble
            }
            (Input::TexColor, Input::Zero, Input::Zero, Input::ChanColor) if simple => Pattern::Add,
            (Input::ChanColor, Input::Zero, Input::Zero, Input::TexColor) if simple => Pattern::Add,
            (Input::TexColor, Input::Zero, Input::Zero, Input::ChanColor)
                if no_scale_no_bias && !positive =>
            {
                Pattern::SubTexFromColor
            }
            (Input::ChanColor, Input::Zero, Input::Zero, Input::TexColor)
                if no_scale_no_bias && !positive =>
            {
                Pattern::SubColorFromTex
            }
            (Input::TexColor, Input::ChanColor, Input::TexAlpha, Input::Zero) if simple => {
                Pattern::Mix
            }
            (Input::ChanColor, Input::TexColor, Input::ChanAlpha, Input::Zero) if simple => {
                Pattern::Mix
            }
            _ => return None,
        })
    }
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pattern = self
            .pattern()
            .map(|p| format!(" [{p:?}]"))
            .unwrap_or_default();

        if self.is_comparative() {
            let a = self.input_a();
            let b = self.input_b();
            let c = self.input_c();
            let d = self.input_d();
            let op = self.comparison_op();
            let target = self.comparison_target();
            let output = self.output();

            write!(
                f,
                "{output:?}.C = ({a}.{target:?} {op} {b}.{target:?}) ? {c} : {d}{pattern}"
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
                "{output:?}.C = {scale} * ({sign}mix({a}, {b}, {c}){d}{bias}){pattern}"
            )
        }
    }
}
