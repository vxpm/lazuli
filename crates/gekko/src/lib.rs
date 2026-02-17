//! The `powerpc` crate, which is a disassembler of PowerPC instructions, is re-exported under
//! [`disasm`].

use std::time::Duration;

use bitos::integer::{i6, u2, u4, u5, u7, u11, u15, u27};
use bitos::{BitUtils, bitos};
use strum::{FromRepr, VariantArray};
use util::offset_of;
use zerocopy::{FromBytes, Immutable, IntoBytes};

/// Disassembling of PowerPC instructions. Re-export of the [`powerpc`] crate.
#[rustfmt::skip]
pub use powerpc as disasm;

/// An address in the Gekko's memory address space. This is a thin wrapper around an [`u32`].
#[repr(transparent)]
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash, IntoBytes, FromBytes, Immutable,
)]
pub struct Address(pub u32);

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "0x{:04X}_{:04X}",
            (self.0 & 0xFFFF_0000) >> 16,
            self.0 & 0xFFFF
        )
    }
}

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Address {
    /// Returns the value of this address. Equivalent to `self.0`.
    #[inline(always)]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Whether this address is null.
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Aligns this address down to the given alignment.
    pub const fn align_down(self, alignment: u32) -> Self {
        let rem = self.0 % alignment;
        Self(self.0 - rem)
    }

    /// Aligns this address up to the given alignment.
    pub const fn align_up(self, alignment: u32) -> Self {
        Self(self.0.next_multiple_of(alignment))
    }
}

impl std::ops::Add<u32> for Address {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0.wrapping_add(rhs))
    }
}

impl std::ops::Add<i32> for Address {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: i32) -> Self::Output {
        Self(self.0.wrapping_add_signed(rhs))
    }
}

impl std::ops::AddAssign<u32> for Address {
    #[inline(always)]
    fn add_assign(&mut self, rhs: u32) {
        *self = *self + rhs;
    }
}

impl std::ops::AddAssign<i32> for Address {
    #[inline(always)]
    fn add_assign(&mut self, rhs: i32) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub<u32> for Address {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: u32) -> Self::Output {
        Self(self.0.wrapping_sub(rhs))
    }
}

impl std::ops::Sub<i32> for Address {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: i32) -> Self::Output {
        Self(self.0.wrapping_sub_signed(rhs))
    }
}

impl std::ops::Sub<Address> for Address {
    type Output = i64;

    #[inline(always)]
    fn sub(self, rhs: Address) -> Self::Output {
        self.0 as i64 - rhs.0 as i64
    }
}

impl std::ops::SubAssign<u32> for Address {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: u32) {
        *self = *self - rhs;
    }
}

impl std::ops::SubAssign<i32> for Address {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: i32) {
        *self = *self - rhs;
    }
}

impl PartialEq<u32> for Address {
    #[inline(always)]
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl From<u32> for Address {
    #[inline(always)]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Address> for u32 {
    #[inline(always)]
    fn from(value: Address) -> Self {
        value.0
    }
}

/// The CPU frequency.
pub const FREQUENCY: u64 = 486_000_000;

/// An amount of cycles of the Gekko CPU. This is a thin wrapper around an [`u64`].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Hash,
    IntoBytes,
    FromBytes,
    Immutable,
)]
pub struct Cycles(pub u64);

impl std::fmt::Display for Cycles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Cycles {
    /// Cycles per second of the CPU. This is an alias for the [`FREQUENCY`] constant, wrapped in
    /// [`Cycles`].
    pub const PER_SECOND: Self = Self(FREQUENCY);

    /// Returns the value of this address. Equivalent to `self.0`.
    #[inline(always)]
    pub const fn value(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub const fn from_secs_f64(secs: f64) -> Self {
        Self((secs * Self::PER_SECOND.0 as f64).round() as u64)
    }

    #[inline(always)]
    pub const fn from_duration(duration: Duration) -> Self {
        Self::from_secs_f64(duration.as_secs_f64())
    }

    #[inline(always)]
    pub fn to_duration(&self) -> Duration {
        Duration::from_secs_f64(self.0 as f64 / Self::PER_SECOND.0 as f64)
    }

    #[inline(always)]
    pub fn to_dsp_cycles(&self) -> f64 {
        self.0 as f64 / 6.0
    }
}

impl std::ops::Add<Cycles> for Cycles {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Cycles) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Add<u64> for Cycles {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl std::ops::Add<i64> for Cycles {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: i64) -> Self::Output {
        if rhs >= 0 {
            self + (rhs as u64)
        } else {
            self - ((-rhs) as u64)
        }
    }
}

impl std::ops::AddAssign<Cycles> for Cycles {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Cycles) {
        *self = *self + rhs;
    }
}

impl std::ops::AddAssign<u64> for Cycles {
    #[inline(always)]
    fn add_assign(&mut self, rhs: u64) {
        *self = *self + rhs;
    }
}

impl std::ops::AddAssign<i64> for Cycles {
    #[inline(always)]
    fn add_assign(&mut self, rhs: i64) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub<u64> for Cycles {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: u64) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl std::ops::Sub<i64> for Cycles {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: i64) -> Self::Output {
        self + (-rhs)
    }
}

