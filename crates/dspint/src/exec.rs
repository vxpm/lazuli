use bitos::BitUtils;
use lazuli::system::System;

use crate::ins::CondCode;
use crate::{Acc40, Ins, Interpreter, Reg, Registers, Status};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MultiplyMode {
    Unsigned,
    Mixed,
    Signed,
}

#[inline(always)]
fn add_carried(lhs: i64, new: i64) -> bool {
    lhs as u64 > new as u64
}

#[inline(always)]
fn sub_carried(lhs: i64, new: i64) -> bool {
    lhs as u64 >= new as u64
}

#[inline(always)]
fn add_overflowed(lhs: i64, rhs: i64, new: i64) -> bool {
    (lhs > 0 && rhs > 0 && new <= 0) || (lhs < 0 && rhs < 0 && new >= 0)
}

#[inline(always)]
fn sub_overflowed(lhs: i64, rhs: i64, new: i64) -> bool {
    add_overflowed(lhs, -rhs, new)
}

/// Rounds a fixed point 24.16 number with ties to even.
#[inline(always)]
fn round_40_ties_to_even(value: i64) -> i64 {
    let half = 0x8000;

    // is the value odd?
    if value.bit(16) {
        // yes - add half, because fract 0.5 should round up (to even)
        (value + half) & !0xFFFF
    } else {
        // no - add (half - 1), because fract 0.5 should round down (to even)
        (value + half - 1) & !0xFFFF
    }
}

fn add_to_addr_reg(ar: u16, wr: u16, value: i16) -> u16 {
    // following algorithm was created by @hrydgard, version implemented here was refined and
    // described by @calc84maniac, and @zaydlang helped me understand - thanks!!

    // compute amount of significant bits in wr, minimum 1
    let n = (16 - wr.leading_zeros()).max(1);

    // create a mask of n bits
    let mask = 1u16.checked_shl(n).map_or(!0, |r| r - 1);

    // compute the carry out of bit n
    let carry = ((ar & mask) as u32 + (value as u16 & mask) as u32) > mask as u32;

    // compute result
    let mut result = ar.wrapping_add_signed(value);

    if value >= 0 {
        if carry {
            result = result.wrapping_sub(wr.wrapping_add(1));
        }
    } else {
        let low_sum = result & mask;
        let low_not_wrap = (!wr) & mask;
        let carry_again = low_sum < low_not_wrap;

        if !carry || carry_again {
            result = result.wrapping_add(wr.wrapping_add(1));
        }
    }

    result
}

fn sub_from_addr_reg(ar: u16, wr: u16, value: i16) -> u16 {
    // following algorithm was created by @hrydgard, version implemented here was refined and
    // described by @calc84maniac, and @zaydlang helped me understand - thanks!!

    // subtraction uses the one's complement
    let value = !value;

    // compute amount of significant bits in wr, minimum 1
    let n = (16 - wr.leading_zeros()).max(1);

    // create a mask of n bits
    let mask = 1u16.checked_shl(n).map_or(!0, |r| r - 1);

    // compute the carry out of bit n
    let carry = ((ar & mask) as u32 + (value as u16 & mask) as u32 + 1) > mask as u32;

    // compute result
    let mut result = ar.wrapping_add_signed(value).wrapping_add(1);
    if (value.wrapping_add(1)) > 0 || value.wrapping_add(1) == -0x8000 {
        if carry {
            result = result.wrapping_sub(wr.wrapping_add(1));
        }
    } else {
        let low_sum = result & mask;
        let low_not_wrap = (!wr) & mask;
        let carry_again = low_sum < low_not_wrap;

        if !carry || carry_again {
            result = result.wrapping_add(wr.wrapping_add(1));
        }
    }

    result
}

impl Interpreter {
    fn base_flags(&mut self, value: i64) {
        self.regs.status.set_sign(value < 0);
        self.regs.status.set_arithmetic_zero(value == 0);
        self.regs
            .status
            .set_above_s32(value > i32::MAX as i64 || value < i32::MIN as i64);
        self.regs
            .status
            .set_top_two_bits_eq(value.bit(30) == value.bit(31));
        self.regs
            .status
            .set_overflow_fused(self.regs.status.overflow() || self.regs.status.overflow_fused());
    }

    pub fn abs(&mut self, _: &mut System, ins: Ins) {
        let idx = ins.base.bit(11) as usize;
        let old = self.regs.acc40[idx].get();
        let new = self.regs.acc40[idx].set(old.abs());

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(new == Acc40::MIN);

        self.base_flags(new);
    }

    pub fn add(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = self.regs.acc40[1 - d].get();
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn addarn(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 2) as usize;
        let s = ins.base.bits(2, 4) as usize;

        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];
        let ix = self.regs.indexing[s];

