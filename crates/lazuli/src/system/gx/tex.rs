//! Texture unit (TX).
use std::collections::HashMap;

use bitos::bitos;
use bitos::integer::{u2, u3, u10, u11};
use color::Rgba8;
use gekko::Address;
use gxtex::PaletteIndex;

use crate::modules::render;
use crate::system::System;
use crate::system::gx::DEPTH_24_BIT_MAX;
use crate::system::gx::pix::{ColorCopyFormat, DepthCopyFormat};

#[derive(Debug, Clone)]
enum LodData {
    Direct(Vec<Rgba8>),
    Indirect(Vec<PaletteIndex>),
}

#[derive(Debug, Clone)]
pub enum TextureData {
    Direct(Vec<Vec<Rgba8>>),
    Indirect(Vec<Vec<PaletteIndex>>),
}

impl TextureData {
    fn push(&mut self, lod: LodData) {
        match (self, lod) {
            (Self::Direct(lods), LodData::Direct(lod)) => lods.push(lod),
            (Self::Indirect(lods), LodData::Indirect(lod)) => lods.push(lod),
            _ => panic!("mismatched texture and lod formats - this is definitely a bug"),
        }
    }

    pub fn lod_count(&self) -> u32 {
        match self {
            Self::Direct(lods) => lods.len() as u32,
            Self::Indirect(lods) => lods.len() as u32,
        }
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapMode {
    #[default]
    Clamp    = 0x0,
    Repeat   = 0x1,
    Mirror   = 0x2,
    Reserved = 0x3,
}

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MinFilter {
    #[default]
    Near            = 0x0,
    NearMipNear     = 0x1,
    NearMipLinear   = 0x2,
    Reserved0       = 0x3,
    Linear          = 0x4,
    LinearMipNear   = 0x5,
    LinearMipLinear = 0x6,
    Reserved        = 0x7,
}

impl MinFilter {
    pub fn is_linear(&self) -> bool {
        matches!(
            self,
            Self::Linear | Self::LinearMipNear | Self::LinearMipLinear
        )
    }

    pub fn uses_lods(&self) -> bool {
        matches!(
            self,
            Self::NearMipNear | Self::NearMipLinear | Self::LinearMipNear | Self::LinearMipLinear
        )
    }
}

#[bitos(4)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    I4        = 0x0,
    I8        = 0x1,
    IA4       = 0x2,
    IA8       = 0x3,
    RGB565    = 0x4,
    RGB5A3    = 0x5,
    RGBA8     = 0x6,
    Reserved0 = 0x7,
    CI4       = 0x8,
    CI8       = 0x9,
    CI14X2    = 0xA,
    Reserved1 = 0xB,
    Reserved2 = 0xC,
    Reserved3 = 0xD,
    Cmpr      = 0xE,
    Reserved4 = 0xF,
}

impl Format {
    pub fn is_direct(&self) -> bool {
        !matches!(self, Self::CI4 | Self::CI8 | Self::CI14X2)
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SamplerMode {
    #[bits(0..2)]
    pub wrap_u: WrapMode,
    #[bits(2..4)]
    pub wrap_v: WrapMode,
    #[bits(4)]
    pub mag_linear: bool,
    #[bits(5..8)]
    pub min_filter: MinFilter,
    #[bits(8)]
    pub diagonal_lod: bool,
    #[bits(9..17)]
    pub lod_bias_raw: u8,
    #[bits(19..21)]
    pub max_anisotropy_log2: u2,
    #[bits(21)]
    pub lod_and_bias_clamp: bool,
}

impl SamplerMode {
    pub fn lod_bias(&self) -> f32 {
        let raw = self.lod_bias_raw() as i8 as i32;
        let bias = raw as f32 / 32.0;
        assert!(bias >= -4.0);
        assert!(bias <= 4.0);

        bias
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Encoding {
    #[bits(0..10)]
    pub width_minus_one: u10,
    #[bits(10..20)]
    pub height_minus_one: u10,
    #[bits(20..24)]
    pub format: Format,
}

impl Encoding {
    pub fn width(&self) -> u32 {
        self.width_minus_one().value() as u32 + 1
    }

    pub fn height(&self) -> u32 {
        self.height_minus_one().value() as u32 + 1
    }

    pub fn lod_count(&self) -> u32 {
        self.width().ilog2().max(self.height().ilog2()) + 1
    }

    pub fn length_for(width: u32, height: u32, format: Format) -> u32 {
        use gxtex::{CI4, CI8, CI14X2, Cmpr, I4, I8, IA4, IA8, Rgb5A3, Rgb565, Rgba8};

        let width = width as usize;
        let height = height as usize;

        (match format {
            Format::I4 => gxtex::compute_size::<I4>(width, height),
            Format::I8 => gxtex::compute_size::<I8>(width, height),
            Format::IA4 => gxtex::compute_size::<IA4>(width, height),
            Format::IA8 => gxtex::compute_size::<IA8>(width, height),
            Format::RGB565 => gxtex::compute_size::<Rgb565>(width, height),
            Format::RGB5A3 => gxtex::compute_size::<Rgb5A3>(width, height),
            Format::RGBA8 => gxtex::compute_size::<Rgba8>(width, height),
            Format::Cmpr => gxtex::compute_size::<Cmpr>(width, height),
            Format::CI4 => gxtex::compute_size::<CI4>(width, height),
            Format::CI8 => gxtex::compute_size::<CI8>(width, height),
            Format::CI14X2 => gxtex::compute_size::<CI14X2>(width, height),
            _ => todo!("format {:?}", format),
        }) as u32
    }

    // Size, in bytes, of the texture.
    pub fn length(&self) -> u32 {
        Self::length_for(self.width(), self.height(), self.format())
    }

    // Size, in bytes, of the texture, considering it as a mipmap.
    pub fn length_mipmap(&self) -> u32 {
        let mut current_width = self.width();
        let mut current_height = self.height();

        let mut size = 0;
        for _ in 0..self.lod_count() {
            size += Self::length_for(current_width, current_height, self.format());
            current_width = (current_width / 2).max(1);
            current_height = (current_height / 2).max(1);
        }

        size
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScaleU {
    #[bits(0..16)]
    pub scale_minus_one: u16,
    #[bits(16)]
    pub range_bias_enable: bool,
    #[bits(17)]
    pub cylindrical_wrapping: bool,
    #[bits(18)]
    pub offset_lines: bool,
    #[bits(19)]
    pub offset_points: bool,
}

impl ScaleU {
    pub fn scale(&self) -> Option<u32> {
        let scale = self.scale_minus_one();
        (self.scale_minus_one() != 0).then_some(scale as u32 + 1)
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScaleV {
    #[bits(0..16)]
    pub scale_minus_one: u16,
    #[bits(16)]
    pub range_bias_enable: bool,
    #[bits(17)]
    pub cylindrical_wrapping: bool,
}

impl ScaleV {
    pub fn scale(&self) -> Option<u32> {
        let scale = self.scale_minus_one();
        (self.scale_minus_one() != 0).then_some(scale as u32 + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scaling {
    pub u: ScaleU,
    pub v: ScaleV,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct OddLod {
    #[bits(18..21)]
    pub cache_height: u3,
}

impl OddLod {
    pub fn has_lods(&self) -> bool {
        self.cache_height().value() != 0
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LodLimits {
    #[bits(0..8)]
    pub min_raw: u8,
    #[bits(8..16)]
    pub max_raw: u8,
}

impl LodLimits {
    pub fn min(&self) -> f32 {
        self.min_raw() as f32 / 16.0
    }

    pub fn max(&self) -> f32 {
        self.max_raw() as f32 / 16.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Lods {
    pub limits: LodLimits,
    pub odd: OddLod,
}

#[derive(Debug, Clone, Default)]
pub struct TextureMap {
    pub address: Address,
    pub encoding: Encoding,
    pub sampler: SamplerMode,
    pub scaling: Scaling,
    pub clut: ClutRef,
    pub lods: Lods,
    pub dirty: bool,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClutFormat {
    #[default]
    IA8       = 0b00,
    RGB565    = 0b01,
    RGB5A3    = 0b10,
    Reserved0 = 0b11,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ClutLoad {
    #[bits(0..10)]
    pub tmem_offset: u10,
    #[bits(10..21)]
    pub count: u11,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ClutRef {
    #[bits(0..10)]
    pub tmem_offset: u10,
    #[bits(10..12)]
    pub format: ClutFormat,
}

#[derive(Default)]
pub struct Interface {
    pub maps: [TextureMap; 8],
    pub clut_addr: Address,
    pub clut_load: ClutLoad,
    pub tex_cache: HashMap<Address, u64>,
    pub clut_cache: HashMap<Address, u64>,
}

impl std::fmt::Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface")
            .field("maps", &self.maps)
            .field("cache", &self.tex_cache)
            .finish()
    }
}

impl Interface {
    pub fn update_tex_hash(&mut self, addr: Address, data: &[u8]) -> bool {
        let new_hash = twox_hash::XxHash3_64::oneshot(data);
        let Some(old_hash) = self.tex_cache.get(&addr) else {
            self.tex_cache.insert(addr, new_hash);
            return true;
        };

        if *old_hash == new_hash {
            false
        } else {
            self.tex_cache.insert(addr, new_hash);
            true
        }
    }

    pub fn update_clut_hash(&mut self, addr: Address, data: &[u8]) -> bool {
        let new_hash = twox_hash::XxHash3_64::oneshot(data);
        let Some(old_hash) = self.clut_cache.get(&addr) else {
            self.clut_cache.insert(addr, new_hash);
            return true;
        };

        if *old_hash == new_hash {
            false
        } else {
            self.clut_cache.insert(addr, new_hash);
            true
        }
    }
}

/// Decodes a planar texture.
fn decode_planar(data: &[u8], width: u32, height: u32, format: Format) -> LodData {
    use gxtex::{
        AlphaChannel, CI4, CI8, CI14X2, Cmpr, FastLuma, FastRgb565, I4, I8, IA4, IA8, Rgb5A3,
        Rgba8, decode,
    };

    let width = width as usize;
    let height = height as usize;

    match format {
        Format::I4 => LodData::Direct(decode::<I4<FastLuma>>(width, height, data)),
        Format::IA4 => LodData::Direct(decode::<IA4<FastLuma, AlphaChannel>>(width, height, data)),
        Format::I8 => LodData::Direct(decode::<I8<FastLuma>>(width, height, data)),
        Format::IA8 => LodData::Direct(decode::<IA8<FastLuma, AlphaChannel>>(width, height, data)),
        Format::RGB565 => LodData::Direct(decode::<FastRgb565>(width, height, data)),
        Format::RGB5A3 => LodData::Direct(decode::<Rgb5A3>(width, height, data)),
        Format::RGBA8 => LodData::Direct(decode::<Rgba8>(width, height, data)),
        Format::Cmpr => LodData::Direct(decode::<Cmpr>(width, height, data)),
        Format::CI4 => LodData::Indirect(decode::<CI4>(width, height, data)),
        Format::CI8 => LodData::Indirect(decode::<CI8>(width, height, data)),
        Format::CI14X2 => LodData::Indirect(decode::<CI14X2>(width, height, data)),
        _ => todo!("reserved texture format"),
    }
}

/// Decodes a mipmap texture with `count` levels.
fn decode_mipmap(
    data: &[u8],
    width: u32,
    height: u32,
    format: Format,
    count: usize,
) -> TextureData {
    let mut mipmap = if format.is_direct() {
        TextureData::Direct(Vec::with_capacity(count))
    } else {
        TextureData::Indirect(Vec::with_capacity(count))
    };

    let mut current_data = data;
    let mut current_width = width;
    let mut current_height = height;
    for _ in 0..count {
        mipmap.push(self::decode_planar(
            current_data,
            current_width,
            current_height,
            format,
        ));

        let consumed = Encoding::length_for(current_width, current_height, format) as usize;
        current_data = &current_data[consumed..];
        current_width = (current_width / 2).max(1);
        current_height = (current_height / 2).max(1);
    }

    mipmap
}

pub fn encode_color_texture(
    texels: Vec<u32>,
    format: ColorCopyFormat,
    stride: u32,
    width: u32,
    height: u32,
    output: &mut [u8],
) {
    use gxtex::{
        AlphaChannel, BlueChannel, FastLuma, FastRgb565, GreenChannel, I4, I8, IA4, IA8,
        RedChannel, Rgb5A3, Rgba8, encode,
    };

    let pixels = texels
        .into_iter()
        .map(|c| zerocopy::transmute!(c))
        .collect::<Vec<_>>();

    macro_rules! encode {
        ($fmt:ty) => {
            encode::<$fmt>(
                stride as usize,
                width as usize,
                height as usize,
                &pixels,
                output,
            )
        };
    }

    match format {
        ColorCopyFormat::R4 => encode!(I4<RedChannel>),
        ColorCopyFormat::Y8 => encode!(I8<FastLuma>),
        ColorCopyFormat::RA4 => encode!(IA4<RedChannel, AlphaChannel>),
        ColorCopyFormat::RA8 => encode!(IA8<RedChannel, AlphaChannel>),
        ColorCopyFormat::RGB565 => encode!(FastRgb565),
        ColorCopyFormat::RGB5A3 => encode!(Rgb5A3),
        ColorCopyFormat::RGBA8 => encode!(Rgba8),
        ColorCopyFormat::A8 => encode!(I8<AlphaChannel>),
        ColorCopyFormat::R8 => encode!(I8<RedChannel>),
        ColorCopyFormat::G8 => encode!(I8<GreenChannel>),
        ColorCopyFormat::B8 => encode!(I8<BlueChannel>),
        ColorCopyFormat::RG8 => encode!(IA8<RedChannel, GreenChannel>),
        ColorCopyFormat::GB8 => encode!(IA8<GreenChannel, BlueChannel>),
        _ => panic!("reserved color format"),
    }
}

pub fn encode_depth_texture(
    data: Vec<u32>,
    format: DepthCopyFormat,
    stride: u32,
    width: u32,
    height: u32,
    output: &mut [u8],
) {
    use gxtex::{BlueChannel, GreenChannel, I8, IA8, RedChannel, Rgba8, encode};

    let depth = data
        .into_iter()
        .map(f32::from_bits)
        .map(|x| (x * DEPTH_24_BIT_MAX as f32) as u32)
        .map(u32::to_le_bytes)
        .map(|c| gxtex::Pixel {
            r: c[2], // high
            g: c[1], // mid
            b: c[0], // low
            a: 0,
        })
        .collect::<Vec<_>>();

    macro_rules! encode {
        ($fmt:ty) => {
            encode::<$fmt>(
                stride as usize,
                width as usize,
                height as usize,
                &depth,
                output,
            )
        };
    }

    match format {
        DepthCopyFormat::Z4 => todo!(),
        DepthCopyFormat::Z8 => encode!(I8<RedChannel>), // not sure...
        DepthCopyFormat::Z16C => encode!(IA8<RedChannel, GreenChannel>),
        DepthCopyFormat::Z24X8 => encode!(Rgba8),
        DepthCopyFormat::Z8H => encode!(I8<RedChannel>),
        DepthCopyFormat::Z8M => encode!(I8<GreenChannel>),
        DepthCopyFormat::Z8L => encode!(I8<BlueChannel>),
        DepthCopyFormat::Z16A => encode!(IA8<RedChannel, GreenChannel>),
        DepthCopyFormat::Z16B => encode!(IA8<GreenChannel, RedChannel>),
        _ => panic!("reserved depth format"),
    }
}

pub fn update_texture(sys: &mut System, index: usize) {
    let map = sys.gpu.tex.maps[index].clone();
    let base = map.address;
    let width = map.encoding.width();
    let height = map.encoding.height();
    let format = map.encoding.format();
    let texture_id = render::TextureId(base.value());
    let clut_id = render::ClutId(map.clut.tmem_offset().value());
    let clut_fmt = map.clut.format();

    let (len, lods) = if map.sampler.min_filter().uses_lods() {
        (
            map.encoding.length_mipmap() as usize,
            map.encoding.lod_count() as usize,
        )
    } else {
        (map.encoding.length() as usize, 1)
    };

    let data = &sys.mem.ram()[base.value() as usize..][..len];
    if sys.gpu.tex.update_tex_hash(base, data) {
        let data = self::decode_mipmap(data, width, height, format, lods);
        sys.modules.render.exec(render::Action::LoadTexture {
            id: texture_id,
            texture: render::Texture {
                width,
                height,
                format,
                data,
            },
        });
    }

    let scale_u = map.scaling.u.scale().unwrap_or(width) as f32 / width as f32;
    let scale_v = map.scaling.v.scale().unwrap_or(height) as f32 / height as f32;

    sys.modules.render.exec(render::Action::SetTextureSlot {
        slot: index,
        texture_id,
        clut_ref: render::ClutRef {
            id: clut_id,
            fmt: clut_fmt,
        },
        sampler: render::Sampler {
            mode: map.sampler,
            lods: map.lods.limits,
        },
        scaling: render::Scaling {
            u: scale_u,
            v: scale_v,
        },
    });
}

pub fn update_clut(sys: &mut System) {
    let load = sys.gpu.tex.clut_load;
    let clut_addr = render::ClutId(load.tmem_offset().value());

    let base = sys.gpu.tex.clut_addr;
    let len = load.count().value() as usize * 16 * 2;
    let data = &sys.mem.ram()[base.value() as usize..][..len];

    if sys.gpu.tex.update_clut_hash(base, data) {
        let clut = data
            .chunks_exact(2)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
            .collect();

        sys.modules.render.exec(render::Action::LoadClut {
            id: clut_addr,
            clut: render::ClutData(clut),
        });
    }
}