impl std::ops::Sub<Cycles> for Cycles {
    type Output = Cycles;

    #[inline(always)]
    fn sub(self, rhs: Cycles) -> Self::Output {
        Self(self.0.checked_sub(rhs.0).expect("cycles sub overflow"))
    }
}

impl std::ops::SubAssign<Cycles> for Cycles {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Cycles) {
        *self = *self - rhs;
    }
}

impl std::ops::SubAssign<u64> for Cycles {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: u64) {
        *self = *self - rhs;
    }
}

impl std::ops::SubAssign<i64> for Cycles {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: i64) {
        *self = *self - rhs;
    }
}

impl PartialEq<u64> for Cycles {
    #[inline(always)]
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Cycles> for u64 {
    #[inline(always)]
    fn eq(&self, other: &Cycles) -> bool {
        *self == other.0
    }
}

impl PartialOrd<u64> for Cycles {
    fn partial_cmp(&self, other: &u64) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(other))
    }
}

impl PartialOrd<Cycles> for u64 {
    fn partial_cmp(&self, other: &Cycles) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other.0))
    }
}

impl From<u64> for Cycles {
    #[inline(always)]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Cycles> for u64 {
    #[inline(always)]
    fn from(value: Cycles) -> Self {
        value.0
    }
}

/// Extension trait for [`Ins`](disasm::Ins).
pub trait InsExt {
    /// GPR indicated by field rA.
    fn gpr_a(&self) -> GPR;
    /// GPR indicated by field rB.
    fn gpr_b(&self) -> GPR;
    /// GPR indicated by field rS.
    fn gpr_s(&self) -> GPR;
    /// GPR indicated by field rD.
    fn gpr_d(&self) -> GPR;
    /// FPR indicated by field frA.
    fn fpr_a(&self) -> FPR;
    /// FPR indicated by field frB.
    fn fpr_b(&self) -> FPR;
    /// FPR indicated by field frC.
    fn fpr_c(&self) -> FPR;
    /// FPR indicated by field frS.
    fn fpr_s(&self) -> FPR;
    /// FPR indicated by field frD.
    fn fpr_d(&self) -> FPR;
    /// SPR indicated by field SPR.
    fn spr(&self) -> SPR;
}

impl InsExt for disasm::Ins {
    #[inline(always)]
    fn gpr_a(&self) -> GPR {
        GPR::new(self.field_ra())
    }

    #[inline(always)]
    fn gpr_b(&self) -> GPR {
        GPR::new(self.field_rb())
    }

    #[inline(always)]
    fn gpr_s(&self) -> GPR {
        GPR::new(self.field_rs())
    }

    #[inline(always)]
    fn gpr_d(&self) -> GPR {
        GPR::new(self.field_rd())
    }

    #[inline(always)]
    fn fpr_a(&self) -> FPR {
        FPR::new(self.field_fra())
    }

    #[inline(always)]
    fn fpr_b(&self) -> FPR {
        FPR::new(self.field_frb())
    }

    #[inline(always)]
    fn fpr_c(&self) -> FPR {
        FPR::new(self.field_frc())
    }

    #[inline(always)]
    fn fpr_s(&self) -> FPR {
        FPR::new(self.field_frs())
    }

    #[inline(always)]
    fn fpr_d(&self) -> FPR {
        FPR::new(self.field_frd())
    }

    #[inline(always)]
    fn spr(&self) -> SPR {
        SPR::new(self.field_spr())
    }
}

/// An exception which can be generated by the Gekko CPU. The variants have the lower 16 bits of the
/// exception vector as their values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Exception {
    Reset              = 0x0100,
    MachineCheck       = 0x0200,
    DSI                = 0x0300,
    ISI                = 0x0400,
    Interrupt          = 0x0500,
    Alignment          = 0x0600,
    Program            = 0x0700,
    FloatUnavailable   = 0x0800,
    Decrementer        = 0x0900,
    Syscall            = 0x0C00,
    Trace              = 0x0D00,
    PerformanceMonitor = 0x0F00,
    Breakpoint         = 0x1300,
}

impl Exception {
    #[rustfmt::skip]    pub const SPECIAL_SRR1_BITS_MASK: u32 = 0b0111_1000_0011_1100_0000_0000_0000_0000_u32;
    #[rustfmt::skip]    pub const MSR_TO_SRR1_MASK:       u32 = 0b0000_0111_1100_0000_1111_1111_1111_1111_u32;
    #[rustfmt::skip]    pub const SRR1_TO_MSR_MASK:       u32 = 0b1000_0111_1100_0000_1111_1111_0111_0011_u32;

    pub fn srr0_skip(self) -> bool {
        matches!(self, Self::Syscall)
    }
}

/// A condition group field in the [`CondReg`].
#[bitos(4)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Cond {
    /// Whether the result has overflowed.
    #[bits(0)]
    pub ov: bool,
    /// Whether the operands are equal.
    #[bits(1)]
    pub eq: bool,
    /// Whether the first operand is greater than the second.
    #[bits(2)]
    pub gt: bool,
    /// Whether the first operand is less than the second.
    #[bits(3)]
    pub lt: bool,
}