        self.regs.addressing[d] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn addax(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = self.regs.acc32[s] as i64;
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn addaxl(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = self.regs.acc32[s].bits(0, 16) as u64 as i64;
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn addi(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = (ins.extra as i16 as i64) << 16;
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn addis(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = (ins.base.bits(0, 8) as i8 as i64) << 16;
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn addp(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let (carry, overflow, rhs) = self.regs.product.get();
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new) || carry);
        self.regs
            .status
            .set_overflow(add_overflowed(lhs, rhs, new) ^ overflow);

        self.base_flags(new);
    }

    pub fn addpaxz(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let (carry, overflow, lhs) = self.regs.product.get();
        let lhs = round_40_ties_to_even(lhs);

        let rhs = self.regs.acc32[s] as i64;
        let new = self.regs.acc40[d].set((lhs + rhs) & !0xFFFF);

        self.regs.status.set_carry(add_carried(lhs, new) ^ carry);
        self.regs
            .status
            .set_overflow(add_overflowed(lhs, rhs, new) ^ overflow);

        self.base_flags(new);
    }

    pub fn addr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bits(9, 11) as u8;

        let lhs = self.regs.acc40[d].get();
        let rhs = (self.regs.get(Reg::new(s + 0x18)) as i16 as i64) << 16;
        let new = self.regs.acc40[d].set(lhs + rhs);

        self.regs.status.set_carry(add_carried(lhs, new));
        self.regs.status.set_overflow(add_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn andc(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid &= self.regs.acc40[1 - d].mid;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn andcf(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let is_equal = self.regs.acc40[d].mid & ins.extra == ins.extra;
        self.regs.status.set_logic_zero(is_equal);
    }

    pub fn andf(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let is_equal = self.regs.acc40[d].mid & ins.extra == 0;
        self.regs.status.set_logic_zero(is_equal);
    }

    pub fn andi(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid &= ins.extra;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn andr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        self.regs.acc40[d].mid &= (self.regs.acc32[s] >> 16) as u16;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn asl(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let imm = ins.base.bits(0, 6) as u8;

        let lhs = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(lhs << imm);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn asr(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let imm = ins.base.bits(0, 6);

        let lhs = self.regs.acc40[r].get();
        let rhs = (64 - imm) % 64;
        let new = self.regs.acc40[r].set(lhs >> rhs);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn asrn(&mut self, _: &mut System, _: Ins) {
        let lhs = self.regs.acc40[0].get();
        let signed_shift = self.regs.acc40[1].mid;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[0].set(lhs << rhs)
        } else {
            self.regs.acc40[0].set(lhs >> rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn asrnr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let signed_shift = self.regs.acc40[1 - d].mid;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[d].set(lhs >> rhs)
        } else {
            self.regs.acc40[d].set(lhs << rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn asrnrx(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[d].get();
        let signed_shift = self.regs.acc32[s] >> 16;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[d].set(lhs >> rhs)
        } else {
            self.regs.acc40[d].set(lhs << rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn asr16(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(11) as usize;

        let old = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(old >> 16);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn clr15(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_unsigned_mul(false);
    }

    pub fn clr(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(11) as usize;

        let new = self.regs.acc40[r].set(0);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn clrl(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;

        let old = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(round_40_ties_to_even(old));

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn clrp(&mut self, _: &mut System, _: Ins) {
        self.regs.product.low = 0x0000;
        self.regs.product.mid1 = 0xFFF0;
        self.regs.product.mid2 = 0x0010;
        self.regs.product.high = 0x00FF;
    }

    pub fn cmp(&mut self, _: &mut System, _: Ins) {
        let lhs = self.regs.acc40[0].get();
        let rhs = self.regs.acc40[1].get();
        let diff = Acc40::from(lhs - rhs).get();

        self.regs.status.set_carry(sub_carried(lhs, diff));
        self.regs
            .status
            .set_overflow(sub_overflowed(lhs, rhs, diff));

        self.base_flags(diff);
    }

    pub fn cmpaxh(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bit(11) as usize;
        let r = ins.base.bit(12) as usize;

        let lhs = self.regs.acc40[s].get();
        let rhs = ((self.regs.acc32[r] as i64) >> 16) << 16;
        let diff = Acc40::from(lhs - rhs).get();

        self.regs.status.set_carry(sub_carried(lhs, diff));
        self.regs
            .status
            .set_overflow(sub_overflowed(lhs, rhs, diff));

        self.base_flags(diff);
    }

    pub fn cmpi(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = (ins.extra as i16 as i64) << 16;
        let diff = Acc40::from(lhs - rhs).get();

        self.regs.status.set_carry(sub_carried(lhs, diff));
        self.regs
            .status
            .set_overflow(sub_overflowed(lhs, rhs, diff));

        self.base_flags(diff);
    }

    pub fn cmpis(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = (ins.base as i8 as i64) << 16;
        let diff = Acc40::from(lhs - rhs).get();

        self.regs.status.set_carry(sub_carried(lhs, diff));
        self.regs
            .status
            .set_overflow(sub_overflowed(lhs, rhs, diff));

        self.base_flags(diff);
    }

    pub fn dar(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 2) as usize;

        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];

        self.regs.addressing[d] = sub_from_addr_reg(ar, wr, 1i16);
    }

    pub fn dec(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let old = self.regs.acc40[d].get();
        let new = self.regs.acc40[d].set(old.wrapping_sub(1));

        self.regs.status.set_carry(sub_carried(old, new));
        self.regs.status.set_overflow(add_overflowed(old, -1, new));

        self.base_flags(new);
    }

    pub fn decm(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let old = self.regs.acc40[d].get();
        let new = self.regs.acc40[d].set(old - (1 << 16));

        self.regs.status.set_carry(sub_carried(old, new));
        self.regs
            .status
            .set_overflow(add_overflowed(old, -(1 << 16), new));

        self.base_flags(new);
    }

    pub fn halt(&mut self, sys: &mut System, _: Ins) {
        sys.dsp.control.set_halt(true);
    }

    pub fn iar(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bits(0, 2) as usize;

        let ar = self.regs.addressing[r];
        let wr = self.regs.wrapping[r];

        self.regs.addressing[r] = add_to_addr_reg(ar, wr, 1);
    }

    #[inline(always)]
    fn condition(&self, code: CondCode) -> bool {
        let status = self.regs.status;
        match code {
            CondCode::GreaterOrEqual => status.overflow() == status.sign(),
            CondCode::Less => status.overflow() != status.sign(),
            CondCode::Greater => status.overflow() == status.sign() && !status.arithmetic_zero(),
            CondCode::LessOrEqual => status.overflow() != status.sign() || status.arithmetic_zero(),
            CondCode::NotZero => !status.arithmetic_zero(),
            CondCode::Zero => status.arithmetic_zero(),
            CondCode::NotCarry => !status.carry(),
            CondCode::Carry => status.carry(),
            CondCode::BelowS32 => !status.above_s32(),
            CondCode::AboveS32 => status.above_s32(),
            CondCode::WeirdA => {
                (status.above_s32() || status.top_two_bits_eq()) && !status.arithmetic_zero()
            }
            CondCode::WeirdB => {
                (!status.above_s32() && !status.top_two_bits_eq()) || status.arithmetic_zero()
            }
            CondCode::NotLogicZero => !status.logic_zero(),
            CondCode::LogicZero => status.logic_zero(),
            CondCode::Overflow => status.overflow(),
            CondCode::Always => true,
        }
    }

    pub fn ifcc(&mut self, _: &mut System, ins: Ins) {
        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        if !self.condition(code) {
            self.pc += 1;
        }
    }

    pub fn inc(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let old = self.regs.acc40[d].get();
        let new = self.regs.acc40[d].set(old.wrapping_add(1));

        self.regs.status.set_carry(add_carried(old, new));
        self.regs.status.set_overflow(add_overflowed(old, 1, new));

        self.base_flags(new);
    }

    pub fn incm(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let old = self.regs.acc40[d].get();
        let new = self.regs.acc40[d].set(old + (1 << 16));

        self.regs.status.set_carry(add_carried(old, new));
        self.regs
            .status
            .set_overflow(add_overflowed(old, 1 << 16, new));

        self.base_flags(new);
    }

    pub fn lsl(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let shift = ins.base.bits(0, 6);

        let old = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(old << shift);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsl16(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;

        let old = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(old << 16);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsr(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let shift = ins.base.bits(0, 6);

        let lhs = (self.regs.acc40[r].get() as u64) & ((1 << 40) - 1);
        let rhs = (64 - shift) % 64;
        let new = self.regs.acc40[r].set((lhs >> rhs) as i64);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsrn(&mut self, _: &mut System, _: Ins) {
        let lhs = (self.regs.acc40[0].get()) & ((1 << 40) - 1);
        let signed_shift = self.regs.acc40[1].mid;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[0].set(lhs << rhs)
        } else {
            self.regs.acc40[0].set(lhs >> rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsrnr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = (self.regs.acc40[d].get()) & ((1 << 40) - 1);
        let signed_shift = self.regs.acc40[1 - d].mid;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[d].set(lhs >> rhs)
        } else {
            self.regs.acc40[d].set(lhs << rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsrnrx(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = (self.regs.acc40[d].get()) & ((1 << 40) - 1);
        let signed_shift = self.regs.acc32[s] >> 16;
        let rhs = signed_shift.bits(0, 6);

        let new = if signed_shift.bit(6) {
            let rhs = (64 - rhs) % 64;
            self.regs.acc40[d].set(lhs >> rhs)
        } else {
            self.regs.acc40[d].set(lhs << rhs)
        };

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn lsr16(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;

        let old = (self.regs.acc40[r].get() as u64) & ((1 << 40) - 1);
        let new = self.regs.acc40[r].set((old >> 16) as i64);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn m0(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_dont_double_result(true);
    }

    pub fn m2(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_dont_double_result(false);
    }

    // NOTE: carry flag issue
    pub fn madd(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bit(8) as usize;

        let acc = self.regs.acc32[s];
        let low = (acc << 16) >> 16;
        let high = acc >> 16;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod + result as i64);
    }

    // NOTE: carry flag issue
    pub fn maddc(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod + result);
    }

    // NOTE: carry flag issue
    pub fn maddx(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(8) as u8;
        let s = ins.base.bit(9) as u8;

        let lhs = self.regs.get(Reg::new(0x18 + 2 * s)) as i16 as i64;
        let rhs = self.regs.get(Reg::new(0x19 + 2 * t)) as i16 as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod + result);
    }

    pub fn mov(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let new = self.regs.acc40[d].set(self.regs.acc40[1 - d].get());

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn movax(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let new = self.regs.acc40[d].set(self.regs.acc32[s] as i64);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn movnp(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let (carry, overflow, prod) = self.regs.product.get();
        let new = self.regs.acc40[d].set(-prod);

        self.regs.status.set_carry((prod != 0) && !carry);
        self.regs.status.set_overflow(overflow);

        self.base_flags(new);
    }

    pub fn movp(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let (carry, overflow, prod) = self.regs.product.get();
        let new = self.regs.acc40[d].set(prod);

        self.regs.status.set_carry(carry);
        self.regs.status.set_overflow(overflow);

        self.base_flags(new);
    }

    // TODO: carry flag
    pub fn movpz(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let (carry, overflow, prod) = self.regs.product.get();
        let new = self.regs.acc40[d].set(round_40_ties_to_even(prod));

        self.regs.status.set_carry(carry);
        self.regs.status.set_overflow(overflow);

        self.base_flags(new);
    }

    pub fn movr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bits(9, 11) as u8;

        let lhs = self.regs.get(Reg::new(0x18 + s)) as i16 as i64;
        let new = self.regs.acc40[d].set(lhs << 16);

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);
    }

    pub fn mrr(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let d = ins.base.bits(5, 10) as u8;

        let src = self.regs.get(Reg::new(s));
        self.regs.set_saturate(Reg::new(d), src);
    }

    // NOTE: carry flag issue
    pub fn msub(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bit(8) as usize;

        let acc = self.regs.acc32[s];
        let low = (acc << 16) >> 16;
        let high = acc >> 16;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod - result as i64);
    }

    // NOTE: carry flag issue
    pub fn msubc(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod - result);
    }

    // NOTE: carry flag issue
    pub fn msubx(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(8) as u8;
        let s = ins.base.bit(9) as u8;

        let lhs = self.regs.get(Reg::new(0x18 + 2 * s)) as i16 as i64;
        let rhs = self.regs.get(Reg::new(0x19 + 2 * t)) as i16 as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        let (_, _, prod) = self.regs.product.get();
        self.regs.product.set(prod - result);
    }

    // NOTE: carry flag issue
    pub fn mul(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bit(11) as usize;

        let acc = self.regs.acc32[s];
        let low = ((acc << 16) >> 16) as i64;
        let high = (acc >> 16) as i64;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
    }

    // NOTE: carry flag issue
    pub fn mulac(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let s = ins.base.bit(11) as usize;

        let acc_r = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(acc_r + self.regs.product.get().2);

        let acc_s = self.regs.acc32[s];
        let low = ((acc_s << 16) >> 16) as i64;
        let high = (acc_s >> 16) as i64;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    // NOTE: carry flag issue
    pub fn mulaxh(&mut self, _: &mut System, _: Ins) {
        let val = (self.regs.acc32[0] >> 16) as i64;
        let mul = val * val;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
    }

    // NOTE: carry flag issue
    pub fn mulc(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(11) as usize;
        let s = ins.base.bit(12) as usize;

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
    }

    // NOTE: carry flag issue
    pub fn mulcac(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11) as usize;
        let s = ins.base.bit(12) as usize;

        let (_, _, prod) = self.regs.product.get();

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
        let acc_r = self.regs.acc40[r].get();
        let new = self.regs.acc40[r].set(acc_r + prod);

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    // NOTE: carry flag issue
    pub fn mulcmv(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11) as usize;
        let s = ins.base.bit(12) as usize;

        let (_, _, prod) = self.regs.product.get();

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
        let new = self.regs.acc40[r].set(prod);

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    // NOTE: carry flag issue
    pub fn mulcmvz(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11) as usize;
        let s = ins.base.bit(12) as usize;

        let (_, _, prod) = self.regs.product.get();

        let lhs = self.regs.acc40[s].mid as i16 as i64;
        let rhs = (self.regs.acc32[t] >> 16) as i64;
        let mul = lhs * rhs;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);
        let new = self.regs.acc40[r].set(round_40_ties_to_even(prod));

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    // NOTE: carry flag issue
    pub fn mulmv(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let s = ins.base.bit(11) as usize;

        let (_, _, prod) = self.regs.product.get();
        let new = self.regs.acc40[r].set(prod);

        let low = ((self.regs.acc32[s] << 16) >> 16) as i64;
        let high = (self.regs.acc32[s] >> 16) as i64;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    // NOTE: carry flag issue
    pub fn mulmvz(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let s = ins.base.bit(11) as usize;

        let (_, _, prod) = self.regs.product.get();
        let new = self.regs.acc40[r].set(round_40_ties_to_even(prod));

        let low = ((self.regs.acc32[s] << 16) >> 16) as i64;
        let high = (self.regs.acc32[s] >> 16) as i64;
        let mul = low * high;
        let result = if self.regs.status.dont_double_result() {
            mul
        } else {
            2 * mul
        };

        self.regs.product.set(result);

        self.regs.status.set_overflow(false);
        self.base_flags(new);
    }

    fn multiply(&self, mode: MultiplyMode, a: u16, b: u16) -> i64 {
        let factor = if self.regs.status.dont_double_result() {
            1
        } else {
            2
        };

        let (a, b) = if mode == MultiplyMode::Signed || !self.regs.status.unsigned_mul() {
            // sign ext, sign ext
            (a as i16 as i64, b as i16 as i64)
        } else if mode == MultiplyMode::Mixed {
            // zero ext, sign ext
            (a as u64 as i64, b as i16 as i64)
        } else {
            // zero ext, zero ext
            (a as u64 as i64, b as u64 as i64)
        };

        a * b * factor
    }

    pub fn mulx(&mut self, _: &mut System, ins: Ins) {
        let t = ins.base.bit(11);
        let s = ins.base.bit(12);

        let lhs = if s {
            (self.regs.acc32[0] >> 16) as u16
        } else {
            self.regs.acc32[0] as u16
        };

        let rhs = if t {
            (self.regs.acc32[1] >> 16) as u16
        } else {
            self.regs.acc32[1] as u16
        };

        let (mode, lhs, rhs) = match (s, t) {
            (false, false) => (MultiplyMode::Unsigned, lhs, rhs),
            (false, true) => (MultiplyMode::Mixed, lhs, rhs),
            (true, false) => (MultiplyMode::Mixed, rhs, lhs),
            (true, true) => (MultiplyMode::Signed, lhs, rhs),
        };

        let result = self.multiply(mode, lhs, rhs);
        self.regs.product.set(result);
    }

    pub fn mulxac(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11);
        let s = ins.base.bit(12);

        let (_, _, prod) = self.regs.product.get();
        let acc = self.regs.acc40[r].get();
        self.regs.acc40[r].set(acc + prod);

        let lhs = if s {
            (self.regs.acc32[0] >> 16) as u16
        } else {
            self.regs.acc32[0] as u16
        };

        let rhs = if t {
            (self.regs.acc32[1] >> 16) as u16
        } else {
            self.regs.acc32[1] as u16
        };

        let (mode, lhs, rhs) = match (s, t) {
            (false, false) => (MultiplyMode::Unsigned, lhs, rhs),
            (false, true) => (MultiplyMode::Mixed, lhs, rhs),
            (true, false) => (MultiplyMode::Mixed, rhs, lhs),
            (true, true) => (MultiplyMode::Signed, lhs, rhs),
        };

        let result = self.multiply(mode, lhs, rhs);
        self.regs.product.set(result);
    }

    pub fn mulxmv(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11);
        let s = ins.base.bit(12);

        let (_, _, prod) = self.regs.product.get();
        self.regs.acc40[r].set(prod);

        let lhs = if s {
            (self.regs.acc32[0] >> 16) as u16
        } else {
            self.regs.acc32[0] as u16
        };

        let rhs = if t {
            (self.regs.acc32[1] >> 16) as u16
        } else {
            self.regs.acc32[1] as u16
        };

        let (mode, lhs, rhs) = match (s, t) {
            (false, false) => (MultiplyMode::Unsigned, lhs, rhs),
            (false, true) => (MultiplyMode::Mixed, lhs, rhs),
            (true, false) => (MultiplyMode::Mixed, rhs, lhs),
            (true, true) => (MultiplyMode::Signed, lhs, rhs),
        };

        let result = self.multiply(mode, lhs, rhs);
        self.regs.product.set(result);
    }

    pub fn mulxmvz(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;
        let t = ins.base.bit(11);
        let s = ins.base.bit(12);

        let (_, _, prod) = self.regs.product.get();
        self.regs.acc40[r].set(round_40_ties_to_even(prod));

        let lhs = if s {
            (self.regs.acc32[0] >> 16) as u16
        } else {
            self.regs.acc32[0] as u16
        };

        let rhs = if t {
            (self.regs.acc32[1] >> 16) as u16
        } else {
            self.regs.acc32[1] as u16
        };

        let (mode, lhs, rhs) = match (s, t) {
            (false, false) => (MultiplyMode::Unsigned, lhs, rhs),
            (false, true) => (MultiplyMode::Mixed, lhs, rhs),
            (true, false) => (MultiplyMode::Mixed, rhs, lhs),
            (true, true) => (MultiplyMode::Signed, lhs, rhs),
        };

        let result = self.multiply(mode, lhs, rhs);
        self.regs.product.set(result);
    }

    pub fn neg(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let old = self.regs.acc40[d].get();
        let new = self.regs.acc40[d].set(-old);

        self.regs.status.set_carry(old == 0);
        self.regs.status.set_overflow(old == (1 << 40));

        self.base_flags(new);
    }

    pub fn not(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid ^= 0xFFFF;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn orc(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid |= self.regs.acc40[1 - d].mid;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn ori(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid |= ins.extra;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn orr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        self.regs.acc40[d].mid |= (self.regs.acc32[s] >> 16) as u16;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn sbclr(&mut self, _: &mut System, ins: Ins) {
        let i = ins.base.bits(0, 3) as u8;

        let idx = 6 + i;
        let old = self.regs.status.to_bits();
        let new = if idx == 13 {
            old
        } else {
            old.with_bit(idx, false)
        };

        self.regs.status = Status::from_bits(new);
    }

    pub fn sbset(&mut self, _: &mut System, ins: Ins) {
        let i = ins.base.bits(0, 3) as u8;

        let idx = 6 + i;
        let old = self.regs.status.to_bits();
        let new = if idx == 13 || idx == 8 {
            old
        } else {
            old.with_bit(idx, true)
        };

        self.regs.status = Status::from_bits(new);
    }

    pub fn set15(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_unsigned_mul(true);
    }

    pub fn set16(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_sign_extend_to_40(false);
    }

    pub fn set40(&mut self, _: &mut System, _: Ins) {
        self.regs.status.set_sign_extend_to_40(true);
    }

    pub fn sub(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = self.regs.acc40[1 - d].get();
        let new = self.regs.acc40[d].set(lhs - rhs);

        self.regs.status.set_carry(sub_carried(lhs, new));
        self.regs.status.set_overflow(sub_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn subarn(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 2) as usize;

        let ix = self.regs.indexing[d];
        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];

        self.regs.addressing[d] = sub_from_addr_reg(ar, wr, ix as i16);
    }

    pub fn subax(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        let lhs = self.regs.acc40[d].get();
        let rhs = self.regs.acc32[s] as i64;
        let new = self.regs.acc40[d].set(lhs - rhs);

        self.regs.status.set_carry(sub_carried(lhs, new));
        self.regs.status.set_overflow(sub_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn subp(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        let lhs = self.regs.acc40[d].get();
        let (carry, overflow, rhs) = self.regs.product.get();
        let new = self.regs.acc40[d].set(lhs - rhs);

        self.regs.status.set_carry(sub_carried(lhs, new) ^ !carry);
        self.regs
            .status
            .set_overflow(sub_overflowed(lhs, rhs, new) ^ overflow);

        self.base_flags(new);
    }

    pub fn subr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bits(9, 11) as u8;

        let lhs = self.regs.acc40[d].get();
        let rhs = (self.regs.get(Reg::new(s + 0x18)) as i16 as i64) << 16;
        let new = self.regs.acc40[d].set(lhs - rhs);

        self.regs.status.set_carry(sub_carried(lhs, new));
        self.regs.status.set_overflow(sub_overflowed(lhs, rhs, new));

        self.base_flags(new);
    }

    pub fn tst(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(11) as usize;

        let acc = self.regs.acc40[r].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(acc);
    }

    pub fn tstaxh(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bit(8) as usize;

        let acc = self.regs.acc32[r] >> 16;

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(acc as i64);

        self.regs
            .status
            .set_top_two_bits_eq(acc.bit(15) == acc.bit(14));
    }

    pub fn tstprod(&mut self, _: &mut System, _: Ins) {
        let (carry, overflow, prod) = self.regs.product.get();

        self.regs.status.set_carry(carry);
        self.regs.status.set_overflow(overflow);

        self.base_flags(prod);
    }

    pub fn xorc(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid ^= self.regs.acc40[1 - d].mid;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn xori(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;

        self.regs.acc40[d].mid ^= ins.extra;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn xorr(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bit(8) as usize;
        let s = ins.base.bit(9) as usize;

        self.regs.acc40[d].mid ^= (self.regs.acc32[s] >> 16) as u16;
        let new = self.regs.acc40[d].get();

        self.regs.status.set_carry(false);
        self.regs.status.set_overflow(false);

        self.base_flags(new);

        self.regs
            .status
            .set_arithmetic_zero(self.regs.acc40[d].mid == 0);
        self.regs
            .status
            .set_sign((self.regs.acc40[d].mid as i16) < 0);
    }

    pub fn bloop(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bits(0, 5) as u8;

        let counter = self.regs.get(Reg::new(r));

        if counter != 0 {
            self.regs.call_stack.push(self.pc.wrapping_add(2));
            self.regs.loop_stack.push(ins.extra);
            self.regs.loop_count.push(counter);
        } else {
            self.pc = (ins.extra + 1) - 2;
        }
    }

    pub fn bloopi(&mut self, _: &mut System, ins: Ins) {
        let counter = ins.base.bits(0, 8);

        if counter != 0 {
            self.regs.call_stack.push(self.pc.wrapping_add(2));
            self.regs.loop_stack.push(ins.extra);
            self.regs.loop_count.push(counter);
        } else {
            panic!("what the fuck?")
        }
    }

    pub fn call(&mut self, _: &mut System, ins: Ins) {
        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        if self.condition(code) {
            self.regs.call_stack.push(self.pc.wrapping_add(2));
            self.pc = ins.extra - 2;
        }
    }

    pub fn callr(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bits(5, 8) as u8;

        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        let addr = self.regs.get(Reg::new(r));

        if self.condition(code) {
            self.regs.call_stack.push(self.pc.wrapping_add(1));
            self.pc = addr - 1;
        }
    }

    pub fn jmp(&mut self, _: &mut System, ins: Ins) {
        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        if self.condition(code) {
            self.pc = ins.extra - 2;
        }
    }

    pub fn jmpr(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bits(5, 8) as u8;

        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        let addr = self.regs.get(Reg::new(r));

        if self.condition(code) {
            self.pc = addr.wrapping_sub(1);
        }
    }

    pub fn ret(&mut self, _: &mut System, ins: Ins) {
        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        if self.condition(code) {
            let addr = self.regs.call_stack.pop().unwrap();
            self.pc = addr - 1;
        }
    }

    pub fn lr(&mut self, sys: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        let data = self.read_dmem(sys, ins.extra);
        self.regs.set_saturate(Reg::new(d), data);
    }

    pub fn lri(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        self.regs.set_saturate(Reg::new(d), ins.extra);
    }

    pub fn lris(&mut self, _: &mut System, ins: Ins) {
        let d = ins.base.bits(8, 11) as u8;
        let imm = ins.base.bits(0, 8) as i8 as i16;
        self.regs.set_saturate(Reg::new(0x18 + d), imm as u16);
    }

    pub fn lrr(&mut self, sys: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        let s = ins.base.bits(5, 7) as usize;

        let ar = self.regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(Reg::new(d), data);
    }

    pub fn lrrd(&mut self, sys: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        let s = ins.base.bits(5, 7) as usize;

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        let data = self.read_dmem(sys, ar);
        self.regs.addressing[s] = sub_from_addr_reg(ar, wr, 1);

        self.regs.set_saturate(Reg::new(d), data);
    }

    pub fn lrri(&mut self, sys: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        let s = ins.base.bits(5, 7) as usize;

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        let data = self.read_dmem(sys, ar);
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);

        self.regs.set_saturate(Reg::new(d), data);
    }

    pub fn lrrn(&mut self, sys: &mut System, ins: Ins) {
        let d = ins.base.bits(0, 5) as u8;
        let s = ins.base.bits(5, 7) as usize;

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        let ix = self.regs.indexing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);

        self.regs.set_saturate(Reg::new(d), data);
    }

    pub fn lrs(&mut self, sys: &mut System, ins: Ins) {
        let imm = ins.base.bits(0, 8) as u8;
        let d = ins.base.bits(8, 11) as u8;

        let addr = u16::from_le_bytes([imm, self.regs.config]);
        let data = self.read_dmem(sys, addr);
        self.regs.set_saturate(Reg::new(0x18 + d), data);
    }

    pub fn ilrr(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bit(8);

        let reg = if d { Reg::Acc40Mid1 } else { Reg::Acc40Mid0 };
        let addr = self.regs.addressing[s];
        let data = self.read_imem(addr);

        self.regs.set_saturate(reg, data);
    }

    pub fn ilrrd(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bit(8);

        let reg = if d { Reg::Acc40Mid1 } else { Reg::Acc40Mid0 };
        let ar = self.regs.addressing[s];
        let data = self.read_imem(ar);
        self.regs.set_saturate(reg, data);

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        self.regs.addressing[s] = sub_from_addr_reg(ar, wr, 1);
    }

    pub fn ilrri(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bit(8);

        let reg = if d { Reg::Acc40Mid1 } else { Reg::Acc40Mid0 };
        let ar = self.regs.addressing[s];
        let data = self.read_imem(ar);
        self.regs.set_saturate(reg, data);

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ilrrn(&mut self, _: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bit(8);

        let reg = if d { Reg::Acc40Mid1 } else { Reg::Acc40Mid0 };
        let ar = self.regs.addressing[s];
        let data = self.read_imem(ar);
        self.regs.set_saturate(reg, data);

        let ar = self.regs.addressing[s];
        let wr = self.regs.wrapping[s];
        let ix = self.regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn si(&mut self, sys: &mut System, ins: Ins) {
        let offset = ins.base.bits(0, 8) as u8;
        let addr = u16::from_le_bytes([offset, 0xFF]);
        self.write_dmem(sys, addr, ins.extra);
    }

    pub fn sr(&mut self, sys: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let data = self.regs.get(Reg::new(s));
        self.write_dmem(sys, ins.extra, data);
    }

    pub fn srr(&mut self, sys: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let d = ins.base.bits(5, 7) as usize;

        let data = self.regs.get(Reg::new(s));
        let addr = self.regs.addressing[d];
        self.write_dmem(sys, addr, data);
    }

    pub fn srrd(&mut self, sys: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let d = ins.base.bits(5, 7) as usize;

        let data = self.regs.get(Reg::new(s));
        let ar = self.regs.addressing[d];
        self.write_dmem(sys, ar, data);

        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];
        self.regs.addressing[d] = sub_from_addr_reg(ar, wr, 1);
    }

    pub fn srri(&mut self, sys: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let d = ins.base.bits(5, 7) as usize;

        let data = self.regs.get(Reg::new(s));
        let ar = self.regs.addressing[d];
        self.write_dmem(sys, ar, data);

        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];
        self.regs.addressing[d] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn srrn(&mut self, sys: &mut System, ins: Ins) {
        let s = ins.base.bits(0, 5) as u8;
        let d = ins.base.bits(5, 7) as usize;

        let data = self.regs.get(Reg::new(s));
        let ar = self.regs.addressing[d];
        self.write_dmem(sys, ar, data);

        let ar = self.regs.addressing[d];
        let wr = self.regs.wrapping[d];
        let ix = self.regs.indexing[d];
        self.regs.addressing[d] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn srs(&mut self, sys: &mut System, ins: Ins) {
        let imm = ins.base.bits(0, 8) as u8;
        let s = ins.base.bits(8, 10) as u8;

        let addr = u16::from_le_bytes([imm, self.regs.config]);
        let data = self.regs.get(Reg::new(0x1C + s));
        self.write_dmem(sys, addr, data);
    }

    pub fn srsh(&mut self, sys: &mut System, ins: Ins) {
        let imm = ins.base.bits(0, 8) as u8;
        let s = ins.base.bit(8) as usize;

        let addr = u16::from_le_bytes([imm, self.regs.config]);
        let data = self.regs.acc40[s].high as i8 as i16 as u16;
        self.write_dmem(sys, addr, data);
    }

    pub fn loop_(&mut self, _: &mut System, ins: Ins) {
        let r = ins.base.bits(0, 5) as u8;

        let counter = self.regs.get(Reg::new(r));

        if counter != 0 {
            self.regs.call_stack.push(self.pc.wrapping_add(1));
            self.regs.loop_stack.push(self.pc.wrapping_add(1));
            self.regs.loop_count.push(counter);
        } else {
            self.pc += 1;
        }
    }

    pub fn loopi(&mut self, _: &mut System, ins: Ins) {
        let imm = ins.base.bits(0, 8) as u8;

        let counter = imm as u16;

        if counter != 0 {
            self.regs.call_stack.push(self.pc.wrapping_add(1));
            self.regs.loop_stack.push(self.pc.wrapping_add(1));
            self.regs.loop_count.push(counter);
        } else {
            self.pc += 1;
        }
    }

    pub fn rti(&mut self, _: &mut System, ins: Ins) {
        let code = CondCode::new(ins.base.bits(0, 4) as u8);
        if self.condition(code) {
            let sr = self.regs.data_stack.pop().unwrap();
            let pc = self.regs.call_stack.pop().unwrap();
            self.regs.set(Reg::Status, sr);
            self.pc = pc - 1;
        }
    }
}

impl Interpreter {
    pub fn ext_dr(&mut self, _: &mut System, ins: Ins, regs: &Registers) {
        let r = ins.base.bits(0, 2) as usize;

        let ar = regs.addressing[r];
        let wr = regs.wrapping[r];

        self.regs.addressing[r] = sub_from_addr_reg(ar, wr, 1i16);
    }

    pub fn ext_ir(&mut self, _: &mut System, ins: Ins, regs: &Registers) {
        let r = ins.base.bits(0, 2) as usize;

        let ar = regs.addressing[r];
        let wr = regs.wrapping[r];

        self.regs.addressing[r] = add_to_addr_reg(ar, wr, 1i16);
    }

    pub fn ext_nr(&mut self, _: &mut System, ins: Ins, regs: &Registers) {
        let r = ins.base.bits(0, 2) as usize;

        let ar = regs.addressing[r];
        let wr = regs.wrapping[r];
        let ir = regs.indexing[r];

        self.regs.addressing[r] = add_to_addr_reg(ar, wr, ir as i16);
    }

    pub fn ext_mv(&mut self, _: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as u8;
        let d = ins.base.bits(2, 4) as u8;

        self.regs
            .set(Reg::new(0x18 + d), regs.get_pure(Reg::new(0x1C + s)));
    }

    pub fn ext_l(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bits(3, 6) as u8;

        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(Reg::new(0x18 + d), data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_ln(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        let d = ins.base.bits(3, 6) as u8;

        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(Reg::new(0x18 + d), data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        let ix = regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ld(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        if s == 3 {
            self.ext_ldax(sys, ins, regs);
            return;
        }

        let r = ins.base.bit(4);
        let d = ins.base.bit(5);

        let d = if d { Reg::Acc32High0 } else { Reg::Acc32Low0 };
        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(d, data);

        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let r = if r { Reg::Acc32High1 } else { Reg::Acc32Low1 };
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(r, data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_ldax(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(5) as usize;
        let r = ins.base.bit(4) as usize;

        let ar = regs.addressing[s];
        let high = self.read_dmem(sys, ar);
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let low = self.read_dmem(sys, ar);

        self.regs.acc32[r] = (((high as u32) << 16) | low as u32) as i32;

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_ldm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        if s == 3 {
            self.ext_ldaxm(sys, ins, regs);
            return;
        }

        let r = ins.base.bit(4);
        let d = ins.base.bit(5);

        let d = if d { Reg::Acc32High0 } else { Reg::Acc32Low0 };
        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(d, data);

        let r = if r { Reg::Acc32High1 } else { Reg::Acc32Low1 };
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(r, data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ldaxm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(5) as usize;
        let r = ins.base.bit(4) as usize;

        let ar = regs.addressing[s];
        let high = self.read_dmem(sys, ar);
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let low = self.read_dmem(sys, ar);

        self.regs.acc32[r] = (((high as u32) << 16) | low as u32) as i32;

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ldnm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        if s == 3 {
            self.ext_ldaxnm(sys, ins, regs);
            return;
        }

        let r = ins.base.bit(4);
        let d = ins.base.bit(5);

        let d = if d { Reg::Acc32High0 } else { Reg::Acc32Low0 };
        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(d, data);

        let r = if r { Reg::Acc32High1 } else { Reg::Acc32Low1 };
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(r, data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        let ix = regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ldaxnm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(5) as usize;
        let r = ins.base.bit(4) as usize;

        let ar = regs.addressing[s];
        let high = self.read_dmem(sys, ar);
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let low = self.read_dmem(sys, ar);

        self.regs.acc32[r] = (((high as u32) << 16) | low as u32) as i32;

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        let ix = regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ldn(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bits(0, 2) as usize;
        if s == 3 {
            self.ext_ldaxn(sys, ins, regs);
            return;
        }

        let r = ins.base.bit(4);
        let d = ins.base.bit(5);

        let d = if d { Reg::Acc32High0 } else { Reg::Acc32Low0 };
        let ar = regs.addressing[s];
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(d, data);

        let r = if r { Reg::Acc32High1 } else { Reg::Acc32Low1 };
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let data = self.read_dmem(sys, ar);
        self.regs.set_saturate(r, data);

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        let ix = regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_ldaxn(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(5) as usize;
        let r = ins.base.bit(4) as usize;

        let ar = regs.addressing[s];
        let high = self.read_dmem(sys, ar);
        let ar = if (regs.addressing[3] >> 10) == (regs.addressing[s] >> 10) {
            regs.addressing[s]
        } else {
            regs.addressing[3]
        };
        let low = self.read_dmem(sys, ar);

        self.regs.acc32[r] = (((high as u32) << 16) | low as u32) as i32;

        let ar = regs.addressing[s];
        let wr = regs.wrapping[s];
        let ix = regs.indexing[s];
        self.regs.addressing[s] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_s(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let d = ins.base.bits(0, 2) as usize;
        let s = ins.base.bits(3, 5) as u8;

        let ar = regs.addressing[d];
        let data = regs.get_pure(Reg::new(0x1C + s));
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[d];
        let wr = regs.wrapping[d];
        self.regs.addressing[d] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_sn(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let d = ins.base.bits(0, 2) as usize;
        let s = ins.base.bits(3, 5) as u8;

        let ar = regs.addressing[d];
        let data = regs.get_pure(Reg::new(0x1C + s));
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[d];
        let wr = regs.wrapping[d];
        let ix = regs.indexing[d];
        self.regs.addressing[d] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_ls(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[3];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_lsm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[3];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_lsnm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[3];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        let ix = regs.indexing[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_lsn(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[3];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        let ix = regs.indexing[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_sl(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[3];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }

    pub fn ext_slm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[3];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, 1);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_slnm(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[3];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        let ix = regs.indexing[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        let ix = regs.indexing[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, ix as i16);
    }

    pub fn ext_sln(&mut self, sys: &mut System, ins: Ins, regs: &Registers) {
        let s = ins.base.bit(0) as usize;
        let d = ins.base.bits(4, 6) as u8;

        let ar = regs.addressing[0];
        let data = regs.acc40[s].mid;
        self.write_dmem(sys, ar, data);

        let ar = regs.addressing[3];
        let data = self.read_dmem(sys, ar);
        self.regs.set(Reg::new(0x18 + d), data);

        let ar = regs.addressing[0];
        let wr = regs.wrapping[0];
        let ix = regs.indexing[0];
        self.regs.addressing[0] = add_to_addr_reg(ar, wr, ix as i16);

        let ar = regs.addressing[3];
        let wr = regs.wrapping[3];
        self.regs.addressing[3] = add_to_addr_reg(ar, wr, 1);
    }
}
