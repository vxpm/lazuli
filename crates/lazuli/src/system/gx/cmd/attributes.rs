//! Vertex attribute parsing.
use bitos::integer::u5;
use bitos::{BitUtils, bitos};
use color::Rgba;
use glam::{Vec2, Vec3};

use crate::stream::BinReader;
use crate::system::gx::cmd::{ArrayDescriptor, Arrays, VertexDescriptor};

/// A vertex attribute descriptor. The descriptor defines how the attribute is encoded.
pub trait AttributeDescriptor: std::fmt::Debug {
    /// The value type of this attribute.
    type Value;

    /// Size of a value of this attribute in an attribute stream.
    fn size(&self) -> u32;

    /// Reads a value defined by this descriptor from binary data.
    fn read(&self, reader: &mut BinReader) -> Option<Self::Value>;
}

#[derive(Debug)]
pub struct IndexDescriptor;

impl AttributeDescriptor for IndexDescriptor {
    type Value = u8;

    fn size(&self) -> u32 {
        1
    }

    fn read(&self, reader: &mut BinReader) -> Option<Self::Value> {
        reader.read_be::<u8>()
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionKind {
    /// Two components (x, y).
    #[default]
    Vec2 = 0b0,
    /// Three components (x, y, z).
    Vec3 = 0b1,
}

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoordsFormat {
    #[default]
    U8        = 0b000,
    I8        = 0b001,
    U16       = 0b010,
    I16       = 0b011,
    F32       = 0b100,
    Reserved0 = 0b101,
    Reserved1 = 0b110,
    Reserved2 = 0b111,
}

impl CoordsFormat {
    pub fn size(self) -> u32 {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::F32 => 4,
            _ => panic!("reserved format"),
        }
    }
}

#[bitos(9)]
#[derive(Debug, Clone, Default)]
pub struct PositionDescriptor {
    #[bits(0)]
    pub kind: PositionKind,
    #[bits(1..4)]
    pub format: CoordsFormat,
    #[bits(4..9)]
    pub shift: u5,
}

impl AttributeDescriptor for PositionDescriptor {
    type Value = Vec3;

    fn size(&self) -> u32 {
        match self.kind() {
            PositionKind::Vec2 => 2 * self.format().size(),
            PositionKind::Vec3 => 3 * self.format().size(),
        }
    }