/// The condition register (CR) contains 8 fields, named CR0-CR7, each containing flags
/// corresponding to some comparison operation.
///
/// There are two special cases:
/// - CR0: Integer instructions which have the `Rc` flag set update CR0 to contain comparisons to
///   zero and an overflow bit.
/// - CR1: Floating point instructions which have the `Rc` flag set update CR1 to contain a copy of
///   bits 0..4 of the FPSCR, indicating floating point exception status.
///
/// Other than that, comparison instructions specify one of the fields to receive the results of
/// the comparison.
#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CondReg {
    // NOTE: CR0 is actually index 7! PPC bit order is big endian
    #[bits(..)]
    pub fields: [Cond; 8],
}

/// The Machine State register.
#[bitos(32)]
#[derive(Debug, Clone, PartialEq)]
pub struct MachineState {
    /// Whether little endian mode is turned on. Not supported.
    #[bits(0)]
    pub little_endian: bool,
    /// Whether the last exception is recoverable.
    #[bits(1)]
    pub recoverable_exception: bool,
    /// Performance monitor. Not supported.
    #[bits(2)]
    pub performance_monitor: bool,
    /// Whether data address translation is enabled.
    #[bits(4)]
    pub data_addr_translation: bool,
    /// Whether instruction address translation is enabled.
    #[bits(5)]
    pub instr_addr_translation: bool,
    /// Whether exception vectors are at 0x0000_nnnn (off) or 0xFFF0_nnnn (on).
    #[bits(6)]
    pub exception_prefix: bool,
    #[bits(8)]
    pub float_exception_mode_1: bool,
    /// Branch trace enable. Not supported.
    #[bits(9)]
    pub branch_trace: bool,
    /// Step trace enable. Not supported.
    #[bits(10)]
    pub step_trace: bool,
    #[bits(11)]
    pub float_exception_mode_0: bool,
    /// Whether machine check exceptions are enabled. Not supported.
    #[bits(12)]
    pub machine_check: bool,
    /// Whether floating point instructions can be used.
    #[bits(13)]
    pub float_available: bool,
    /// Whether the processor is running in user mode. Not supported.
    #[bits(14)]
    pub user_mode: bool,
    /// Whether external exceptions are enabled.
    #[bits(15)]
    pub interrupts: bool,
    /// Whether the CPU should be set to little endian mode after an exception occurs. Not
    /// supported.
    #[bits(16)]
    pub exception_little_endian: bool,
    /// Power management. Not supported.
    #[bits(18)]
    pub reduced_power: bool,
}

impl Default for MachineState {
    fn default() -> Self {
        Self(0).with_exception_prefix(true)
    }
}

impl MachineState {
    pub fn enter_exception_mode(&mut self) {
        let prev = self.clone();
        *self = MachineState::from_bits(0)
            .with_little_endian(prev.exception_little_endian())
            .with_exception_prefix(prev.exception_prefix())
            .with_machine_check(prev.machine_check())
            .with_exception_little_endian(prev.exception_little_endian());
    }
}

/// The XER register contains information about overflow and carry operations, and is also used by
/// the load/store string indexed instructions.
#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XerReg {
    /// The number of bytes to be transferred by a lswx or stswx.
    #[bits(0..7)]
    pub byte_count: u7,
    /// Used by carrying instructions, contains the carry bit of the result.
    #[bits(29)]
    pub carry: bool,
    /// Whether an overflow has occured.
    #[bits(30)]
    pub overflow: bool,
    /// Set whenever the overflow bit is set and stays set until cleared by specific instructions.
    #[bits(31)]
    pub overflow_fuse: bool,
}

#[bitos(4)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatCond {
    UnordedOrNaN = 0b0001,
    Equal        = 0b0010,
    GreaterThan  = 0b0100,
    LessThan     = 0b1000,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatRounding {
    Nearest       = 0b00,
    TowardsZero   = 0b01,
    TowardsPosInf = 0b10,
    TowardsNegInf = 0b11,
}

