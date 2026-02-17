mod exec;

pub mod ins;

use bitos::integer::{u3, u4, u15};
use bitos::{BitUtils, bitos};
use lazuli::Primitive;
use lazuli::system::System;
use lazuli::system::dspi::{DspDmaControl, DspDmaDirection, DspDmaTarget, Mailbox};
use strum::FromRepr;
use tinyvec::ArrayVec;
use util::boxed_array;
use zerocopy::IntoBytes;

use crate::ins::{ExtensionOpcode, Opcode};

#[rustfmt::skip]
pub use crate::ins::Ins;

const IRAM_LEN: usize = 0x1000;
const IROM_LEN: usize = 0x1000;
const DRAM_LEN: usize = 0x1000;
const COEF_LEN: usize = 0x0800;

pub struct Memory {
    pub iram: Box<[u16; IRAM_LEN]>,
    pub irom: Box<[u16; IROM_LEN]>,
    pub dram: Box<[u16; DRAM_LEN]>,
    pub coef: Box<[u16; COEF_LEN]>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            iram: boxed_array(0),
            irom: boxed_array(0),
            dram: boxed_array(0),
            coef: boxed_array(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    Reset                = 0,
    StackOverflow        = 1,
    Unknown0             = 2,
    AccelRawReadOverflow = 3,
    AccelRawWriteOverflow = 4,
    AccelSampleReadOverflow = 5,
    Unknown1             = 6,
    External             = 7,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Acc40 {
    pub low: u16,
    pub mid: u16,
    pub high: u8,
}

impl Acc40 {
    const MIN: i64 = (1 << 63) >> 24;

    #[inline(always)]
    pub fn from(value: i64) -> Self {
        Self {
            low: value.bits(0, 16) as u16,
            mid: value.bits(16, 32) as u16,
            high: value.bits(32, 40) as u8,
        }
    }

    #[inline(always)]
    pub fn get(&self) -> i64 {
        let bits = 0
            .with_bits(0, 16, self.low as i64)
            .with_bits(16, 32, self.mid as i64)
            .with_bits(32, 40, self.high as i64);

        (bits << 24) >> 24
    }

    #[inline(always)]
    pub fn set(&mut self, value: i64) -> i64 {
        *self = Self::from(value);
        self.get()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Product {
    pub low: u16,
    pub mid1: u16,
    pub mid2: u16,
    pub high: u8,
}

impl Product {
    pub fn get(&self) -> (bool, bool, i64) {
        let (sum, carry) = self.mid1.overflowing_add(self.mid2);
        let (c_high, carry) = self.high.overflowing_add(carry as u8);
        let overflow = self.high as i8 >= 0 && ((c_high as i8) < 0);

        let bits = 0
            .with_bits(0, 16, self.low as i64)
            .with_bits(16, 32, sum as i64)
            .with_bits(32, 40, c_high as i64);

        let value = (bits << 24) >> 24;

        (carry, overflow, value)
    }

    pub fn set(&mut self, value: i64) {
        self.low = value as u16;
        self.mid1 = 0;
        self.mid2 = (value >> 16) as u16;
        self.high = (value >> 32) as u8;
    }
}

#[bitos(16)]
#[derive(Debug, Clone, Copy)]
pub struct Status {
    #[bits(0)]
    pub carry: bool,
    #[bits(1)]
    pub overflow: bool,
    #[bits(2)]
    pub arithmetic_zero: bool,
    #[bits(3)]
    pub sign: bool,
    #[bits(4)]
    pub above_s32: bool,
    #[bits(5)]
    pub top_two_bits_eq: bool,
    #[bits(6)]
    pub logic_zero: bool,
    #[bits(7)]
    pub overflow_fused: bool,
    #[bits(9)]
    pub interrupt_enable: bool,
    #[bits(11)]
    pub external_interrupt_enable: bool,
    #[bits(13)]
    pub dont_double_result: bool,
    #[bits(14)]
    pub sign_extend_to_40: bool,
    #[bits(15)]
    pub unsigned_mul: bool,
}

impl Default for Status {
    fn default() -> Self {
        Self::from_bits(0)
            .with_interrupt_enable(true)
            .with_external_interrupt_enable(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub enum Reg {
    Addr0,
    Addr1,
    Addr2,
    Addr3,
    Index0,
    Index1,
    Index2,
    Index3,
    Wrap0,
    Wrap1,
    Wrap2,
    Wrap3,
    CallStack,
    DataStack,
    LoopStack,
    LoopCount,
    Acc40High0,
    Acc40High1,
    Config,
    Status,
    ProdLow,
    ProdMid1,
    ProdHigh,
    ProdMid2,
    Acc32Low0,
    Acc32Low1,
    Acc32High0,
    Acc32High1,
    Acc40Low0,
    Acc40Low1,
    Acc40Mid0,
    Acc40Mid1,
}

impl Reg {
    pub fn new(index: u8) -> Self {
        Self::from_repr(index).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Registers {
    pub addressing: [u16; 4],
    pub indexing: [u16; 4],
    pub wrapping: [u16; 4],
    pub call_stack: ArrayVec<[u16; 8]>,
    pub data_stack: ArrayVec<[u16; 4]>,
    pub loop_stack: ArrayVec<[u16; 4]>,
    pub loop_count: ArrayVec<[u16; 4]>,
    pub product: Product,
    pub acc40: [Acc40; 2],
    pub acc32: [i32; 2],
    pub config: u8,
    pub status: Status,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            addressing: Default::default(),
            indexing: Default::default(),
            wrapping: [0xFFFF; 4],
            call_stack: Default::default(),
            data_stack: Default::default(),
            loop_stack: Default::default(),
            loop_count: Default::default(),
            product: Default::default(),
            acc40: Default::default(),
            acc32: Default::default(),
            config: Default::default(),
            status: Default::default(),
        }
    }
}

impl Registers {
    pub fn get(&self, reg: Reg) -> u16 {
        let acc_saturate = |i: usize| {
            let ml = self.acc40[i].get() as i32 as i64;
            let hml = self.acc40[i].get();

            if self.status.sign_extend_to_40() && ml != hml {
                if hml >= 0 { 0x7FFF } else { 0x8000 }
            } else {
                self.acc40[i].mid
            }
        };

        match reg {
            Reg::Addr0 => self.addressing[0],
            Reg::Addr1 => self.addressing[1],
            Reg::Addr2 => self.addressing[2],
            Reg::Addr3 => self.addressing[3],
            Reg::Index0 => self.indexing[0],
            Reg::Index1 => self.indexing[1],
            Reg::Index2 => self.indexing[2],
            Reg::Index3 => self.indexing[3],
            Reg::Wrap0 => self.wrapping[0],
            Reg::Wrap1 => self.wrapping[1],
            Reg::Wrap2 => self.wrapping[2],
            Reg::Wrap3 => self.wrapping[3],
            Reg::CallStack => self.call_stack.last().copied().unwrap_or_default(),
            Reg::DataStack => self.data_stack.last().copied().unwrap_or_default(),
            Reg::LoopStack => self.loop_stack.last().copied().unwrap_or_default(),
            Reg::LoopCount => self.loop_count.last().copied().unwrap_or_default(),
            Reg::Acc40High0 => self.acc40[0].high as i8 as i16 as u16,
            Reg::Acc40High1 => self.acc40[1].high as i8 as i16 as u16,
            Reg::Config => self.config as u16,
            Reg::Status => self.status.to_bits(),
            Reg::ProdLow => self.product.low,
            Reg::ProdMid1 => self.product.mid1,
            Reg::ProdHigh => self.product.high as u16,
            Reg::ProdMid2 => self.product.mid2,
            Reg::Acc32Low0 => self.acc32[0].bits(0, 16) as u16,
            Reg::Acc32Low1 => self.acc32[1].bits(0, 16) as u16,
            Reg::Acc32High0 => self.acc32[0].bits(16, 32) as u16,
            Reg::Acc32High1 => self.acc32[1].bits(16, 32) as u16,
            Reg::Acc40Low0 => self.acc40[0].low,
            Reg::Acc40Low1 => self.acc40[1].low,
            Reg::Acc40Mid0 => acc_saturate(0),
            Reg::Acc40Mid1 => acc_saturate(1),
        }
    }

    pub fn set(&mut self, reg: Reg, value: u16) {
        match reg {
            Reg::Addr0 => self.addressing[0] = value,
            Reg::Addr1 => self.addressing[1] = value,
            Reg::Addr2 => self.addressing[2] = value,
            Reg::Addr3 => self.addressing[3] = value,
            Reg::Index0 => self.indexing[0] = value,
            Reg::Index1 => self.indexing[1] = value,
            Reg::Index2 => self.indexing[2] = value,
            Reg::Index3 => self.indexing[3] = value,
            Reg::Wrap0 => self.wrapping[0] = value,
            Reg::Wrap1 => self.wrapping[1] = value,
            Reg::Wrap2 => self.wrapping[2] = value,
            Reg::Wrap3 => self.wrapping[3] = value,
            Reg::CallStack => self.call_stack.push(value),
            Reg::DataStack => self.data_stack.push(value),
            Reg::LoopStack => self.loop_stack.push(value),
            Reg::LoopCount => self.loop_count.push(value),
            Reg::Acc40High0 => self.acc40[0].high = value as u8,
            Reg::Acc40High1 => self.acc40[1].high = value as u8,
            Reg::Config => self.config = value as u8,
            Reg::Status => self.status = Status::from_bits(value.with_bit(8, false)),
            Reg::ProdLow => self.product.low = value,
            Reg::ProdMid1 => self.product.mid1 = value,
            Reg::ProdHigh => self.product.high = value as u8,
            Reg::ProdMid2 => self.product.mid2 = value,
            Reg::Acc32Low0 => self.acc32[0] = self.acc32[0].with_bits(0, 16, value as i32),
            Reg::Acc32Low1 => self.acc32[1] = self.acc32[1].with_bits(0, 16, value as i32),
            Reg::Acc32High0 => self.acc32[0] = self.acc32[0].with_bits(16, 32, value as i32),
            Reg::Acc32High1 => self.acc32[1] = self.acc32[1].with_bits(16, 32, value as i32),
            Reg::Acc40Low0 => self.acc40[0].low = value,
            Reg::Acc40Low1 => self.acc40[1].low = value,
            Reg::Acc40Mid0 => self.acc40[0].mid = value,
            Reg::Acc40Mid1 => self.acc40[1].mid = value,
        }
    }

    fn set_acc_saturate(&mut self, i: usize, value: u16) {
        if self.status.sign_extend_to_40() {
            self.acc40[i].low = 0;
            self.acc40[i].mid = value;
            self.acc40[i].high = if value.bit(15) { !0 } else { 0 };
        } else {
            self.acc40[i].mid = value;
        }
    }

    pub fn set_saturate(&mut self, reg: Reg, value: u16) {
        match reg {
            Reg::Acc40Mid0 => {
                std::hint::cold_path();
                self.set_acc_saturate(0, value)
            }
            Reg::Acc40Mid1 => {
                std::hint::cold_path();
                self.set_acc_saturate(1, value)
            }
            Reg::LoopStack => std::hint::cold_path(),
            _ => self.set(reg, value),
        }
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleSize {
    #[default]
    Nibble   = 0b00,
    Byte     = 0b01,
    Word     = 0b10,
    Reserved = 0b11,
}

impl SampleSize {
    pub fn size(self) -> u32 {
        match self {
            Self::Nibble => 1,
            Self::Byte => 2,
            Self::Word => 4,
            _ => panic!("reserved size"),
        }
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, Default)]
pub enum SampleDecoding {
    #[default]
    AramAdpcm  = 0b00,
    AcinPcm    = 0b01,
    AramPcm    = 0b10,
    AcinPcmInc = 0b11,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, Default)]
pub enum PcmDivisor {
    #[default]
    D2048    = 0b00,
    D1       = 0b01,
    D65536   = 0b10,
    Reserved = 0b11,
}

impl PcmDivisor {
    pub fn value(self) -> u32 {
        match self {
            Self::D2048 => 2048,
            Self::D1 => 1,
            Self::D65536 => 65536,
            _ => panic!("reserved divisor"),
        }
    }

    /// Applies rounding division.
    pub fn apply(self, value: i32) -> i32 {
        match self {
            Self::D2048 => (value + (1 << 10)) >> 11,
            Self::D1 => value,
            Self::D65536 => (value + (1 << 15)) >> 16,
            _ => panic!("reserved divisor"),
        }
    }
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct AccelFormat {
    #[bits(0..2)]
    pub sample: SampleSize,
    #[bits(2..4)]
    pub decoding: SampleDecoding,
    #[bits(4..6)]
    pub divisor: PcmDivisor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelWrap {
    RawRead,
    RawWrite,
    SampleRead,
}

impl AccelWrap {
    pub fn interrupt(self) -> Interrupt {
        match self {
            Self::RawRead => Interrupt::AccelRawReadOverflow,
            Self::RawWrite => Interrupt::AccelRawWriteOverflow,
            Self::SampleRead => Interrupt::AccelSampleReadOverflow,
        }
    }
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct AccelPredictor {
    #[bits(0..4)]
    pub scale_log2: u4,
    #[bits(4..7)]
    pub coefficients: u3,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AccelCoefficients {
    pub a: i16,
    pub b: i16,
}

#[derive(Default)]
pub struct Accelerator {
    pub coefficients: [AccelCoefficients; 8],
    pub format: AccelFormat,
    pub predictor: AccelPredictor,
    pub aram_start: u32,
    pub aram_end: u32,
    pub aram_curr: u32,
    pub gain: i16,
    pub input: i16,
    pub wrapped: Option<AccelWrap>,
    pub previous_samples: [i16; 2],
    pub has_data: bool,
}

#[derive(Clone, Copy)]
struct CachedIns {
    ins: Ins,
    len: u16,
    main: OpcodeFn,
    extension: Option<ExtensionFn>,
}

pub struct Interpreter {
    pub pc: u16,
    pub regs: Registers,
    pub mem: Memory,
    pub accel: Accelerator,
    pub loop_counter: Option<u16>,
    pub old_reset_high: bool,

    cached: Box<[Option<CachedIns>; 1 << 16]>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            pc: Default::default(),
            regs: Default::default(),
            mem: Default::default(),
            accel: Default::default(),
            loop_counter: Default::default(),
            old_reset_high: Default::default(),
            cached: util::boxed_array(None),
        }
    }
}

type OpcodeFn = for<'a, 'b> fn(&'a mut Interpreter, &'b mut System, Ins);

static OPCODE_EXEC_LUT: [OpcodeFn; 1 << 8] = {
    fn nop(_: &mut Interpreter, _: &mut System, _: Ins) {}
    let mut lut = [nop as OpcodeFn; 1 << 8];

    lut[Opcode::Abs as usize] = Interpreter::abs as OpcodeFn;
    lut[Opcode::Add as usize] = Interpreter::add as OpcodeFn;
    lut[Opcode::Addarn as usize] = Interpreter::addarn as OpcodeFn;
    lut[Opcode::Addax as usize] = Interpreter::addax as OpcodeFn;
    lut[Opcode::Addaxl as usize] = Interpreter::addaxl as OpcodeFn;
    lut[Opcode::Addi as usize] = Interpreter::addi as OpcodeFn;
    lut[Opcode::Addis as usize] = Interpreter::addis as OpcodeFn;
    lut[Opcode::Addp as usize] = Interpreter::addp as OpcodeFn;
    lut[Opcode::Addpaxz as usize] = Interpreter::addpaxz as OpcodeFn;
    lut[Opcode::Addr as usize] = Interpreter::addr as OpcodeFn;
    lut[Opcode::Andc as usize] = Interpreter::andc as OpcodeFn;
    lut[Opcode::Andcf as usize] = Interpreter::andcf as OpcodeFn;
    lut[Opcode::Andf as usize] = Interpreter::andf as OpcodeFn;
    lut[Opcode::Andi as usize] = Interpreter::andi as OpcodeFn;
    lut[Opcode::Andr as usize] = Interpreter::andr as OpcodeFn;
    lut[Opcode::Asl as usize] = Interpreter::asl as OpcodeFn;
    lut[Opcode::Asr as usize] = Interpreter::asr as OpcodeFn;
    lut[Opcode::Asr16 as usize] = Interpreter::asr16 as OpcodeFn;
    lut[Opcode::Asrn as usize] = Interpreter::asrn as OpcodeFn;
    lut[Opcode::Asrnr as usize] = Interpreter::asrnr as OpcodeFn;
    lut[Opcode::Asrnrx as usize] = Interpreter::asrnrx as OpcodeFn;
    lut[Opcode::Bloop as usize] = Interpreter::bloop as OpcodeFn;
    lut[Opcode::Bloopi as usize] = Interpreter::bloopi as OpcodeFn;
    lut[Opcode::Call as usize] = Interpreter::call as OpcodeFn;
    lut[Opcode::Callr as usize] = Interpreter::callr as OpcodeFn;
    lut[Opcode::Clr as usize] = Interpreter::clr as OpcodeFn;
    lut[Opcode::Clr15 as usize] = Interpreter::clr15 as OpcodeFn;
    lut[Opcode::Clrl as usize] = Interpreter::clrl as OpcodeFn;
    lut[Opcode::Clrp as usize] = Interpreter::clrp as OpcodeFn;
    lut[Opcode::Cmp as usize] = Interpreter::cmp as OpcodeFn;
    lut[Opcode::Cmpaxh as usize] = Interpreter::cmpaxh as OpcodeFn;
    lut[Opcode::Cmpi as usize] = Interpreter::cmpi as OpcodeFn;
    lut[Opcode::Cmpis as usize] = Interpreter::cmpis as OpcodeFn;
    lut[Opcode::Dar as usize] = Interpreter::dar as OpcodeFn;
    lut[Opcode::Dec as usize] = Interpreter::dec as OpcodeFn;
    lut[Opcode::Decm as usize] = Interpreter::decm as OpcodeFn;
    lut[Opcode::Halt as usize] = Interpreter::halt as OpcodeFn;
    lut[Opcode::Iar as usize] = Interpreter::iar as OpcodeFn;
    lut[Opcode::If as usize] = Interpreter::ifcc as OpcodeFn;
    lut[Opcode::Ilrr as usize] = Interpreter::ilrr as OpcodeFn;
    lut[Opcode::Ilrrd as usize] = Interpreter::ilrrd as OpcodeFn;
    lut[Opcode::Ilrri as usize] = Interpreter::ilrri as OpcodeFn;
    lut[Opcode::Ilrrn as usize] = Interpreter::ilrrn as OpcodeFn;
    lut[Opcode::Inc as usize] = Interpreter::inc as OpcodeFn;
    lut[Opcode::Incm as usize] = Interpreter::incm as OpcodeFn;
    lut[Opcode::Jmp as usize] = Interpreter::jmp as OpcodeFn;
    lut[Opcode::Jr as usize] = Interpreter::jmpr as OpcodeFn;
    lut[Opcode::Loop as usize] = Interpreter::loop_ as OpcodeFn;
    lut[Opcode::Loopi as usize] = Interpreter::loopi as OpcodeFn;
    lut[Opcode::Lr as usize] = Interpreter::lr as OpcodeFn;
    lut[Opcode::Lri as usize] = Interpreter::lri as OpcodeFn;
    lut[Opcode::Lris as usize] = Interpreter::lris as OpcodeFn;
    lut[Opcode::Lrr as usize] = Interpreter::lrr as OpcodeFn;
    lut[Opcode::Lrrd as usize] = Interpreter::lrrd as OpcodeFn;
    lut[Opcode::Lrri as usize] = Interpreter::lrri as OpcodeFn;
    lut[Opcode::Lrrn as usize] = Interpreter::lrrn as OpcodeFn;
    lut[Opcode::Lrs as usize] = Interpreter::lrs as OpcodeFn;
    lut[Opcode::Lsl as usize] = Interpreter::lsl as OpcodeFn;
    lut[Opcode::Lsl16 as usize] = Interpreter::lsl16 as OpcodeFn;
    lut[Opcode::Lsr as usize] = Interpreter::lsr as OpcodeFn;
    lut[Opcode::Lsr16 as usize] = Interpreter::lsr16 as OpcodeFn;
    lut[Opcode::Lsrn as usize] = Interpreter::lsrn as OpcodeFn;
    lut[Opcode::Lsrnr as usize] = Interpreter::lsrnr as OpcodeFn;
    lut[Opcode::Lsrnrx as usize] = Interpreter::lsrnrx as OpcodeFn;
    lut[Opcode::M0 as usize] = Interpreter::m0 as OpcodeFn;
    lut[Opcode::M2 as usize] = Interpreter::m2 as OpcodeFn;
    lut[Opcode::Madd as usize] = Interpreter::madd as OpcodeFn;
    lut[Opcode::Maddc as usize] = Interpreter::maddc as OpcodeFn;
    lut[Opcode::Maddx as usize] = Interpreter::maddx as OpcodeFn;
    lut[Opcode::Mov as usize] = Interpreter::mov as OpcodeFn;
    lut[Opcode::Movax as usize] = Interpreter::movax as OpcodeFn;
    lut[Opcode::Movnp as usize] = Interpreter::movnp as OpcodeFn;
    lut[Opcode::Movp as usize] = Interpreter::movp as OpcodeFn;
    lut[Opcode::Movpz as usize] = Interpreter::movpz as OpcodeFn;
    lut[Opcode::Movr as usize] = Interpreter::movr as OpcodeFn;
    lut[Opcode::Mrr as usize] = Interpreter::mrr as OpcodeFn;
    lut[Opcode::Msub as usize] = Interpreter::msub as OpcodeFn;
    lut[Opcode::Msubc as usize] = Interpreter::msubc as OpcodeFn;
    lut[Opcode::Msubx as usize] = Interpreter::msubx as OpcodeFn;
    lut[Opcode::Mul as usize] = Interpreter::mul as OpcodeFn;
    lut[Opcode::Mulac as usize] = Interpreter::mulac as OpcodeFn;
    lut[Opcode::Mulaxh as usize] = Interpreter::mulaxh as OpcodeFn;
    lut[Opcode::Mulc as usize] = Interpreter::mulc as OpcodeFn;
    lut[Opcode::Mulcac as usize] = Interpreter::mulcac as OpcodeFn;
    lut[Opcode::Mulcmv as usize] = Interpreter::mulcmv as OpcodeFn;
    lut[Opcode::Mulcmvz as usize] = Interpreter::mulcmvz as OpcodeFn;
    lut[Opcode::Mulmv as usize] = Interpreter::mulmv as OpcodeFn;
    lut[Opcode::Mulmvz as usize] = Interpreter::mulmvz as OpcodeFn;
    lut[Opcode::Mulx as usize] = Interpreter::mulx as OpcodeFn;
    lut[Opcode::Mulxac as usize] = Interpreter::mulxac as OpcodeFn;
    lut[Opcode::Mulxmv as usize] = Interpreter::mulxmv as OpcodeFn;
    lut[Opcode::Mulxmvz as usize] = Interpreter::mulxmvz as OpcodeFn;
    lut[Opcode::Neg as usize] = Interpreter::neg as OpcodeFn;
    lut[Opcode::Not as usize] = Interpreter::not as OpcodeFn;
    lut[Opcode::Orc as usize] = Interpreter::orc as OpcodeFn;
    lut[Opcode::Ori as usize] = Interpreter::ori as OpcodeFn;
    lut[Opcode::Orr as usize] = Interpreter::orr as OpcodeFn;
    lut[Opcode::Ret as usize] = Interpreter::ret as OpcodeFn;
    lut[Opcode::Rti as usize] = Interpreter::rti as OpcodeFn;
    lut[Opcode::Sbclr as usize] = Interpreter::sbclr as OpcodeFn;
    lut[Opcode::Sbset as usize] = Interpreter::sbset as OpcodeFn;
    lut[Opcode::Set15 as usize] = Interpreter::set15 as OpcodeFn;
    lut[Opcode::Set16 as usize] = Interpreter::set16 as OpcodeFn;
    lut[Opcode::Set40 as usize] = Interpreter::set40 as OpcodeFn;
    lut[Opcode::Si as usize] = Interpreter::si as OpcodeFn;
    lut[Opcode::Sr as usize] = Interpreter::sr as OpcodeFn;
    lut[Opcode::Srr as usize] = Interpreter::srr as OpcodeFn;
    lut[Opcode::Srrd as usize] = Interpreter::srrd as OpcodeFn;
    lut[Opcode::Srri as usize] = Interpreter::srri as OpcodeFn;
    lut[Opcode::Srrn as usize] = Interpreter::srrn as OpcodeFn;
    lut[Opcode::Srs as usize] = Interpreter::srs as OpcodeFn;
    lut[Opcode::Srsh as usize] = Interpreter::srsh as OpcodeFn;
    lut[Opcode::Sub as usize] = Interpreter::sub as OpcodeFn;
    lut[Opcode::Subarn as usize] = Interpreter::subarn as OpcodeFn;
    lut[Opcode::Subax as usize] = Interpreter::subax as OpcodeFn;
    lut[Opcode::Subp as usize] = Interpreter::subp as OpcodeFn;
    lut[Opcode::Subr as usize] = Interpreter::subr as OpcodeFn;
    lut[Opcode::Tst as usize] = Interpreter::tst as OpcodeFn;
    lut[Opcode::Tstaxh as usize] = Interpreter::tstaxh as OpcodeFn;
    lut[Opcode::Tstprod as usize] = Interpreter::tstprod as OpcodeFn;
    lut[Opcode::Xorc as usize] = Interpreter::xorc as OpcodeFn;
    lut[Opcode::Xori as usize] = Interpreter::xori as OpcodeFn;
    lut[Opcode::Xorr as usize] = Interpreter::xorr as OpcodeFn;

    lut
};

type ExtensionFn = for<'a, 'b, 'c> fn(&'a mut Interpreter, &'b mut System, Ins, &'c Registers);

static EXTENSION_EXEC_LUT: [ExtensionFn; 1 << 8] = {
    fn nop(_: &mut Interpreter, _: &mut System, _: Ins, _: &Registers) {}
    let mut lut = [nop as ExtensionFn; 1 << 8];

    lut[ExtensionOpcode::Dr as usize] = Interpreter::ext_dr as ExtensionFn;
    lut[ExtensionOpcode::Ir as usize] = Interpreter::ext_ir as ExtensionFn;
    lut[ExtensionOpcode::L as usize] = Interpreter::ext_l as ExtensionFn;
    lut[ExtensionOpcode::Ld as usize] = Interpreter::ext_ld as ExtensionFn;
    lut[ExtensionOpcode::Ldm as usize] = Interpreter::ext_ldm as ExtensionFn;
    lut[ExtensionOpcode::Ldn as usize] = Interpreter::ext_ldn as ExtensionFn;
    lut[ExtensionOpcode::Ldnm as usize] = Interpreter::ext_ldnm as ExtensionFn;
    lut[ExtensionOpcode::Ln as usize] = Interpreter::ext_ln as ExtensionFn;
    lut[ExtensionOpcode::Ls as usize] = Interpreter::ext_ls as ExtensionFn;
    lut[ExtensionOpcode::Lsm as usize] = Interpreter::ext_lsm as ExtensionFn;
    lut[ExtensionOpcode::Lsn as usize] = Interpreter::ext_lsn as ExtensionFn;
    lut[ExtensionOpcode::Lsnm as usize] = Interpreter::ext_lsnm as ExtensionFn;
    lut[ExtensionOpcode::Mv as usize] = Interpreter::ext_mv as ExtensionFn;
    lut[ExtensionOpcode::Nr as usize] = Interpreter::ext_nr as ExtensionFn;
    lut[ExtensionOpcode::S as usize] = Interpreter::ext_s as ExtensionFn;
    lut[ExtensionOpcode::Sl as usize] = Interpreter::ext_sl as ExtensionFn;
    lut[ExtensionOpcode::Slm as usize] = Interpreter::ext_slm as ExtensionFn;
    lut[ExtensionOpcode::Sln as usize] = Interpreter::ext_sln as ExtensionFn;
    lut[ExtensionOpcode::Slnm as usize] = Interpreter::ext_slnm as ExtensionFn;
    lut[ExtensionOpcode::Sn as usize] = Interpreter::ext_sn as ExtensionFn;

    lut
};

impl Interpreter {
    fn raise_interrupt(&mut self, interrupt: Interrupt) {
        self.regs.call_stack.push(self.pc);
        self.regs.data_stack.push(self.regs.status.to_bits());
        self.pc = interrupt as u16 * 2;

        match interrupt {
            Interrupt::External => self.regs.status.set_external_interrupt_enable(false),
            _ => self.regs.status.set_interrupt_enable(false),
        };
    }

    #[inline(always)]
    pub fn check_interrupts(&mut self, sys: &mut System) {
        if self.loop_counter.is_some() {
            return;
        }

        if self.regs.status.interrupt_enable()
            && let Some(wrap) = self.accel.wrapped.take()
        {
            std::hint::cold_path();
            self.raise_interrupt(wrap.interrupt());
            return;
        }

        // external interrupt does not care about status interrupt enable
        if self.regs.status.external_interrupt_enable() && sys.dsp.control.interrupt() {
            std::hint::cold_path();
            tracing::warn!("DSP external interrupt raised");
            sys.dsp.control.set_interrupt(false);
            self.raise_interrupt(Interrupt::External);
        }
    }

    #[inline(always)]
    fn check_stacks(&mut self) {
        if self.regs.loop_stack.last().is_some_and(|v| *v == self.pc) {
            std::hint::cold_path();

            let counter = self.regs.loop_count.last_mut().unwrap();
            *counter = counter.saturating_sub(1);

            if *counter == 0 {
                std::hint::cold_path();
                self.regs.call_stack.pop();
                self.regs.loop_stack.pop();
                self.regs.loop_count.pop();
            } else {
                let addr = *self.regs.call_stack.last().unwrap();
                self.pc = addr;
            }
        }
    }

    /// Soft resets the DSP.
    pub fn reset(&mut self, sys: &mut System) {
        self.loop_counter = None;

        self.regs = Default::default();
        sys.dsp.dsp_mailbox = Mailbox::from_bits(0);
        sys.dsp.cpu_mailbox = Mailbox::from_bits(0);

        self.cached.fill(None);
        self.pc = if sys.dsp.control.reset_high() {
            tracing::debug!("resetting at IROM (0x8000)");
            0x8000
        } else {
            tracing::debug!("resetting at IRAM (0x0000)");
            0x0000
        };
    }

    /// Checks for reset.
    pub fn check_reset(&mut self, sys: &mut System) {
        if sys.dsp.control.reset() || (sys.dsp.control.reset_high() != self.old_reset_high) {
            std::hint::cold_path();

            // DMA from main memory if resetting at low
            if !sys.dsp.control.reset_high() {
                tracing::debug!("DSP DMA stub from main memory");
                let data = sys.mem.ram()[0x0100_0000..][..1024]
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]));

                for (word, data) in self.mem.iram[..512].iter_mut().zip(data) {
                    *word = data;
                }
            }

            tracing::debug!("DSP reset");
            self.reset(sys);
        }

        sys.dsp.control.set_reset(false);
        self.old_reset_high = sys.dsp.control.reset_high();
    }

    /// Performs the DSP DMA if the transfer is ongoing.
    pub fn do_dma(&mut self, sys: &mut System) {
        if sys.dsp.dsp_dma.control.transfer_ongoing() {
            std::hint::cold_path();

            let ram_base = sys.dsp.dsp_dma.ram_base.with_bits(26, 32, 0);
            let dsp_base = sys.dsp.dsp_dma.dsp_base;
            let length = sys.dsp.dsp_dma.length;

            let (target, direction) = (
                sys.dsp.dsp_dma.control.dsp_target(),
                sys.dsp.dsp_dma.control.direction(),
            );

            match (target, direction) {
                (DspDmaTarget::Dmem, DspDmaDirection::FromRamToDsp) => {
                    tracing::debug!(
                        "DSP DMA {length:04X} bytes from RAM {ram_base:08X} to DMEM {dsp_base:04X}",
                    );

                    for word in 0..(length / 2) {
                        let data = u16::read_be_bytes(
                            &sys.mem.ram()[(ram_base + 2 * word as u32) as usize..],
                        );

                        self.write_dmem(sys, dsp_base + word, data);
                    }
                }
                (DspDmaTarget::Dmem, DspDmaDirection::FromDspToRam) => {
                    tracing::debug!(
                        "DSP DMA {length:04X} bytes from DMEM {dsp_base:04X} to RAM {ram_base:08X}"
                    );

                    for word in 0..(length / 2) {
                        let data = self.read_dmem(sys, dsp_base + word);
                        data.write_be_bytes(
                            &mut sys.mem.ram_mut()[(ram_base + 2 * word as u32) as usize..],
                        );
                    }
                }
                (DspDmaTarget::Imem, DspDmaDirection::FromRamToDsp) => {
                    std::hint::cold_path();

                    tracing::info!(
                        "DSP DMA {length:04X} bytes from RAM {ram_base:08X} to IMEM {dsp_base:04X} (ucode)"
                    );

                    for word in 0..(length / 2) {
                        let data = u16::read_be_bytes(
                            &sys.mem.ram()[(ram_base + 2 * word as u32) as usize..],
                        );

                        self.write_imem(dsp_base + word, data);
                    }

                    // clear cache
                    self.cached.fill(None);
                }
                (DspDmaTarget::Imem, DspDmaDirection::FromDspToRam) => unimplemented!(),
            };

            sys.dsp.dsp_dma.length = 0;
            sys.dsp.dsp_dma.control.set_transfer_ongoing(false);
        }
    }

    fn increment_aram_curr(&mut self, _wrap: Option<AccelWrap>) {
        self.accel.aram_curr += 1;
        if self.accel.aram_curr > self.accel.aram_end {
            self.accel.aram_curr = self.accel.aram_start;
            // HACK: wrap exceptions break Disney Cars (stacks overflow)
            // self.accel.wrapped = wrap;
            self.accel.has_data = false;
        }
    }

    fn read_aram_raw(&mut self, sys: &mut System, wrap: Option<AccelWrap>) -> u16 {
        let format = self.accel.format;
        let index = self.accel.aram_curr.with_bit(31, false);
        let value = match format.sample() {
            SampleSize::Nibble => {
                let address = index / 2;
                let byte = u8::read_be_bytes(&sys.dsp.aram[address as usize..]) as u16;
                if index.is_multiple_of(2) {
                    byte >> 4
                } else {
                    byte & 0xF
                }
            }
            SampleSize::Byte => u8::read_be_bytes(&sys.dsp.aram[index as usize..]) as u16,
            SampleSize::Word => {
                let address = index * 2;
                u16::read_be_bytes(&sys.dsp.aram[address as usize..])
            }
            _ => panic!("reserved format"),
        };

        tracing::debug!(
            "accelerator reading 0x{value:04X} from ARAM 0x{:08X} (wraps at 0x{:08X})",
            self.accel.aram_curr,
            self.accel.aram_end
        );

        self.increment_aram_curr(wrap);
        value
    }

    fn read_accelerator_raw(&mut self, sys: &mut System) -> u16 {
        self.read_aram_raw(sys, Some(AccelWrap::RawRead))
    }

    fn pcm_gain(&self, value: i32) -> i32 {
        value * self.accel.gain as i32
    }

    fn pcm_decode(&self, value: i32) -> i16 {
        let predictor = self.accel.predictor;
        let coeff_idx = predictor.coefficients().value();
        let coeffs = self.accel.coefficients[coeff_idx as usize];

        let acc = self.pcm_gain(value)
            + self.pcm_gain(coeffs.a as i32 * self.accel.previous_samples[0] as i32)
            + self.pcm_gain(coeffs.b as i32 * self.accel.previous_samples[1] as i32);

        self.accel.format.divisor().apply(acc) as i16
    }

    fn adpcm_decode(&mut self, sys: &mut System) -> i16 {
        assert_eq!(self.accel.format.sample(), SampleSize::Nibble);

        if self.accel.aram_curr.is_multiple_of(16) {
            let coeff_idx = self.read_aram_raw(sys, None) as u8;
            let scale = self.read_aram_raw(sys, None) as u8;
            self.accel.predictor.set_coefficients(u3::new(coeff_idx));
            self.accel.predictor.set_scale_log2(u4::new(scale));
        }

        let predictor = self.accel.predictor;
        let coeff_idx = predictor.coefficients().value();

        let coeffs = self.accel.coefficients[coeff_idx as usize];
        let scale = 1 << predictor.scale_log2().value();

        let data = ((self.read_aram_raw(sys, None) as i8) << 4) >> 4;
        let value = scale * data as i32;

        let prediction = coeffs.a as i32 * self.accel.previous_samples[0] as i32
            + coeffs.b as i32 * self.accel.previous_samples[1] as i32;

        let result = PcmDivisor::D2048.apply(prediction) + value;
        result.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    fn read_accelerator_sample(&mut self, sys: &mut System) -> i16 {
        if !self.accel.has_data {
            return 0;
        }

        let value = match self.accel.format.decoding() {
            SampleDecoding::AramAdpcm => self.adpcm_decode(sys),
            SampleDecoding::AcinPcm => self.pcm_decode(self.accel.input as i32),
            SampleDecoding::AramPcm => {
                let value = self.read_aram_raw(sys, Some(AccelWrap::SampleRead)) as i16;
                self.pcm_decode(value as i32)
            }
            SampleDecoding::AcinPcmInc => {
                self.increment_aram_curr(Some(AccelWrap::SampleRead));
                self.pcm_decode(self.accel.input as i32)
            }
        };

        self.accel.previous_samples[1] = self.accel.previous_samples[0];
        self.accel.previous_samples[0] = value;

        value
    }

    pub fn read_mmio(&mut self, sys: &mut System, offset: u8) -> u16 {
        match offset {
            // Coefficients
            0xA0..=0xAF => {
                let index = (offset as usize - 0xA0) / 2;
                if offset.is_multiple_of(2) {
                    self.accel.coefficients[index].a as u16
                } else {
                    self.accel.coefficients[index].b as u16
                }
            }

            // DMA
            0xC9 => sys.dsp.dsp_dma.control.to_bits(),
            0xCB => sys.dsp.dsp_dma.length,
            0xCD => sys.dsp.dsp_dma.dsp_base,
            0xCE => (sys.dsp.dsp_dma.ram_base >> 16) as u16,
            0xCF => sys.dsp.dsp_dma.ram_base as u16,

            // Accelerator
            0xD3 => self.read_accelerator_raw(sys),
            0xD4 => self.accel.aram_start.bits(16, 32) as u16,
            0xD5 => self.accel.aram_start.bits(0, 16) as u16,
            0xD6 => self.accel.aram_end.bits(16, 32) as u16,
            0xD7 => self.accel.aram_end.bits(0, 16) as u16,
            0xD8 => self.accel.aram_curr.bits(16, 32) as u16,
            0xD9 => self.accel.aram_curr.bits(0, 16) as u16,
            0xDA => self.accel.predictor.to_bits(),
            0xDB => self.accel.previous_samples[0] as u16,
            0xDC => self.accel.previous_samples[1] as u16,
            0xDD => self.read_accelerator_sample(sys) as u16,
            0xDE => self.accel.gain as u16,
            0xDF => self.accel.input as u16,

            // Mailboxes
            0xFC => sys.dsp.dsp_mailbox.high_and_status(),
            0xFD => sys.dsp.dsp_mailbox.low(),
            0xFE => sys.dsp.cpu_mailbox.high_and_status(),
            0xFF => {
                if sys.dsp.cpu_mailbox.status() {
                    tracing::trace!(
                        "received from CPU mailbox: 0x{:08X}",
                        sys.dsp.cpu_mailbox.data().value()
                    );
                    sys.dsp.cpu_mailbox.set_status(false);
                }

                sys.dsp.cpu_mailbox.low()
            }
            _ => unimplemented!("read from {offset:02X}"),
        }
    }

    pub fn write_mmio(&mut self, sys: &mut System, offset: u8, value: u16) {
        match offset {
            // Coefficients
            0xA0..=0xAF => {
                let index = (offset as usize - 0xA0) / 2;
                if offset.is_multiple_of(2) {
                    self.accel.coefficients[index].a = value as i16
                } else {
                    self.accel.coefficients[index].b = value as i16
                }
            }

            // DMA
            0xC9 => sys.dsp.dsp_dma.control = DspDmaControl::from_bits(value),
            0xCB => {
                sys.dsp.dsp_dma.length = value;
                sys.dsp.dsp_dma.control.set_transfer_ongoing(true);
            }
            0xCD => sys.dsp.dsp_dma.dsp_base = value,
            0xCE => {
                sys.dsp.dsp_dma.ram_base = sys.dsp.dsp_dma.ram_base.with_bits(16, 32, value as u32)
            }
            0xCF => {
                sys.dsp.dsp_dma.ram_base = sys.dsp.dsp_dma.ram_base.with_bits(0, 16, value as u32)
            }

            // Interrupt
            0xFB => {
                if value > 0 {
                    sys.dsp.control.set_dsp_interrupt(true);
                }
            }

            // Accelerator
            0xD1 => self.accel.format = AccelFormat::from_bits(value),
            0xD3 => {
                tracing::debug!(
                    "accelerator writing 0x{value:04X} to ARAM 0x{:08X} (wraps at 0x{:08X})",
                    self.accel.aram_curr,
                    self.accel.aram_end
                );

                value.write_be_bytes(
                    sys.dsp.aram[self.accel.aram_curr.with_bit(31, false) as usize..]
                        .as_mut_bytes(),
                );

                self.accel.aram_curr += 1;
                if self.accel.aram_curr > self.accel.aram_end {
                    todo!("should wrap");
                }
            }
            0xD4 => self.accel.aram_start = self.accel.aram_start.with_bits(16, 32, value as u32),
            0xD5 => self.accel.aram_start = self.accel.aram_start.with_bits(0, 16, value as u32),
            0xD6 => self.accel.aram_end = self.accel.aram_end.with_bits(16, 32, value as u32),
            0xD7 => self.accel.aram_end = self.accel.aram_end.with_bits(0, 16, value as u32),
            0xD8 => self.accel.aram_curr = self.accel.aram_curr.with_bits(16, 32, value as u32),
            0xD9 => self.accel.aram_curr = self.accel.aram_curr.with_bits(0, 16, value as u32),
            0xDA => self.accel.predictor = AccelPredictor::from_bits(value),
            0xDB => {
                self.accel.previous_samples[0] = value as i16;
            }
            0xDC => {
                self.accel.previous_samples[1] = value as i16;
                self.accel.has_data = true;
            }
            0xDE => self.accel.gain = value as i16,
            0xDF => self.accel.input = value as i16,

            // Mailboxes
            0xFC => {
                sys.dsp.dsp_mailbox.set_high(u15::new(value));
            }
            0xFD => {
                sys.dsp.dsp_mailbox.set_low(value);
                sys.dsp.dsp_mailbox.set_status(true);
            }
            _ => unimplemented!("write to {offset:02X}"),
        }
    }

    /// Reads from data memory.
    pub fn read_dmem(&mut self, sys: &mut System, addr: u16) -> u16 {
        match addr {
            0x0000..0x1000 => self.mem.dram[addr as usize],
            0x1000..0x1800 => self.mem.coef[addr as usize - 0x1000],
            0xFF00.. => self.read_mmio(sys, addr as u8),
            _ => {
                std::hint::cold_path();
                0
            }
        }
    }

    /// Writes to data memory.
    pub fn write_dmem(&mut self, sys: &mut System, addr: u16, value: u16) {
        match addr {
            0x0000..0x1000 => self.mem.dram[addr as usize] = value,
            0x1000..0x1800 => {
                std::hint::cold_path();
                tracing::warn!("writing to coefficient data");
            }
            0xFF00.. => self.write_mmio(sys, addr as u8, value),
            _ => (),
        }
    }

    /// Reads from instruction memory.
    #[inline(always)]
    pub fn read_imem(&mut self, addr: u16) -> u16 {
        match addr {
            0x0000..0x1000 => self.mem.iram[addr as usize],
            0x8000..0x9000 => {
                std::hint::cold_path();
                self.mem.irom[addr as usize - 0x8000]
            }
            _ => {
                std::hint::cold_path();
                0
            }
        }
    }

    /// Writes to instruction memory.
    #[inline(always)]
    pub fn write_imem(&mut self, addr: u16, value: u16) {
        if let 0x0000..0x1000 = addr {
            self.mem.iram[addr as usize] = value
        }
    }

    fn is_waiting_for_cpu_mail_inner(&mut self, offset: i16) -> bool {
        let start = self.pc.wrapping_add_signed(offset);
        let pattern_a = [
            // lrs   $ACM0, @cmbh
            0b0010_0110_1111_1110,
            // andcf $ACM0, #0x8000
            0b0000_0010_1100_0000,
            0x8000,
            // jlnz	 start
            0b0000_0010_1001_1100,
            start,
        ];

        let pattern_b = [
            // lrs   $ACM1, @cmbh
            0b0010_0111_1111_1110,
            // andcf $ACM1, #0x8000
            0b0000_0011_1100_0000,
            0x8000,
            // jlnz	 start
            0b0000_0010_1001_1100,
            start,
        ];

        let current = [
            self.read_imem(start),
            self.read_imem(start.wrapping_add(1)),
            self.read_imem(start.wrapping_add(2)),
            self.read_imem(start.wrapping_add(3)),
            self.read_imem(start.wrapping_add(4)),
        ];

        current == pattern_a || current == pattern_b
    }

    #[inline(always)]
    pub fn is_waiting_for_cpu_mail(&mut self) -> bool {
        self.is_waiting_for_cpu_mail_inner(0)
            || self.is_waiting_for_cpu_mail_inner(-1)
            || self.is_waiting_for_cpu_mail_inner(-3)
    }

    fn is_waiting_for_dsp_mail_inner(&mut self, offset: i16) -> bool {
        let start = self.pc.wrapping_add_signed(offset);
        let pattern_a = [
            // lrs   $ACM0, @dmbh
            0b0010_0110_1111_1100,
            // andcf $ACM0, #0x8000
            0b0000_0010_1100_0000,
            0x8000,
            // jlz	 start
            0b0000_0010_1001_1101,
            start,
        ];

        let pattern_b = [
            // lrs   $ACM1, @dmbh
            0b0010_0111_1111_1100,
            // andcf $ACM1, #0x8000
            0b0000_0011_1100_0000,
            0x8000,
            // jlz	 start
            0b0000_0010_1001_1101,
            start,
        ];

        let current = [
            self.read_imem(start),
            self.read_imem(start.wrapping_add(1)),
            self.read_imem(start.wrapping_add(2)),
            self.read_imem(start.wrapping_add(3)),
            self.read_imem(start.wrapping_add(4)),
        ];

        current == pattern_a || current == pattern_b
    }

    #[inline(always)]
    pub fn is_waiting_for_dsp_mail(&mut self) -> bool {
        self.is_waiting_for_dsp_mail_inner(0)
            || self.is_waiting_for_dsp_mail_inner(-1)
            || self.is_waiting_for_dsp_mail_inner(-3)
    }

    fn fetch_decode_and_cache(&mut self) -> CachedIns {
        // fetch
        let mut ins = Ins::new(self.read_imem(self.pc));

        // decode
        let decoded = ins.decoded();
        let extra = decoded
            .needs_extra
            .then_some(self.read_imem(self.pc.wrapping_add(1)));

        let len = if let Some(extra) = extra {
            ins.extra = extra;
            2
        } else {
            1
        };

        let main = OPCODE_EXEC_LUT[decoded.opcode as usize];
        let extension = decoded
            .extension
            .map(|extension| EXTENSION_EXEC_LUT[extension as usize]);

        // cache
        let cached = CachedIns {
            ins,
            len,
            main,
            extension,
        };
        self.cached[self.pc as usize] = Some(cached);

        cached
    }

    pub fn exec(&mut self, sys: &mut System, instructions: u32) {
        let mut i = 0;
        while i < instructions {
            if sys.dsp.control.halt() {
                std::hint::cold_path();
                break;
            }

            self.check_interrupts(sys);
            self.check_stacks();

            // have we cached this instruction already?
            let ins = if let Some(cached) = self.cached[self.pc as usize] {
                cached
            } else {
                std::hint::cold_path();
                self.fetch_decode_and_cache()
            };

            // execute
            if let Some(extension) = ins.extension {
                let regs_previous = self.regs.clone();
                (ins.main)(self, sys, ins.ins);
                (extension)(self, sys, ins.ins, &regs_previous);
            } else {
                (ins.main)(self, sys, ins.ins);
            }

            if let Some(loop_counter) = &mut self.loop_counter {
                if *loop_counter == 0 {
                    std::hint::cold_path();
                    self.loop_counter = None;
                    self.pc += 1;
                } else {
                    *loop_counter -= 1;
                }
            } else {
                self.pc = self.pc.wrapping_add(ins.len);
            }

            i += 1;
        }
    }

    pub fn step(&mut self, sys: &mut System) {
        self.exec(sys, 1);
    }
}
