//! Command processor (CP).
pub mod attributes;

use attributes::VertexAttributeTable;
use bitos::integer::u3;
use bitos::{BitUtils, bitos};
use gekko::Address;
use strum::FromRepr;
use zerocopy::IntoBytes;

use crate::Primitive;
use crate::stream::{BinRingBuffer, BinaryStream};
use crate::system::System;
use crate::system::gx::cmd::attributes::{AttributeDescriptor, AttributeMode};
use crate::system::gx::{self, Gpu, Reg as GxReg, Topology};

/// A command processor register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub enum Reg {
    Unknown00       = 0x00,
    Unknown10       = 0x10,
    Unknown20       = 0x20,

    MatIndexLow     = 0x30,
    MatIndexHigh    = 0x40,

    // VCD
    VcdLow          = 0x50,
    VcdHigh         = 0x60,

    // VAT
    Vat0A           = 0x70,
    Vat1A           = 0x71,
    Vat2A           = 0x72,
    Vat3A           = 0x73,
    Vat4A           = 0x74,
    Vat5A           = 0x75,
    Vat6A           = 0x76,
    Vat7A           = 0x77,

    Vat0B           = 0x80,
    Vat1B           = 0x81,
    Vat2B           = 0x82,
    Vat3B           = 0x83,
    Vat4B           = 0x84,
    Vat5B           = 0x85,
    Vat6B           = 0x86,
    Vat7B           = 0x87,

    Vat0C           = 0x90,
    Vat1C           = 0x91,
    Vat2C           = 0x92,
    Vat3C           = 0x93,
    Vat4C           = 0x94,
    Vat5C           = 0x95,
    Vat6C           = 0x96,
    Vat7C           = 0x97,

    // Array Base
    PositionPtr     = 0xA0,
    NormalPtr       = 0xA1,
    Chan0Ptr        = 0xA2,
    Chan1Ptr        = 0xA3,
    Tex0CoordPtr    = 0xA4,
    Tex1CoordPtr    = 0xA5,
    Tex2CoordPtr    = 0xA6,
    Tex3CoordPtr    = 0xA7,
    Tex4CoordPtr    = 0xA8,
    Tex5CoordPtr    = 0xA9,
    Tex6CoordPtr    = 0xAA,
    Tex7CoordPtr    = 0xAB,
    GpArr0Ptr       = 0xAC,
    GpArr1Ptr       = 0xAD,
    GpArr2Ptr       = 0xAE,
    GpArr3Ptr       = 0xAF,

    // Array Stride
    PositionStride  = 0xB0,
    NormalStride    = 0xB1,
    Chan0Stride     = 0xB2,
    Chan1Stride     = 0xB3,
    Tex0CoordStride = 0xB4,
    Tex1CoordStride = 0xB5,
    Tex2CoordStride = 0xB6,
    Tex3CoordStride = 0xB7,
    Tex4CoordStride = 0xB8,
    Tex5CoordStride = 0xB9,
    Tex6CoordStride = 0xBA,
    Tex7CoordStride = 0xBB,
    GpArr0Stride    = 0xBC,
    GpArr1Stride    = 0xBD,
    GpArr2Stride    = 0xBE,
    GpArr3Stride    = 0xBF,
}

impl Reg {
    pub fn is_matrices_index(self) -> bool {
        matches!(self, Self::MatIndexLow | Self::MatIndexHigh)
    }
}

#[bitos(5)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Operation {
    #[default]
    NOP               = 0b0_0000,
    SetCP             = 0b0_0001,
    SetXF             = 0b0_0010,
    IndexedSetXFA     = 0b0_0100,
    IndexedSetXFB     = 0b0_0101,
    IndexedSetXFC     = 0b0_0110,
    IndexedSetXFD     = 0b0_0111,
    Call              = 0b0_1000,
    InvalidateVertexCache = 0b0_1001,
    SetBP             = 0b0_1100,
    DrawQuadList      = 0b1_0000,
    DrawTriangleList  = 0b1_0010,
    DrawTriangleStrip = 0b1_0011,
    DrawTriangleFan   = 0b1_0100,
    DrawLineList      = 0b1_0101,
    DrawLineStrip     = 0b1_0110,
    DrawPointList     = 0b1_0111,
}

#[bitos(8)]
#[derive(Debug)]
pub struct Opcode {
    #[bits(0..3)]
    pub vat_index: u3,
    #[bits(3..8)]
    pub operation: Option<Operation>,
}