#[bitos(5)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FloatResultFlags {
    #[bits(0..4)]
    pub cond: Option<FloatCond>,
    #[bits(4)]
    pub class: bool,
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FloatControlReg {
    /// Floating-point rounding mode.
    #[bits(0..2)]
    pub rounding: FloatRounding,
    /// Whether IEEE conformance is disabled.
    #[bits(2)]
    pub ieee_disabled: bool,
    /// Whether inexact exceptions are enabled.
    #[bits(3)]
    pub inexact_exception_enabled: bool,
    /// Whether zero divide exceptions are enabled.
    #[bits(4)]
    pub zero_divide_exception_enabled: bool,
    /// Whether underflow exceptions are enabled.
    #[bits(5)]
    pub underflow_exception_enabled: bool,
    /// Whether overflow exceptions are enabled.
    #[bits(6)]
    pub overflow_exception_enabled: bool,
    /// Whether invalid operation exceptions are enabled.
    #[bits(7)]
    pub invalid_exception_enabled: bool,
    /// Invalid operation exception for invalid integer conversion.
    #[bits(8)]
    pub invalid_conversion_exception: bool,
    /// Invalid operation exception for invalid square root.
    #[bits(9)]
    pub invalid_sqrt_exception: bool,
    /// Invalid operation exception for software request.
    #[bits(10)]
    pub invalid_soft_exception: bool,
    /// Result flags.
    #[bits(12..17)]
    pub result_flags: FloatResultFlags,
    /// Whether the last arithmethic or rounding and conversion instruction rounded an intermediate
    /// result or caused a disabled overflow exception.
    #[bits(17)]
    pub fraction_inexact: bool,
    /// Whether the last arithmethic or rounding and conversion instruction that rounded an
    /// intermediate result incremented the fraction.
    #[bits(18)]
    pub fraction_rounded: bool,
    /// Invalid operation exception for invalid comparison.
    #[bits(19)]
    pub invalid_compare_exception: bool,
    /// Invalid operation exception for `inf * zero`.
    #[bits(20)]
    pub invalid_inf_mul_zero_exception: bool,
    /// Invalid operation exception for `zero / zero`.
    #[bits(21)]
    pub invalid_zero_div_zero_exception: bool,
    /// Invalid operation exception for `inf / inf`.
    #[bits(22)]
    pub invalid_inf_div_inf_exception: bool,
    /// Invalid operation exception for `inf - inf`.
    #[bits(23)]
    pub invalid_inf_sub_inf_exception: bool,
    /// Invalid operation exception for signaling NaN.
    #[bits(24)]
    pub invalid_snan_exception: bool,
    /// Inexact exception.
    #[bits(25)]
    pub inexact_exception: bool,
    /// Zero divide exception.
    #[bits(26)]
    pub zero_divide_exception: bool,
    /// Underflow exception.
    #[bits(27)]
    pub underflow_exception: bool,
    /// Overflow exception.
    #[bits(28)]
    pub overflow_exception: bool,
    /// Floating-point exception summary, i.e. whether any of the invalid operation exception bits
    /// have been set. This bit cannot be changed by software.
    #[bits(29)]
    pub invalid_op_exception_summary: bool,
    /// Same as the floating-point exception summary, except it only considers enabled exceptions.
    /// This bit cannot be changed by software.
    #[bits(30)]
    pub enabled_exception_summary: bool,
    /// Floating-point exception summary, i.e. whether any of the exception bits have been set.
    #[bits(31)]
    pub exception_summary: bool,
}

/// A pair of double precision floating point numbers, used by the paired singles extension.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct FloatPair(pub [f64; 2]);

impl std::ops::Deref for FloatPair {
    type Target = [f64; 2];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for FloatPair {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// User level registers.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct User {
    /// General Purpose Registers
    pub gpr: [u32; 32],
    /// Floating Point Registers
    pub fpr: [FloatPair; 32],
    /// Condition Register
    pub cr: CondReg,
    /// Floating Point Status and Condition Register
    pub fpscr: FloatControlReg,

    /// XER Register
    pub xer: XerReg,
    /// Link Register
    pub lr: u32,
    /// Count Register
    pub ctr: u32,
}

/// A Block Address Translation register.
#[bitos(64)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bat {
    // lower
    #[bits(0..2)]
    pub protection: u2,
    #[bits(3..7)]
    pub wimg: u4,
    #[bits(17..32)]
    pub physical_address_region: u15,

    // upper
    #[bits(32)]
    pub user_mode: bool,
    #[bits(33)]
    pub supervisor_mode: bool,
    #[bits(34..45)]
    pub block_length_mask: u11,
    #[bits(49..64)]
    pub effective_address_region: u15,
}

impl Bat {
    /// The length of the memory region, in bytes.
    #[inline(always)]
    pub fn block_length(&self) -> u32 {
        (bytesize::kib(128u64) as u32) << (self.block_length_mask().value()).count_ones()
    }

    /// The start address of the memory region, inclusive.
    #[inline(always)]
    pub fn logical_start(&self) -> Address {
        Address(
            ((self.effective_address_region().value() as u32) << 17)
            // mask the EPI with the block length! aka floor it to a multiple of block length
                & !((self.block_length_mask().value() as u32) << 17),
        )
    }

    /// The start address of the physical memory region, inclusive.
    #[inline(always)]
    pub fn physical_start(&self) -> Address {
        Address(
            ((self.physical_address_region().value() as u32) << 17)
            // mask the EPI with the block length! aka floor it to a multiple of block length
                & !((self.block_length_mask().value() as u32) << 17),
        )
    }

    /// The end address of the memory region, inclusive.
    #[inline(always)]
    pub fn logical_end(&self) -> Address {
        self.logical_start() + (self.block_length() - 1)
    }

    /// The end address of the memory region, inclusive.
    #[inline(always)]
    pub fn physical_end(&self) -> Address {
        self.physical_start() + (self.block_length() - 1)
    }

    /// Whether the memory region contains the given logical address.
    #[inline(always)]
    pub fn contains(&self, addr: Address) -> bool {
        (self.logical_start()..=self.logical_end()).contains(&addr)
    }

    /// Translates a logical address into a physical address.
    #[inline(always)]
    pub fn translate(&self, addr: Address) -> Address {
        let offset = addr.value().bits(0, 17);
        let region = ((addr.value().bits(17, 28) << 17)
            // only allow bits within the block length to be changed
            & ((self.block_length_mask().value() as u32) << 17))
            // insert the real page number
            | ((self.physical_address_region().value() as u32) << 17);

        Address(region | offset)
    }
}

