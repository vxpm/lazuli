use cranelift::codegen::ir;
use cranelift::prelude::{FloatCC, FunctionBuilder, InstBuilder, IntCC};
use gekko::disasm::{Ins, ParsedIns};
use gekko::{Reg, SPR};
use zerocopy::IntoBytes;

use super::{Action, BlockBuilder};
use crate::builder::InstructionInfo;

/// Trait for transforming values into an IR value in a function.
pub trait IntoIrValue {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value;
}

impl IntoIrValue for ir::Value {
    fn into_value(self, _: &mut FunctionBuilder<'_>) -> ir::Value {
        self
    }
}

impl IntoIrValue for bool {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I8, self as i64)
    }
}

impl IntoIrValue for i8 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I8, self as i64)
    }
}

impl IntoIrValue for u8 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I8, self as u64 as i64)
    }
}

impl IntoIrValue for i16 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I16, self as i64)
    }
}

impl IntoIrValue for u16 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I16, self as u64 as i64)
    }
}

impl IntoIrValue for i32 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I32, self as i64)
    }
}

impl IntoIrValue for u32 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().iconst(ir::types::I32, self as u64 as i64)
    }
}

impl IntoIrValue for f32 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().f32const(self)
    }
}

impl IntoIrValue for f64 {
    fn into_value(self, bd: &mut FunctionBuilder<'_>) -> ir::Value {
        bd.ins().f64const(self)
    }
}