#[derive(Debug)]
pub enum Command {
    Nop,
    InvalidateVertexCache,
    Call {
        address: Address,
        length: u32,
    },
    SetCP {
        register: Reg,
        value: u32,
    },
    SetBP {
        register: GxReg,
        value: u32,
    },
    SetXF {
        start: u16,
        values: Vec<u32>,
    },
    IndexedSetXFA {
        base: u16,
        length: u8,
        index: u16,
    },
    IndexedSetXFB {
        base: u16,
        length: u8,
        index: u16,
    },
    IndexedSetXFC {
        base: u16,
        length: u8,
        index: u16,
    },
    IndexedSetXFD {
        base: u16,
        length: u8,
        index: u16,
    },
    Draw {
        topology: Topology,
        vertex_attributes: VertexAttributeStream,
    },
}

/// CP status register
#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Status {
    #[bits(0)]
    pub fifo_overflow: bool,
    #[bits(1)]
    pub fifo_underflow: bool,
    #[bits(2)]
    pub read_idle: bool,
    #[bits(3)]
    pub write_idle: bool,
    #[bits(4)]
    pub breakpoint_interrupt: bool,
}

/// CP control register
#[bitos(16)]
#[derive(Debug, Clone, Copy)]
pub struct Control {
    #[bits(0)]
    pub fifo_read_enable: bool,
    #[bits(1)]
    pub fifo_breakpoint_enable: bool,
    #[bits(2)]
    pub fifo_overflow_interrupt_enable: bool,
    #[bits(3)]
    pub fifo_underflow_interrupt_enable: bool,
    #[bits(4)]
    pub linked_mode: bool,
    #[bits(5)]
    pub fifo_breakpoint_interrupt_enable: bool,
}