/// Memory management registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemoryManagement {
    /// Instruction Block Address Translation registers
    pub ibat: [Bat; 4],
    /// Data Block Address Translation registers
    pub dbat: [Bat; 4],
    /// Segment Registers
    pub sr: [u32; 16],
    /// Page table base address (?)
    pub sdr1: u32,
}

impl MemoryManagement {
    /// Default configuration for BATs used by the Dolphin OS.
    pub fn setup_default_bats(&mut self) {
        let bat = |upper, lower| {
            use zerocopy::big_endian::{U32, U64};
            use zerocopy::transmute;

            let data: U64 = transmute!([U32::new(upper), U32::new(lower)]);
            Bat::from_bits(data.get())
        };

        self.ibat[0] = bat(0x8000_1FFF, 0x0000_0002);
        self.ibat[1] = bat(0x0000_0000, 0x0000_0000);
        self.ibat[2] = bat(0x0000_0000, 0x0000_0000);
        self.ibat[3] = bat(0xFFF0_001F, 0xFFF0_0001);

        self.dbat[0] = bat(0x8000_1FFF, 0x0000_0002);
        self.dbat[1] = bat(0xC000_1FFF, 0x0000_002A);
        self.dbat[2] = bat(0x0000_0000, 0x0000_0000);
        self.dbat[3] = bat(0xFFF0_001F, 0xFFF0_0001);
    }
}

/// Exception handling registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExceptionHandling {
    /// Data Address Register
    pub dar: u32,
    /// Data Storage Interrupt Status Register
    pub dsisr: u32,
    /// Registers provided for the use of the operating system
    pub sprg: [u32; 4],
    /// Save and Restore Registers
    pub srr: [u32; 2],
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WriteGatherPipe {
    /// Whether the write gather buffer has any data
    #[bits(0)]
    pub buffer_not_empty: bool,
    /// Top 27 bits of the address
    #[bits(5..32)]
    pub address_base: u27,
}

impl WriteGatherPipe {
    pub fn address(&self) -> Address {
        Address(self.address_base().value() << 5)
    }
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DmaConfigUpper {
    #[bits(0..5)]
    pub length_upper: u5,
    #[bits(5..32)]
    pub mem_base: u27,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmaDirection {
    #[default]
    FromCacheToRam = 0,
    FromRamToCache = 1,
}

#[bitos(32)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DmaConfigLower {
    #[bits(0)]
    pub flush: bool,
    #[bits(1)]
    pub trigger: bool,
    #[bits(2..4)]
    pub length_lower: u2,
    #[bits(4)]
    pub direction: DmaDirection,
    #[bits(5..32)]
    pub cache_base: u27,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DmaConfig {
    pub upper: DmaConfigUpper,
    pub lower: DmaConfigLower,
}

impl DmaConfig {
    pub fn mem_address(&self) -> Address {
        Address(self.upper.mem_base().value() << 5)
    }

    pub fn cache_address(&self) -> Address {
        Address(self.lower.cache_base().value() << 5)
    }

    pub fn length(&self) -> u32 {
        let base = 0
            .with_bits(0, 2, self.lower.length_lower().value() as u32)
            .with_bits(2, 7, self.upper.length_upper().value() as u32)
            << 5;

        if base == 0 { 4096 } else { base }
    }
}

/// Configuration registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Configuration {
    /// Machine State Register
    pub msr: MachineState,
    /// Hardware Implementation Dependent registers
    pub hid: [u32; 3],
    /// Write Gather Pipe configuration
    pub wpar: WriteGatherPipe,
    /// DMA configuration
    pub dma: DmaConfig,
}

/// A quantized type.
#[bitos(3)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuantizedType {
    #[default]
    Float,
    Reserved0,
    Reserved1,
    Reserved2,
    U8,
    U16,
    I8,
    I16,
}

impl QuantizedType {
    pub fn size(&self) -> u8 {
        match self {
            Self::Float => 4,
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            _ => panic!("reserved quantized type"),
        }
    }
}

/// A graphics quantization register.
#[bitos(32)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuantReg {
    /// Type of operand resulting from a conversion by a store instruction
    #[bits(0..3)]
    pub store_type: QuantizedType,
    /// Scale used by a store instruction
    #[bits(8..14)]
    pub store_scale: i6,
    /// Type of operand resulting from a conversion by a load instruction
    #[bits(16..19)]
    pub load_type: QuantizedType,
    /// Scale used by a load instruction
    #[bits(24..30)]
    pub load_scale: i6,
}

pub static DEQUANTIZATION_LUT: [f64; 1 << 6] = {
    let mut result = [0.0; 1 << 6];

    let mut i = 0;
    loop {
        let scale = ((i as i8) << 2) >> 2;
        let exp = scale.unsigned_abs();
        let factor = if scale >= 0 {
            1.0 / ((1 << exp) as f64)
        } else {
            (1u64 << exp) as f64
        };

        result[i as usize] = factor;

        i += 1;
        if i >= (1 << 6) {
            break;
        }
    }

    result
};