impl BlockBuilder<'_> {
    /// NOP instruction - does absolutely nothing on purpose.
    pub fn nop(&mut self, action: Action) -> InstructionInfo {
        self.bd.ins().nop();
        InstructionInfo {
            cycles: 2,
            auto_pc: true,
            action,
        }
    }

    /// Stub instruction - does absolutely nothing as a temporary implementation.
    #[allow(dead_code)]
    pub fn stub(&mut self, ins: Ins) -> InstructionInfo {
        let mut parsed = ParsedIns::new();
        ins.parse_basic(&mut parsed);

        tracing::warn!("emitting stubbed instruction ({parsed})");

        self.bd.ins().nop();
        InstructionInfo {
            cycles: 2,
            auto_pc: true,
            action: Action::FlushAndPrologue,
        }
    }

    /// Creates an IR value from the given `value`.
    pub fn ir_value(&mut self, value: impl IntoIrValue) -> ir::Value {
        value.into_value(&mut self.bd)
    }

    /// Gets bit `index` in the `value` (must be an I32).
    pub fn get_bit(&mut self, value: ir::Value, index: impl IntoIrValue) -> ir::Value {
        let index = self.ir_value(index);

        let shifted = self.bd.ins().ushr(value, index);
        let bit = self.bd.ins().band_imm(shifted, 0b1);

        self.bd.ins().ireduce(ir::types::I8, bit)
    }

    /// Sets bit `index` to `set` in the `value` (must be an I32).
    pub fn set_bit(
        &mut self,
        value: ir::Value,
        index: impl IntoIrValue,
        should_set: impl IntoIrValue,
    ) -> ir::Value {
        let zero = self.ir_value(0i32);
        let one = self.ir_value(1i32);
        let index = self.ir_value(index);
        let should_set = self.ir_value(should_set);

        // create mask for the bit
        let mask = self.bd.ins().ishl(one, index);

        // unset bit
        let value = self.bd.ins().band_not(value, mask);

        // set bit if `should_set` is true
        let rhs = self.bd.ins().select(should_set, mask, zero);

        self.bd.ins().bor(value, rhs)
    }

    /// Rounds each lane in a F64X2 to single point precision (according to the codegen settings).
    pub fn round_to_single(&mut self, value: ir::Value) -> ir::Value {
        if self.codegen.settings.round_to_single {
            let single = self.bd.ins().fvdemote(value);
            self.bd.ins().fvpromote_low(single)
        } else {
            value
        }
    }

    /// Given a F64X2, copies lane 0 to lane 1.
    pub fn copy_ps0_to_ps1(&mut self, value: ir::Value) -> ir::Value {
        let bytes = self.bd.ins().bitcast(
            ir::types::I8X16,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            value,
        );

        const SHUFFLE_CONST: [u8; 16] = [
            0, 1, 2, 3, 4, 5, 6, 7, // ps1
            0, 1, 2, 3, 4, 5, 6, 7, // ps0
        ];

        let shuffle_const = self
            .bd
            .func
            .dfg
            .constants
            .insert(ir::ConstantData::from(SHUFFLE_CONST.as_bytes()));

        let mask = self.bd.ins().vconst(ir::types::I8X16, shuffle_const);
        let value = self.bd.ins().swizzle(bytes, mask);

        self.bd.ins().bitcast(
            ir::types::F64X2,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            value,
        )
    }

    /// Given a F64X2, copies lane 1 to lane 0.
    pub fn copy_ps1_to_ps0(&mut self, value: ir::Value) -> ir::Value {
        let bytes = self.bd.ins().bitcast(
            ir::types::I8X16,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            value,
        );

        const SHUFFLE_CONST: [u8; 16] = [
            8, 9, 10, 11, 12, 13, 14, 15, // ps0
            8, 9, 10, 11, 12, 13, 14, 15, // ps1
        ];

        let shuffle_const = self
            .bd
            .func
            .dfg
            .constants
            .insert(ir::ConstantData::from(SHUFFLE_CONST.as_slice()));

        let mask = self.bd.ins().vconst(ir::types::I8X16, shuffle_const);
        let value = self.bd.ins().swizzle(bytes, mask);

        self.bd.ins().bitcast(
            ir::types::F64X2,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            value,
        )
    }

    /// Given two F64X2, returns a new F64X2 with lanes [a[sel_a], b[sel_b]].
    pub fn ps_merge(&mut self, a: ir::Value, b: ir::Value, sel_a: bool, sel_b: bool) -> ir::Value {
        let mut mask = vec![];
        if sel_a {
            mask.extend_from_slice(&[8, 9, 10, 11, 12, 13, 14, 15]);
        } else {
            mask.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        }

        if sel_b {
            mask.extend_from_slice(&[24, 25, 26, 27, 28, 29, 30, 31]);
        } else {
            mask.extend_from_slice(&[16, 17, 18, 19, 20, 21, 22, 23]);
        }

        let bytes_a = self.bd.ins().bitcast(
            ir::types::I8X16,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            a,
        );

        let bytes_b = self.bd.ins().bitcast(
            ir::types::I8X16,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            b,
        );

        let mask = self
            .bd
            .func
            .dfg
            .immediates
            .push(ir::ConstantData::from(mask.as_slice()));

        let value = self.bd.ins().shuffle(bytes_a, bytes_b, mask);

        self.bd.ins().bitcast(
            ir::types::F64X2,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            value,
        )
    }

    /// Updates OV and SO in XER. `overflowed` must be a boolean (I8).
    pub fn update_xer_ov(&mut self, overflowed: impl IntoIrValue) {
        let xer = self.get(SPR::XER);
        let overflowed = self.ir_value(overflowed);
        let overflowed = self.bd.ins().uextend(ir::types::I32, overflowed);

        let ov = self.bd.ins().ishl_imm(overflowed, 30);
        let so = self.bd.ins().ishl_imm(overflowed, 31);
        let value = self.bd.ins().bor(ov, so);

        let mask = self.ir_value(0b1 << 30);
        let masked = self.bd.ins().band_not(xer, mask);
        let updated = self.bd.ins().bor(masked, value);

        self.set(SPR::XER, updated);
    }

    /// Updates CA in XER. `carry` must be a boolean (I8).
    pub fn update_xer_ca(&mut self, carry: impl IntoIrValue) {
        let xer = self.get(SPR::XER);
        let updated = self.set_bit(xer, 29, carry);

        self.set(SPR::XER, updated);
    }

    /// All IR values must be booleans (i.e. I8).
    pub fn update_cr(
        &mut self,
        index: u8,
        lt: ir::Value,
        gt: ir::Value,
        eq: ir::Value,
        ov: ir::Value,
    ) {
        let cr = self.get(Reg::CR);

        let lt = self.bd.ins().uextend(ir::types::I32, lt);
        let gt = self.bd.ins().uextend(ir::types::I32, gt);
        let eq = self.bd.ins().uextend(ir::types::I32, eq);
        let ov = self.bd.ins().uextend(ir::types::I32, ov);

        let base = (4 * (7 - index)) as u64 as i64;
        let lt = self.bd.ins().ishl_imm(lt, base + 3);
        let gt = self.bd.ins().ishl_imm(gt, base + 2);
        let eq = self.bd.ins().ishl_imm(eq, base + 1);
        let ov = self.bd.ins().ishl_imm(ov, base);

        let value = self.bd.ins().bor(lt, gt);
        let value = self.bd.ins().bor(value, eq);
        let value = self.bd.ins().bor(value, ov);

        let mask = self.ir_value(0b1111u32 << base);
        let updated = self.bd.ins().band_not(cr, mask);
        let updated = self.bd.ins().bor(updated, value);

        self.set(Reg::CR, updated);
    }

    /// Updates CR0 by signed comparison of the given value with 0 and by copying the overflow flag
    /// from XER SO. Value must be an I32.
    pub fn update_cr0_cmpz(&mut self, value: ir::Value) {
        let lt = self.bd.ins().icmp_imm(IntCC::SignedLessThan, value, 0);
        let gt = self.bd.ins().icmp_imm(IntCC::SignedGreaterThan, value, 0);
        let eq = self.bd.ins().icmp_imm(IntCC::Equal, value, 0);

        let xer = self.get(SPR::XER);
        let ov = self.get_bit(xer, 31);

        self.update_cr(0, lt, gt, eq, ov);
    }

    /// Updates the FPRF field in FPSCR register with the given flags.
    pub fn update_fprf(&mut self, lt: ir::Value, gt: ir::Value, eq: ir::Value, un: ir::Value) {
        let fpscr = self.get(Reg::FPSCR);

        let lt = self.bd.ins().uextend(ir::types::I32, lt);
        let gt = self.bd.ins().uextend(ir::types::I32, gt);
        let eq = self.bd.ins().uextend(ir::types::I32, eq);
        let un = self.bd.ins().uextend(ir::types::I32, un);

        let lt = self.bd.ins().ishl_imm(lt, 15);
        let gt = self.bd.ins().ishl_imm(gt, 14);
        let eq = self.bd.ins().ishl_imm(eq, 13);
        let un = self.bd.ins().ishl_imm(un, 12);

        let value = self.bd.ins().bor(lt, gt);
        let value = self.bd.ins().bor(value, eq);
        let value = self.bd.ins().bor(value, un);

        let mask = self.ir_value(0b11111u32 << 12);
        let updated = self.bd.ins().band_not(fpscr, mask);
        let updated = self.bd.ins().bor(updated, value);

        self.set(Reg::FPSCR, updated);
    }

    /// Updates the FPRF field in FPSCR register with flags computed from a comparison with 0.
    pub fn update_fprf_cmpz(&mut self, value: ir::Value) {
        let value = self.bd.ins().extractlane(value, 0);
        let zero = self.ir_value(0.0f64);

        let lt = self.bd.ins().fcmp(FloatCC::LessThan, value, zero);
        let gt = self.bd.ins().fcmp(FloatCC::GreaterThan, value, zero);
        let eq = self.bd.ins().fcmp(FloatCC::Equal, value, zero);
        let un = self.bd.ins().fcmp(FloatCC::Unordered, value, zero);

        self.update_fprf(lt, gt, eq, un);
    }

    pub fn update_fpscr(&mut self) {
        // TODO: implement this
    }

    /// Updates CR1 by copying bits 28..32 of FPSCR.
    pub fn update_cr1_float(&mut self) {
        self.update_fpscr();

        let fpscr = self.get(Reg::FPSCR);
        let cr = self.get(Reg::CR);

        let bits = self.bd.ins().ushr_imm(fpscr, 4);
        let mask = self.ir_value(0b1111 << 24);
        let updated = self.bd.ins().bitselect(mask, bits, cr);

        self.set(Reg::CR, updated);
    }
}