    fn read(&self, reader: &mut BinReader) -> Option<Vec3> {
        let mut component = || {
            let shift = 2.0f32.powi(self.shift().value() as i32);
            Some(match self.format() {
                CoordsFormat::U8 => reader.read_be::<u8>()? as f32 / shift,
                CoordsFormat::I8 => reader.read_be::<i8>()? as f32 / shift,
                CoordsFormat::U16 => reader.read_be::<u16>()? as f32 / shift,
                CoordsFormat::I16 => reader.read_be::<i16>()? as f32 / shift,
                CoordsFormat::F32 => f32::from_bits(reader.read_be::<u32>()?),
                _ => panic!("reserved format"),
            })
        };

        let x = component()?;
        let y = component()?;

        Some(match self.kind() {
            PositionKind::Vec2 => Vec3::new(x, y, 0.0),
            PositionKind::Vec3 => {
                let z = component()?;
                Vec3::new(x, y, z)
            }
        })
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NormalKind {
    /// Three normals.
    #[default]
    N3 = 0b0,
    /// Nine normals.
    N9 = 0b1,
}

#[bitos(4)]
#[derive(Debug, Clone, Default)]
pub struct NormalDescriptor {
    #[bits(0)]
    pub kind: NormalKind,
    #[bits(1..4)]
    pub format: CoordsFormat,
}

impl AttributeDescriptor for NormalDescriptor {
    type Value = Vec3;

    fn size(&self) -> u32 {
        match self.kind() {
            NormalKind::N3 => 3 * self.format().size(),
            NormalKind::N9 => 9 * self.format().size(),
        }
    }

    fn read(&self, reader: &mut BinReader) -> Option<Vec3> {
        let mut component = || {
            let shift_6 = 2.0f32.powi(6);
            let shift_14 = 2.0f32.powi(14);
            Some(match self.format() {
                CoordsFormat::U8 => reader.read_be::<u8>()? as f32 / shift_6,
                CoordsFormat::I8 => reader.read_be::<i8>()? as f32 / shift_6,
                CoordsFormat::U16 => reader.read_be::<u16>()? as f32 / shift_14,
                CoordsFormat::I16 => reader.read_be::<i16>()? as f32 / shift_14,
                CoordsFormat::F32 => f32::from_bits(reader.read_be::<u32>()?),
                _ => panic!("reserved format"),
            })
        };

        let mut vec = || Some(Vec3::new(component()?, component()?, component()?));
        Some(match self.kind() {
            NormalKind::N3 => vec()?,
            NormalKind::N9 => {
                tracing::warn!("parsing binormal and tangent");

                let normal = vec()?;
                vec()?;
                vec()?;

                normal
            }
        })
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorKind {
    /// Three components (r, g, b).
    #[default]
    Rgb  = 0b0,
    /// Four components (r, g, b, a).
    Rgba = 0b1,
}

#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorFormat {
    #[default]
    Rgb565    = 0b000,
    Rgb888    = 0b001,
    Rgb888x   = 0b010,
    Rgba4444  = 0b011,
    Rgba6666  = 0b100,
    Rgba8888  = 0b101,
    Reserved0 = 0b110,
    Reserved1 = 0b111,
}

impl ColorFormat {
    pub fn size(self) -> u32 {
        match self {
            Self::Rgb565 | Self::Rgba4444 => 2,
            Self::Rgb888 | Self::Rgba6666 => 3,
            Self::Rgb888x | Self::Rgba8888 => 4,
            _ => panic!("reserved format"),
        }
    }

    pub fn has_alpha(self) -> bool {
        matches!(self, Self::Rgba4444 | Self::Rgba6666 | Self::Rgba8888)
    }
}

#[bitos(4)]
#[derive(Debug, Clone, Default)]
pub struct ColorDescriptor {
    #[bits(0)]
    pub kind: ColorKind,
    #[bits(1..4)]
    pub format: ColorFormat,
}

impl AttributeDescriptor for ColorDescriptor {
    type Value = Rgba;

    fn size(&self) -> u32 {
        self.format().size()
    }

    fn read(&self, reader: &mut BinReader) -> Option<Rgba> {
        let rgba = match self.format() {
            ColorFormat::Rgb565 => {
                let data = reader.read_be::<u16>()?;
                Rgba::new(
                    data.bits(0, 5) as f32 / 32.0,
                    data.bits(5, 11) as f32 / 64.0,
                    data.bits(11, 16) as f32 / 32.0,
                    1.0,
                )
            }
            ColorFormat::Rgb888 => Rgba::new(
                reader.read_be::<u8>()? as f32 / 255.0,
                reader.read_be::<u8>()? as f32 / 255.0,
                reader.read_be::<u8>()? as f32 / 255.0,
                1.0,
            ),
            ColorFormat::Rgb888x => {
                let color = Rgba::new(
                    reader.read_be::<u8>()? as f32 / 255.0,
                    reader.read_be::<u8>()? as f32 / 255.0,
                    reader.read_be::<u8>()? as f32 / 255.0,
                    1.0,
                );

                // throw away
                _ = reader.read_be::<u8>()?;

                color
            }
            ColorFormat::Rgba4444 => {
                let data = reader.read_be::<u16>()?;
                Rgba::new(
                    data.bits(0, 4) as f32 / 16.0,
                    data.bits(4, 8) as f32 / 16.0,
                    data.bits(8, 12) as f32 / 16.0,
                    data.bits(12, 16) as f32 / 16.0,
                )
            }
            ColorFormat::Rgba6666 => {
                let data = u32::from_be_bytes([
                    0,
                    reader.read_be::<u8>()?,
                    reader.read_be::<u8>()?,
                    reader.read_be::<u8>()?,
                ]);

                Rgba::new(
                    data.bits(0, 6) as f32 / 64.0,
                    data.bits(6, 12) as f32 / 64.0,
                    data.bits(12, 18) as f32 / 64.0,
                    data.bits(18, 24) as f32 / 64.0,
                )
            }
            ColorFormat::Rgba8888 => Rgba::new(
                reader.read_be::<u8>()? as f32 / 255.0,
                reader.read_be::<u8>()? as f32 / 255.0,
                reader.read_be::<u8>()? as f32 / 255.0,
                reader.read_be::<u8>()? as f32 / 255.0,
            ),
            _ => panic!("reserved format"),
        };

        Some(match self.kind() {
            ColorKind::Rgb => rgba.rgb(),
            ColorKind::Rgba => rgba,
        })
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TexCoordsKind {
    /// One components (s).
    #[default]
    Vec1 = 0b0,
    /// Two components (s, t).
    Vec2 = 0b1,
}

#[bitos(9)]
#[derive(Debug, Clone, Default)]
pub struct TexCoordsDescriptor {
    #[bits(0)]
    pub kind: TexCoordsKind,
    #[bits(1..4)]
    pub format: CoordsFormat,
    #[bits(4..9)]
    pub shift: u5,
}

impl AttributeDescriptor for TexCoordsDescriptor {
    type Value = Vec2;

    fn size(&self) -> u32 {
        match self.kind() {
            TexCoordsKind::Vec1 => self.format().size(),
            TexCoordsKind::Vec2 => 2 * self.format().size(),
        }
    }

    fn read(&self, reader: &mut BinReader) -> Option<Vec2> {
        let mut component = || {
            let shift = 2.0f32.powi(self.shift().value() as i32);
            Some(match self.format() {
                CoordsFormat::U8 => reader.read_be::<u8>()? as f32 / shift,
                CoordsFormat::I8 => reader.read_be::<i8>()? as f32 / shift,
                CoordsFormat::U16 => reader.read_be::<u16>()? as f32 / shift,
                CoordsFormat::I16 => reader.read_be::<i16>()? as f32 / shift,
                CoordsFormat::F32 => f32::from_bits(reader.read_be::<u32>()?),
                _ => panic!("reserved format"),
            })
        };

        let s = component()?;
        Some(match self.kind() {
            TexCoordsKind::Vec1 => Vec2::new(s, 0.0),
            TexCoordsKind::Vec2 => {
                let t = component()?;
                Vec2::new(s, t)
            }
        })
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VertexAttributeTableA {
    #[bits(0..9)]
    pub position: PositionDescriptor,
    #[bits(9..13)]
    pub normal: NormalDescriptor,
    #[bits(13..17)]
    pub chan0: ColorDescriptor,
    #[bits(17..21)]
    pub chan1: ColorDescriptor,
    #[bits(21..30)]
    pub tex0: TexCoordsDescriptor,
    #[bits(30)]
    pub byte_dequant: bool,
    #[bits(31)]
    pub normal_index: bool,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VertexAttributeTableB {
    #[bits(0..27)]
    pub tex1to3: [TexCoordsDescriptor; 3],

    #[bits(27)]
    pub tex4_kind: TexCoordsKind,
    #[bits(28..31)]
    pub tex4_format: CoordsFormat,

    #[bits(31)]
    pub vcache_enhance: bool,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VertexAttributeTableC {
    #[bits(0..5)]
    pub tex4_shift: u5,
    #[bits(5..32)]
    pub tex5to7: [TexCoordsDescriptor; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VertexAttributeTable {
    pub a: VertexAttributeTableA,
    pub b: VertexAttributeTableB,
    pub c: VertexAttributeTableC,
}

impl VertexAttributeTable {
    pub fn tex(&self, index: usize) -> Option<TexCoordsDescriptor> {
        Some(match index {
            0 => self.a.tex0(),
            1..4 => self.b.tex1to3_at(index - 1).unwrap(),
            4 => TexCoordsDescriptor::default()
                .with_kind(self.b.tex4_kind())
                .with_format(self.b.tex4_format())
                .with_shift(self.c.tex4_shift()),
            5..8 => self.c.tex5to7_at(index - 5).unwrap(),
            _ => return None,
        })
    }
}

/// The mode of an attribute. The mode defines whether the attribute is present directly in the
/// stream or indirectly through an index into an array.
#[bitos[2]]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttributeMode {
    /// Not present
    #[default]
    None    = 0b00,
    /// Directly in the vertex attribute stream
    Direct  = 0b01,
    /// Indirectly through a 8 bit index in the vertex attribute stream
    Index8  = 0b10,
    /// Indirectly through a 16 bit index in the vertex attribute stream
    Index16 = 0b11,
}

impl AttributeMode {
    pub fn is_present(self) -> bool {
        self != AttributeMode::None
    }

    /// Size of this attribute in a stream, if known.
    pub fn size(self) -> Option<u32> {
        match self {
            Self::None => Some(0),
            Self::Direct => None,
            Self::Index8 => Some(1),
            Self::Index16 => Some(2),
        }
    }
}

/// A vertex attribute.
pub trait Attribute {
    /// Name of the attribute.
    const NAME: &'static str;
    /// The descriptor for this attribute.
    type Descriptor: AttributeDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode;
    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor;
    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor>;
}

pub struct PosMatrixIndex;

impl Attribute for PosMatrixIndex {
    const NAME: &'static str = "PosMatrixIndex";
    type Descriptor = IndexDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        if vcd.pos_mtx_index() {
            AttributeMode::Direct
        } else {
            AttributeMode::None
        }
    }

    fn get_descriptor(_: &VertexAttributeTable) -> Self::Descriptor {
        IndexDescriptor
    }

    fn get_array(_: &Arrays) -> Option<ArrayDescriptor> {
        None
    }
}

pub struct TexMatrixIndex<const N: usize>;

impl<const N: usize> Attribute for TexMatrixIndex<N> {
    const NAME: &'static str = "TexMatrixIndex";
    type Descriptor = IndexDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        if vcd.tex_coord_mtx_index_at(N).unwrap() {
            AttributeMode::Direct
        } else {
            AttributeMode::None
        }
    }

    fn get_descriptor(_: &VertexAttributeTable) -> Self::Descriptor {
        IndexDescriptor
    }

    fn get_array(_: &Arrays) -> Option<ArrayDescriptor> {
        None
    }
}

pub struct Position;

impl Attribute for Position {
    const NAME: &'static str = "Position";
    type Descriptor = PositionDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        vcd.position()
    }

    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor {
        vat.a.position()
    }

    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor> {
        Some(arrays.position)
    }
}

pub struct Normal;

impl Attribute for Normal {
    const NAME: &'static str = "Normal";
    type Descriptor = NormalDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        vcd.normal()
    }

    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor {
        vat.a.normal()
    }

    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor> {
        Some(arrays.normal)
    }
}

pub struct Chan0;

impl Attribute for Chan0 {
    const NAME: &'static str = "Color Channel 0";
    type Descriptor = ColorDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        vcd.chan0()
    }

    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor {
        vat.a.chan0()
    }

    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor> {
        Some(arrays.chan0)
    }
}

pub struct Chan1;

impl Attribute for Chan1 {
    const NAME: &'static str = "Color Channel 1";
    type Descriptor = ColorDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        vcd.chan1()
    }

    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor {
        vat.a.chan1()
    }

    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor> {
        Some(arrays.chan1)
    }
}

pub struct TexCoords<const N: usize>;

impl<const N: usize> Attribute for TexCoords<N> {
    const NAME: &'static str = "TexCoord";
    type Descriptor = TexCoordsDescriptor;

    fn get_mode(vcd: &VertexDescriptor) -> AttributeMode {
        vcd.tex_coord_at(N).unwrap()
    }

    fn get_descriptor(vat: &VertexAttributeTable) -> Self::Descriptor {
        vat.tex(N).unwrap()
    }

    fn get_array(arrays: &Arrays) -> Option<ArrayDescriptor> {
        Some(arrays.tex_coords[N])
    }
}