pub static QUANTIZATION_LUT: [f64; 1 << 6] = {
    let mut result = DEQUANTIZATION_LUT;

    let mut i = 0;
    loop {
        result[i] = 1.0 / result[i];

        i += 1;
        if i >= (1 << 6) {
            break;
        }
    }

    result
};

/// Miscellaneous registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Miscellaneous {
    /// Time Base
    pub tb: u64,
    /// Decrementer
    pub dec: u32,
    /// L2 Control
    pub l2cr: u32,
}

/// Performance monitor registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerformanceMonitor {
    /// Performance Counter registers
    pub counters: [u32; 4],
    /// Monitor Control registers
    pub control: [u32; 2],
}

/// Supervisor level registers.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Supervisor {
    /// Configuration registers
    pub config: Configuration,
    /// Memory management registers
    pub memory: MemoryManagement,
    /// Exception handling registers
    pub exception: ExceptionHandling,
    /// Graphics Quantization registers
    pub gq: [QuantReg; 8],
    /// Performance monitor registers
    pub performance: PerformanceMonitor,
    /// Miscellaneous registers
    pub misc: Miscellaneous,
}

/// Structure of all the registers in the PowerPC Gekko CPU.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cpu {
    /// Program Counter
    pub pc: Address,
    /// User level registers
    pub user: User,
    /// Supervisor level registers
    pub supervisor: Supervisor,
}

impl Cpu {
    /// Takes an exception.
    pub fn raise_exception(&mut self, exception: Exception) {
        if exception == Exception::Decrementer {
            tracing::trace!("raised exception {exception:?} at {}", self.pc);
        } else {
            tracing::debug!("raised exception {exception:?} at {}", self.pc);
        }

        // save PC into SRR0
        self.supervisor.exception.srr[0] = self.pc.value();
        if exception.srr0_skip() {
            self.supervisor.exception.srr[0] += 4;
        }

        // save MSR into SRR1
        let mask = Exception::MSR_TO_SRR1_MASK;
        self.supervisor.exception.srr[1] &= !mask;
        self.supervisor.exception.srr[1] |= self.supervisor.config.msr.to_bits() & mask;

        // set exception specific bits in SRR1
        // NOTE: just clear them for now
        self.supervisor.exception.srr[1] &= !Exception::SPECIAL_SRR1_BITS_MASK;

        // update MSR
        self.supervisor.config.msr.enter_exception_mode();

        // jump to exception vector
        let base = if self.supervisor.config.msr.exception_prefix() {
            0xFFF0_0000
        } else {
            0x0000_0000
        };

        self.pc = Address(base | exception as u32);
    }
}

/// A General Purpose Register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromRepr, VariantArray)]
#[repr(u8)]
pub enum GPR {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    R16,
    R17,
    R18,
    R19,
    R20,
    R21,
    R22,
    R23,
    R24,
    R25,
    R26,
    R27,
    R28,
    R29,
    R30,
    R31,
}

impl GPR {
    /// Creates a new GPR with the given index.
    ///
    /// # Panics
    /// Panics if index is out of range.
    #[inline(always)]
    pub fn new(index: u8) -> Self {
        Self::from_repr(index).unwrap()
    }

    /// Offset of this GPR in the [`Cpu`] struct.
    #[inline(always)]
    pub fn offset(self) -> usize {
        offset_of!(Cpu, user.gpr) + size_of::<u32>() * (self as usize)
    }
}

/// A Floating Point Register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromRepr, VariantArray)]
#[repr(u8)]
pub enum FPR {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    R16,
    R17,
    R18,
    R19,
    R20,
    R21,
    R22,
    R23,
    R24,
    R25,
    R26,
    R27,
    R28,
    R29,
    R30,
    R31,
}

impl FPR {
    /// Creates a new FPR with the given index.
    ///
    /// # Panics
    /// Panics if index is out of range.
    #[inline(always)]
    pub fn new(index: u8) -> Self {
        Self::from_repr(index).unwrap()
    }

    /// Offset of this FPR in the [`Cpu`] struct.
    #[inline(always)]
    pub fn offset(self) -> usize {
        offset_of!(Cpu, user.fpr) + size_of::<FloatPair>() * (self as usize)
    }
}

/// A Special Purpose Register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromRepr, VariantArray)]
#[repr(u16)]
pub enum SPR {
    XER    = 1,
    LR     = 8,
    CTR    = 9,
    DSISR  = 18,
    DAR    = 19,
    DEC    = 22,
    SDR1   = 25,
    SRR0   = 26,
    SRR1   = 27,
    SPRG0  = 272,
    SPRG1  = 273,
    SPRG2  = 274,
    SPRG3  = 275,
    TBL    = 284,
    TBU    = 285,
    IBAT0U = 528,
    IBAT0L = 529,
    IBAT1U = 530,
    IBAT1L = 531,
    IBAT2U = 532,
    IBAT2L = 533,
    IBAT3U = 534,
    IBAT3L = 535,
    DBAT0U = 536,
    DBAT0L = 537,
    DBAT1U = 538,
    DBAT1L = 539,
    DBAT2U = 540,
    DBAT2L = 541,
    DBAT3U = 542,
    DBAT3L = 543,
    GQR0   = 912,
    GQR1   = 913,
    GQR2   = 914,
    GQR3   = 915,
    GQR4   = 916,
    GQR5   = 917,
    GQR6   = 918,
    GQR7   = 919,
    HID2   = 920,
    WPAR   = 921,
    DMAU   = 922,
    DMAL   = 923,
    MMCR0  = 952,
    PMC1   = 953,
    PMC2   = 954,
    MMCR1  = 956,
    PMC3   = 957,
    PMC4   = 958,
    HID0   = 1008,
    HID1   = 1009,
    L2CR   = 1017,
}