impl Default for Control {
    fn default() -> Self {
        Self::from_bits(0).with_linked_mode(true)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Fifo {
    pub start: Address,
    pub end: Address,
    pub high_mark: u32,
    pub low_mark: u32,
    pub write_ptr: Address,
    pub read_ptr: Address,
}

impl Fifo {
    /// The FIFO count.
    pub fn count(&self) -> u32 {
        let count = if self.write_ptr >= self.read_ptr {
            self.write_ptr - self.read_ptr
        } else {
            let start = self.write_ptr - self.start;
            let end = self.end - self.read_ptr;
            start + end
        };

        assert!(
            count >= 0,
            "start: {}, end: {}; write: {}, read: {}",
            self.start,
            self.end,
            self.write_ptr,
            self.read_ptr,
        );

        count as u32
    }
}

/// Describes which attributes are present in the vertices of primitives and how they are present.
#[bitos(64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VertexDescriptor {
    /// Whether the position/normal matrix index is present.
    #[bits(0)]
    pub pos_mat_index: bool,
    /// Whether the texture coordinate matrix N index is present.
    #[bits(1..9)]
    pub tex_coord_mat_index: [bool; 8],
    /// Whether the position attribute is present.
    #[bits(9..11)]
    pub position: AttributeMode,
    /// Whether the normal attribute is present.
    #[bits(11..13)]
    pub normal: AttributeMode,
    /// Whether the color channel 0 attribute is present.
    #[bits(13..15)]
    pub chan0: AttributeMode,
    /// Whether the color channel 1 attribute is present.
    #[bits(15..17)]
    pub chan1: AttributeMode,
    /// Whether the texture coordinate N attribute is present.
    #[bits(32..48)]
    pub tex_coord: [AttributeMode; 8],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ArrayDescriptor {
    pub address: Address,
    pub stride: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Arrays {
    pub position: ArrayDescriptor,
    pub normal: ArrayDescriptor,
    pub chan0: ArrayDescriptor,
    pub chan1: ArrayDescriptor,
    pub tex_coords: [ArrayDescriptor; 8],
    pub general_purpose: [ArrayDescriptor; 4],
}

#[derive(Debug, Clone, Default)]
pub struct Internal {
    pub vertex_descriptor: VertexDescriptor,
    pub vertex_attr_tables: [VertexAttributeTable; 8],
    pub arrays: Arrays,
}

impl Internal {
    pub fn vertex_size(&self, vat: u8) -> u32 {
        let vat = vat as usize;

        let mut size = 0;
        if self.vertex_descriptor.pos_mat_index() {
            size += 1;
        }

        for i in 0..8 {
            if self.vertex_descriptor.tex_coord_mat_index_at(i).unwrap() {
                size += 1;
            }
        }

        size += self
            .vertex_descriptor
            .position()
            .size()
            .unwrap_or_else(|| self.vertex_attr_tables[vat].a.position().size());

        size += self
            .vertex_descriptor
            .normal()
            .size()
            .unwrap_or_else(|| self.vertex_attr_tables[vat].a.normal().size());

        size += self
            .vertex_descriptor
            .chan0()
            .size()
            .unwrap_or_else(|| self.vertex_attr_tables[vat].a.chan0().size());

        size += self
            .vertex_descriptor
            .chan1()
            .size()
            .unwrap_or_else(|| self.vertex_attr_tables[vat].a.chan1().size());

        for i in 0..8 {
            size += self
                .vertex_descriptor
                .tex_coord_at(i)
                .unwrap()
                .size()
                .unwrap_or_else(|| self.vertex_attr_tables[vat].tex(i).unwrap().size());
        }

        size
    }
}

#[derive(Debug, Clone)]
pub struct VertexAttributeStream {
    table: u8,
    count: u16,
    data: Vec<u8>,
}

impl VertexAttributeStream {
    pub fn table_index(&self) -> usize {
        self.table as usize
    }

    pub fn count(&self) -> u16 {
        self.count
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn stride(&self) -> usize {
        self.data.len() / self.count as usize
    }
}

/// CP interface
#[derive(Debug, Default)]
pub struct Interface {
    pub status: Status,
    pub control: Control,
    pub fifo: Fifo,
    pub internal: Internal,
    pub queue: BinRingBuffer,
}

impl Interface {
    /// Write a value to the clear register.
    pub fn write_clear(&mut self, value: u16) {
        if value.bit(0) {
            self.status.set_fifo_overflow(false);
        }

        if value.bit(1) {
            self.status.set_fifo_underflow(false);
        }
    }
}

impl Gpu {
    /// Reads a command from the command queue.
    pub fn read_command(&mut self) -> Option<Command> {
        let mut reader = self.cmd.queue.reader();

        let opcode = Opcode::from_bits(reader.read_be()?);
        let Some(operation) = opcode.operation() else {
            panic!("unknown opcode 0x{:02X?}", opcode.0);
        };

        let command = match operation {
            Operation::NOP => Command::Nop,
            Operation::SetCP => {
                let register = reader.read_be::<u8>()?;
                let value = reader.read_be::<u32>()?;

                let Some(register) = Reg::from_repr(register) else {
                    panic!("unknown internal CP register {register:02X}");
                };

                Command::SetCP { register, value }
            }
            Operation::SetXF => {
                let length = reader.read_be::<u16>()? as u32 + 1;
                if reader.remaining() < 4 * length as usize {
                    return None;
                }

                let start = reader.read_be::<u16>()?;
                let mut values = Vec::with_capacity(length as usize);
                for _ in 0..length {
                    values.push(reader.read_be::<u32>()?);
                }

                Command::SetXF { start, values }
            }
            Operation::IndexedSetXFA => {
                let config = reader.read_be::<u32>()?;
                let base = config.bits(0, 12) as u16;
                let length = config.bits(12, 16) as u8 + 1;
                let index = config.bits(16, 32) as u16;

                Command::IndexedSetXFA {
                    base,
                    length,
                    index,
                }
            }
            Operation::IndexedSetXFB => {
                let config = reader.read_be::<u32>()?;
                let base = config.bits(0, 12) as u16;
                let length = config.bits(12, 16) as u8 + 1;
                let index = config.bits(16, 32) as u16;

                Command::IndexedSetXFB {
                    base,
                    length,
                    index,
                }
            }
            Operation::IndexedSetXFC => {
                let config = reader.read_be::<u32>()?;
                let base = config.bits(0, 12) as u16;
                let length = config.bits(12, 16) as u8 + 1;
                let index = config.bits(16, 32) as u16;

                Command::IndexedSetXFC {
                    base,
                    length,
                    index,
                }
            }
            Operation::IndexedSetXFD => {
                let config = reader.read_be::<u32>()?;
                let base = config.bits(0, 12) as u16;
                let length = config.bits(12, 16) as u8 + 1;
                let index = config.bits(16, 32) as u16;

                Command::IndexedSetXFD {
                    base,
                    length,
                    index,
                }
            }
            Operation::Call => {
                let address = Address(reader.read_be::<u32>()?);
                let length = reader.read_be::<u32>()?;

                Command::Call { address, length }
            }
            Operation::InvalidateVertexCache => Command::InvalidateVertexCache,
            Operation::SetBP => {
                let register = reader.read_be::<u8>()?;
                let value = u32::from_be_bytes([
                    0,
                    reader.read_be::<u8>()?,
                    reader.read_be::<u8>()?,
                    reader.read_be::<u8>()?,
                ]);

                let Some(register) = GxReg::from_repr(register) else {
                    panic!("unknown internal GX register {register:02X}");
                };

                Command::SetBP { register, value }
            }
            Operation::DrawQuadList
            | Operation::DrawTriangleList
            | Operation::DrawTriangleStrip
            | Operation::DrawTriangleFan
            | Operation::DrawLineList
            | Operation::DrawLineStrip
            | Operation::DrawPointList => {
                let vertex_count = reader.read_be::<u16>()?;
                let vertex_size = self.cmd.internal.vertex_size(opcode.vat_index().value());

                let attribute_stream_size = vertex_count as usize * vertex_size as usize;
                if reader.remaining() < attribute_stream_size {
                    return None;
                }

                let vertex_attributes = reader.read_bytes(attribute_stream_size)?;
                let vertex_attributes = VertexAttributeStream {
                    table: opcode.vat_index().value(),
                    count: vertex_count,
                    data: vertex_attributes,
                };

                let topology = match operation {
                    Operation::DrawQuadList => Topology::QuadList,
                    Operation::DrawTriangleList => Topology::TriangleList,
                    Operation::DrawTriangleStrip => Topology::TriangleStrip,
                    Operation::DrawTriangleFan => Topology::TriangleFan,
                    Operation::DrawLineList => Topology::LineList,
                    Operation::DrawLineStrip => Topology::LineStrip,
                    Operation::DrawPointList => Topology::PointList,
                    _ => unreachable!(),
                };

                Command::Draw {
                    topology,
                    vertex_attributes,
                }
            }
        };

        reader.finish();
        Some(command)
    }
}

/// Sets the value of an internal command processor register.
pub fn set_register(sys: &mut System, reg: Reg, value: u32) {
    let cp = &mut sys.gpu.cmd.internal;
    let xf = &mut sys.gpu.xform.internal;

    match reg {
        Reg::MatIndexLow => value.write_ne_bytes(&mut xf.default_matrices.as_mut_bytes()[0..4]),
        Reg::MatIndexHigh => value.write_ne_bytes(&mut xf.default_matrices.as_mut_bytes()[4..8]),

        Reg::VcdLow => value.write_ne_bytes(&mut cp.vertex_descriptor.as_mut_bytes()[0..4]),
        Reg::VcdHigh => value.write_ne_bytes(&mut cp.vertex_descriptor.as_mut_bytes()[4..8]),

        Reg::Vat0A => value.write_ne_bytes(cp.vertex_attr_tables[0].a.as_mut_bytes()),
        Reg::Vat1A => value.write_ne_bytes(cp.vertex_attr_tables[1].a.as_mut_bytes()),
        Reg::Vat2A => value.write_ne_bytes(cp.vertex_attr_tables[2].a.as_mut_bytes()),
        Reg::Vat3A => value.write_ne_bytes(cp.vertex_attr_tables[3].a.as_mut_bytes()),
        Reg::Vat4A => value.write_ne_bytes(cp.vertex_attr_tables[4].a.as_mut_bytes()),
        Reg::Vat5A => value.write_ne_bytes(cp.vertex_attr_tables[5].a.as_mut_bytes()),
        Reg::Vat6A => value.write_ne_bytes(cp.vertex_attr_tables[6].a.as_mut_bytes()),
        Reg::Vat7A => value.write_ne_bytes(cp.vertex_attr_tables[7].a.as_mut_bytes()),

        Reg::Vat0B => value.write_ne_bytes(cp.vertex_attr_tables[0].b.as_mut_bytes()),
        Reg::Vat1B => value.write_ne_bytes(cp.vertex_attr_tables[1].b.as_mut_bytes()),
        Reg::Vat2B => value.write_ne_bytes(cp.vertex_attr_tables[2].b.as_mut_bytes()),
        Reg::Vat3B => value.write_ne_bytes(cp.vertex_attr_tables[3].b.as_mut_bytes()),
        Reg::Vat4B => value.write_ne_bytes(cp.vertex_attr_tables[4].b.as_mut_bytes()),
        Reg::Vat5B => value.write_ne_bytes(cp.vertex_attr_tables[5].b.as_mut_bytes()),
        Reg::Vat6B => value.write_ne_bytes(cp.vertex_attr_tables[6].b.as_mut_bytes()),
        Reg::Vat7B => value.write_ne_bytes(cp.vertex_attr_tables[7].b.as_mut_bytes()),

        Reg::Vat0C => value.write_ne_bytes(cp.vertex_attr_tables[0].c.as_mut_bytes()),
        Reg::Vat1C => value.write_ne_bytes(cp.vertex_attr_tables[1].c.as_mut_bytes()),
        Reg::Vat2C => value.write_ne_bytes(cp.vertex_attr_tables[2].c.as_mut_bytes()),
        Reg::Vat3C => value.write_ne_bytes(cp.vertex_attr_tables[3].c.as_mut_bytes()),
        Reg::Vat4C => value.write_ne_bytes(cp.vertex_attr_tables[4].c.as_mut_bytes()),
        Reg::Vat5C => value.write_ne_bytes(cp.vertex_attr_tables[5].c.as_mut_bytes()),
        Reg::Vat6C => value.write_ne_bytes(cp.vertex_attr_tables[6].c.as_mut_bytes()),
        Reg::Vat7C => value.write_ne_bytes(cp.vertex_attr_tables[7].c.as_mut_bytes()),

        Reg::PositionPtr => value.write_ne_bytes(cp.arrays.position.address.as_mut_bytes()),
        Reg::NormalPtr => value.write_ne_bytes(cp.arrays.normal.address.as_mut_bytes()),
        Reg::Chan0Ptr => value.write_ne_bytes(cp.arrays.chan0.address.as_mut_bytes()),
        Reg::Chan1Ptr => value.write_ne_bytes(cp.arrays.chan1.address.as_mut_bytes()),

        Reg::Tex0CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[0].address.as_mut_bytes()),
        Reg::Tex1CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[1].address.as_mut_bytes()),
        Reg::Tex2CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[2].address.as_mut_bytes()),
        Reg::Tex3CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[3].address.as_mut_bytes()),
        Reg::Tex4CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[4].address.as_mut_bytes()),
        Reg::Tex5CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[5].address.as_mut_bytes()),
        Reg::Tex6CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[6].address.as_mut_bytes()),
        Reg::Tex7CoordPtr => value.write_ne_bytes(cp.arrays.tex_coords[7].address.as_mut_bytes()),

        Reg::GpArr0Ptr => value.write_ne_bytes(cp.arrays.general_purpose[0].address.as_mut_bytes()),
        Reg::GpArr1Ptr => value.write_ne_bytes(cp.arrays.general_purpose[1].address.as_mut_bytes()),
        Reg::GpArr2Ptr => value.write_ne_bytes(cp.arrays.general_purpose[2].address.as_mut_bytes()),
        Reg::GpArr3Ptr => value.write_ne_bytes(cp.arrays.general_purpose[3].address.as_mut_bytes()),

        Reg::PositionStride => value.write_ne_bytes(cp.arrays.position.stride.as_mut_bytes()),
        Reg::NormalStride => value.write_ne_bytes(cp.arrays.normal.stride.as_mut_bytes()),
        Reg::Chan0Stride => value.write_ne_bytes(cp.arrays.chan0.stride.as_mut_bytes()),
        Reg::Chan1Stride => value.write_ne_bytes(cp.arrays.chan1.stride.as_mut_bytes()),

        Reg::Tex0CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[0].stride.as_mut_bytes()),
        Reg::Tex1CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[1].stride.as_mut_bytes()),
        Reg::Tex2CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[2].stride.as_mut_bytes()),
        Reg::Tex3CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[3].stride.as_mut_bytes()),
        Reg::Tex4CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[4].stride.as_mut_bytes()),
        Reg::Tex5CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[5].stride.as_mut_bytes()),
        Reg::Tex6CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[6].stride.as_mut_bytes()),
        Reg::Tex7CoordStride => value.write_ne_bytes(cp.arrays.tex_coords[7].stride.as_mut_bytes()),

        Reg::GpArr0Stride => {
            value.write_ne_bytes(cp.arrays.general_purpose[0].stride.as_mut_bytes())
        }
        Reg::GpArr1Stride => {
            value.write_ne_bytes(cp.arrays.general_purpose[1].stride.as_mut_bytes())
        }
        Reg::GpArr2Stride => {
            value.write_ne_bytes(cp.arrays.general_purpose[2].stride.as_mut_bytes())
        }
        Reg::GpArr3Stride => {
            value.write_ne_bytes(cp.arrays.general_purpose[3].stride.as_mut_bytes())
        }

        _ => tracing::warn!("unimplemented write to internal CP register {reg:?}"),
    }
}

