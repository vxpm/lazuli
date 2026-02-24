use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev::FogMode;
use lazuli::system::gx::xform::BaseTexGen;
use lazuli::system::gx::{CullingMode, tev};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlendSettings {
    pub enabled: bool,
    pub src: wgpu::BlendFactor,
    pub dst: wgpu::BlendFactor,
    pub op: wgpu::BlendOperation,

    pub color_write: bool,
    pub alpha_write: bool,
}

impl Default for BlendSettings {
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
pub struct DepthSettings {
    pub enabled: bool,
    pub compare: wgpu::CompareFunction,
    pub write: bool,
}

impl Default for DepthSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            compare: wgpu::CompareFunction::Less,
            write: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlphaCompareValue {
    False,
    True,
    Unknown,
}

impl AlphaCompareValue {
    pub fn new(alpha_compare: tev::alpha::Compare) -> Self {
        match alpha_compare {
            tev::alpha::Compare::Never => Self::False,
            tev::alpha::Compare::Always => Self::True,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitAnd for AlphaCompareValue {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, Self::True) => Self::True,
            (Self::False, _) => Self::False,
            (_, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitOr for AlphaCompareValue {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, _) => Self::True,
            (_, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitXor for AlphaCompareValue {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, Self::False) => Self::True,
            (Self::False, Self::True) => Self::True,
            (Self::True, Self::True) => Self::False,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::Not for AlphaCompareValue {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::False => Self::True,
            Self::True => Self::False,
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AlphaFuncSettings {
    pub comparison: [tev::alpha::Compare; 2],
    pub logic: tev::alpha::CompareLogic,
}

impl AlphaFuncSettings {
    /// Returns whether this configuration is trivially passable (i.e. never discards).
    pub fn is_noop(&self) -> bool {
        let lhs = AlphaCompareValue::new(self.comparison[0]);
        let rhs = AlphaCompareValue::new(self.comparison[1]);

        let result = match self.logic {
            tev::alpha::CompareLogic::And => lhs & rhs,
            tev::alpha::CompareLogic::Or => lhs | rhs,
            tev::alpha::CompareLogic::Xor => lhs ^ rhs,
            tev::alpha::CompareLogic::Xnor => !(lhs ^ rhs),
        };

        result == AlphaCompareValue::True
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FogSettings {
    pub mode: FogMode,
    pub orthographic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexEnvSettings {
    pub stages: Vec<TexEnvStage>,
    pub alpha_func: AlphaFuncSettings,
    pub depth_tex: tev::depth::Texture,
    pub fog: FogSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexGenStageSettings {
    pub base: BaseTexGen,
    pub normalize: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexGenSettings {
    pub stages: Vec<TexGenStageSettings>,
}

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct ShaderSettings {
    pub texenv: TexEnvSettings,
    pub texgen: TexGenSettings,
}

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Settings {
    pub has_alpha: bool,
    pub culling: CullingMode,
    pub blend: BlendSettings,
    pub depth: DepthSettings,
    pub shader: ShaderSettings,
}