impl SPR {
    pub const GQR: [Self; 8] = [
        Self::GQR0,
        Self::GQR1,
        Self::GQR2,
        Self::GQR3,
        Self::GQR4,
        Self::GQR5,
        Self::GQR6,
        Self::GQR7,
    ];

    /// Creates a new SPR with the given index.
    ///
    /// # Panics
    /// Panics if index is out of range or is unknown.
    #[inline(always)]
    pub fn new(index: u16) -> Self {
        match Self::from_repr(index) {
            Some(spr) => spr,
            None => panic!("unknown SPR {index}"),
        }
    }

    /// Offset of this SPR in the [`Cpu`] struct.
    pub fn offset(self) -> usize {
        match self {
            Self::XER => offset_of!(Cpu, user.xer),
            Self::LR => offset_of!(Cpu, user.lr),
            Self::CTR => offset_of!(Cpu, user.ctr),
            Self::DSISR => offset_of!(Cpu, supervisor.exception.dsisr),
            Self::DAR => offset_of!(Cpu, supervisor.exception.dar),
            Self::DEC => offset_of!(Cpu, supervisor.misc.dec),
            Self::SDR1 => offset_of!(Cpu, supervisor.memory.sdr1),
            Self::SRR0 => offset_of!(Cpu, supervisor.exception.srr[0]),
            Self::SRR1 => offset_of!(Cpu, supervisor.exception.srr[1]),
            Self::SPRG0 => offset_of!(Cpu, supervisor.exception.sprg[0]),
            Self::SPRG1 => offset_of!(Cpu, supervisor.exception.sprg[1]),
            Self::SPRG2 => offset_of!(Cpu, supervisor.exception.sprg[2]),
            Self::SPRG3 => offset_of!(Cpu, supervisor.exception.sprg[3]),
            Self::TBL => offset_of!(Cpu, supervisor.misc.tb),
            Self::TBU => offset_of!(Cpu, supervisor.misc.tb) + 4,
            Self::IBAT0U => offset_of!(Cpu, supervisor.memory.ibat[0]) + 4,
            Self::IBAT0L => offset_of!(Cpu, supervisor.memory.ibat[0]),
            Self::IBAT1U => offset_of!(Cpu, supervisor.memory.ibat[1]) + 4,
            Self::IBAT1L => offset_of!(Cpu, supervisor.memory.ibat[1]),
            Self::IBAT2U => offset_of!(Cpu, supervisor.memory.ibat[2]) + 4,
            Self::IBAT2L => offset_of!(Cpu, supervisor.memory.ibat[2]),
            Self::IBAT3U => offset_of!(Cpu, supervisor.memory.ibat[3]) + 4,
            Self::IBAT3L => offset_of!(Cpu, supervisor.memory.ibat[3]),
            Self::DBAT0U => offset_of!(Cpu, supervisor.memory.dbat[0]) + 4,
            Self::DBAT0L => offset_of!(Cpu, supervisor.memory.dbat[0]),
            Self::DBAT1U => offset_of!(Cpu, supervisor.memory.dbat[1]) + 4,
            Self::DBAT1L => offset_of!(Cpu, supervisor.memory.dbat[1]),
            Self::DBAT2U => offset_of!(Cpu, supervisor.memory.dbat[2]) + 4,
            Self::DBAT2L => offset_of!(Cpu, supervisor.memory.dbat[2]),
            Self::DBAT3U => offset_of!(Cpu, supervisor.memory.dbat[3]) + 4,
            Self::DBAT3L => offset_of!(Cpu, supervisor.memory.dbat[3]),
            Self::GQR0 => offset_of!(Cpu, supervisor.gq[0]),
            Self::GQR1 => offset_of!(Cpu, supervisor.gq[1]),
            Self::GQR2 => offset_of!(Cpu, supervisor.gq[2]),
            Self::GQR3 => offset_of!(Cpu, supervisor.gq[3]),
            Self::GQR4 => offset_of!(Cpu, supervisor.gq[4]),
            Self::GQR5 => offset_of!(Cpu, supervisor.gq[5]),
            Self::GQR6 => offset_of!(Cpu, supervisor.gq[6]),
            Self::GQR7 => offset_of!(Cpu, supervisor.gq[7]),
            Self::HID2 => offset_of!(Cpu, supervisor.config.hid[2]),
            Self::WPAR => offset_of!(Cpu, supervisor.config.wpar),
            Self::DMAU => offset_of!(Cpu, supervisor.config.dma.upper),
            Self::DMAL => offset_of!(Cpu, supervisor.config.dma.lower),
            Self::MMCR0 => offset_of!(Cpu, supervisor.performance.control[0]),
            Self::PMC1 => offset_of!(Cpu, supervisor.performance.counters[0]),
            Self::PMC2 => offset_of!(Cpu, supervisor.performance.counters[1]),
            Self::MMCR1 => offset_of!(Cpu, supervisor.performance.control[1]),
            Self::PMC3 => offset_of!(Cpu, supervisor.performance.counters[2]),
            Self::PMC4 => offset_of!(Cpu, supervisor.performance.counters[3]),
            Self::HID0 => offset_of!(Cpu, supervisor.config.hid[0]),
            Self::HID1 => offset_of!(Cpu, supervisor.config.hid[1]),
            Self::L2CR => offset_of!(Cpu, supervisor.misc.l2cr),
        }
    }