/// Pops a value from the CP FIFO in memory.
fn fifo_pop(sys: &mut System) -> u8 {
    assert!(sys.gpu.cmd.fifo.count() > 0);

    let data = sys.read_phys_slow::<u8>(sys.gpu.cmd.fifo.read_ptr);
    sys.gpu.cmd.fifo.read_ptr += 1;
    if sys.gpu.cmd.fifo.read_ptr > sys.gpu.cmd.fifo.end {
        std::hint::cold_path();
        sys.gpu.cmd.fifo.read_ptr = sys.gpu.cmd.fifo.start;
    }

    data
}

/// Consumes commands available in the CP FIFO.
pub fn consume(sys: &mut System) {
    if !sys.gpu.cmd.control.fifo_read_enable() {
        return;
    }

    while sys.gpu.cmd.fifo.count() > 0 {
        let data = self::fifo_pop(sys);
        sys.gpu.cmd.queue.push_be(data);
    }
}

/// Process consumed CP commands until the queue is either empty or incomplete.
pub fn process(sys: &mut System) {
    let current_token = sys.gpu.pix.token;
    loop {
        let draw_done = sys.gpu.pix.interrupt.finish();
        if draw_done {
            break;
        }

        if current_token != sys.gpu.pix.token {
            break;
        }

        if sys.gpu.cmd.queue.is_empty() {
            break;
        }

        let Some(cmd) = sys.gpu.read_command() else {
            break;
        };

        if !matches!(cmd, Command::Nop | Command::InvalidateVertexCache) {
            tracing::debug!("processing {:02X?}", cmd);
        }

        match cmd {
            Command::Nop => (),
            Command::InvalidateVertexCache => (),
            Command::Call { address, length } => gx::call(sys, address, length),
            Command::SetCP { register, value } => self::set_register(sys, register, value),
            Command::SetBP { register, value } => gx::set_register(sys, register, value),
            Command::SetXF { start, values } => {
                for (offset, value) in values.into_iter().enumerate() {
                    gx::xform::write(sys, start + offset as u16, value);
                }
            }
            Command::IndexedSetXFA {
                base,
                length,
                index,
            } => {
                let array = sys.gpu.cmd.internal.arrays.general_purpose[0];
                gx::xform::write_indexed(sys, array, base, length, index);
            }
            Command::IndexedSetXFB {
                base,
                length,
                index,
            } => {
                let array = sys.gpu.cmd.internal.arrays.general_purpose[1];
                gx::xform::write_indexed(sys, array, base, length, index);
            }
            Command::IndexedSetXFC {
                base,
                length,
                index,
            } => {
                let array = sys.gpu.cmd.internal.arrays.general_purpose[2];
                gx::xform::write_indexed(sys, array, base, length, index);
            }
            Command::IndexedSetXFD {
                base,
                length,
                index,
            } => {
                let array = sys.gpu.cmd.internal.arrays.general_purpose[3];
                gx::xform::write_indexed(sys, array, base, length, index);
            }
            Command::Draw {
                topology,
                vertex_attributes,
            } => {
                gx::draw(sys, topology, &vertex_attributes);
            }
        }
    }

    sys.scheduler.schedule(1 << 16, self::process);
}

/// Synchronizes the CP fifo to the PI fifo.
pub fn sync_to_pi(sys: &mut System) {
    sys.gpu.cmd.fifo.start = sys.processor.fifo_start;
    sys.gpu.cmd.fifo.end = sys.processor.fifo_end;
    sys.gpu.cmd.fifo.write_ptr = sys.processor.fifo_current.address();
}