    pub fn is_data_bat(&self) -> bool {
        matches!(
            self,
            Self::DBAT0U
                | Self::DBAT0L
                | Self::DBAT1U
                | Self::DBAT1L
                | Self::DBAT2U
                | Self::DBAT2L
                | Self::DBAT3U
                | Self::DBAT3L
        )
    }

    pub fn is_instr_bat(&self) -> bool {
        matches!(
            self,
            Self::IBAT0U
                | Self::IBAT0L
                | Self::IBAT1U
                | Self::IBAT1L
                | Self::IBAT2U
                | Self::IBAT2L
                | Self::IBAT3U
                | Self::IBAT3L
        )
    }

    pub fn is_bat(&self) -> bool {
        self.is_data_bat() || self.is_instr_bat()
    }

    pub fn is_gqr(&self) -> bool {
        matches!(
            self,
            Self::GQR0
                | Self::GQR1
                | Self::GQR2
                | Self::GQR3
                | Self::GQR4
                | Self::GQR5
                | Self::GQR6
                | Self::GQR7
        )
    }
}

/// A register in the Gekko CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg {
    GPR(GPR),
    FPR(FPR),
    SPR(SPR),
    PC,
    MSR,
    CR,
    FPSCR,
    SR0,
    SR1,
    SR2,
    SR3,
    SR4,
    SR5,
    SR6,
    SR7,
    SR8,
    SR9,
    SR10,
    SR11,
    SR12,
    SR13,
    SR14,
    SR15,
    TBL,
    TBU,
}

impl Reg {
    pub const SR: [Self; 16] = [
        Self::SR0,
        Self::SR1,
        Self::SR2,
        Self::SR3,
        Self::SR4,
        Self::SR5,
        Self::SR6,
        Self::SR7,
        Self::SR8,
        Self::SR9,
        Self::SR10,
        Self::SR11,
        Self::SR12,
        Self::SR13,
        Self::SR14,
        Self::SR15,
    ];

    /// Offset of this register in the [`Cpu`] struct.
    #[inline(always)]
    pub fn offset(self) -> usize {
        match self {
            Self::GPR(gpr) => gpr.offset(),
            Self::FPR(fpr) => fpr.offset(),
            Self::SPR(spr) => spr.offset(),
            Self::PC => offset_of!(Cpu, pc),
            Self::MSR => offset_of!(Cpu, supervisor.config.msr),
            Self::CR => offset_of!(Cpu, user.cr),
            Self::FPSCR => offset_of!(Cpu, user.fpscr),
            Self::SR0 => offset_of!(Cpu, supervisor.memory.sr[0]),
            Self::SR1 => offset_of!(Cpu, supervisor.memory.sr[1]),
            Self::SR2 => offset_of!(Cpu, supervisor.memory.sr[2]),
            Self::SR3 => offset_of!(Cpu, supervisor.memory.sr[3]),
            Self::SR4 => offset_of!(Cpu, supervisor.memory.sr[4]),
            Self::SR5 => offset_of!(Cpu, supervisor.memory.sr[5]),
            Self::SR6 => offset_of!(Cpu, supervisor.memory.sr[6]),
            Self::SR7 => offset_of!(Cpu, supervisor.memory.sr[7]),
            Self::SR8 => offset_of!(Cpu, supervisor.memory.sr[8]),
            Self::SR9 => offset_of!(Cpu, supervisor.memory.sr[9]),
            Self::SR10 => offset_of!(Cpu, supervisor.memory.sr[10]),
            Self::SR11 => offset_of!(Cpu, supervisor.memory.sr[11]),
            Self::SR12 => offset_of!(Cpu, supervisor.memory.sr[12]),
            Self::SR13 => offset_of!(Cpu, supervisor.memory.sr[13]),
            Self::SR14 => offset_of!(Cpu, supervisor.memory.sr[14]),
            Self::SR15 => offset_of!(Cpu, supervisor.memory.sr[15]),
            Self::TBL => offset_of!(Cpu, supervisor.misc.tb),
            Self::TBU => offset_of!(Cpu, supervisor.misc.tb) + 4,
        }
    }
}

impl From<GPR> for Reg {
    #[inline(always)]
    fn from(value: GPR) -> Self {
        Self::GPR(value)
    }
}

impl From<FPR> for Reg {
    #[inline(always)]
    fn from(value: FPR) -> Self {
        Self::FPR(value)
    }
}

impl From<SPR> for Reg {
    #[inline(always)]
    fn from(value: SPR) -> Self {
        Self::SPR(value)
    }
}
