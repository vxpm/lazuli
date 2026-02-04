use cranelift::codegen::ir;
use cranelift::prelude::{InstBuilder, IntCC};
use gekko::disasm::Ins;
use gekko::{InsExt, SPR};

use super::{Action, BlockBuilder};
use crate::builder::InstructionInfo;

const INT_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

const FLOAT_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

#[derive(Clone, Copy)]
enum AddLhs {
    RA,
    ZeroOrRA,
}

#[derive(Clone, Copy)]
enum AddRhs {
    RB,
    Imm,
    ShiftedImm,
    Zero,
    MinusOne,
}

#[derive(Clone, Copy)]
struct AddOp {
    lhs: AddLhs,
    rhs: AddRhs,
    extend: bool,
    record: bool,
    carry: bool,
    overflow: bool,
}

/// Integer addition operations
impl BlockBuilder<'_> {
    fn addition_get_lhs(&mut self, ins: Ins, lhs: AddLhs) -> ir::Value {
        match lhs {
            AddLhs::RA => self.get(ins.gpr_a()),
            AddLhs::ZeroOrRA => {
                if ins.field_ra() == 0 {
                    self.ir_value(0i32)
                } else {
                    self.get(ins.gpr_a())
                }
            }
        }
    }

    fn addition_get_rhs(&mut self, ins: Ins, rhs: AddRhs) -> ir::Value {
        match rhs {
            AddRhs::RB => self.get(ins.gpr_b()),
            AddRhs::Imm => self.ir_value(ins.field_simm() as i32),
            AddRhs::ShiftedImm => self.ir_value((ins.field_simm() as i32) << 16),
            AddRhs::Zero => self.ir_value(0),
            AddRhs::MinusOne => self.ir_value(-1i32),
        }
    }

    fn addition_overflow(
        &mut self,
        lhs: ir::Value,
        rhs: ir::Value,
        result: ir::Value,
    ) -> ir::Value {
        let lhs_sign = self.bd.ins().band_imm(lhs, 0b1 << 31);
        let rhs_sign = self.bd.ins().band_imm(rhs, 0b1 << 31);
        let result_sign = self.bd.ins().band_imm(result, 0b1 << 31);

        let lhs_eq_rhs = self.bd.ins().icmp(IntCC::Equal, lhs_sign, rhs_sign);
        let result_sign_diff = self.bd.ins().icmp(IntCC::NotEqual, result_sign, lhs_sign);

        self.bd.ins().band(lhs_eq_rhs, result_sign_diff)
    }

    fn addition(&mut self, ins: Ins, op: AddOp) -> InstructionInfo {
        let lhs = self.addition_get_lhs(ins, op.lhs);
        let rhs = self.addition_get_rhs(ins, op.rhs);

        let cin = if op.extend {
            let xer = self.get(SPR::XER);
            let ca = self.get_bit(xer, 29);
            self.bd.ins().uextend(ir::types::I32, ca)
        } else {
            self.ir_value(0i32)
        };

        let (value, cout_a) = self.bd.ins().uadd_overflow(lhs, rhs);
        let (value, cout_b) = self.bd.ins().uadd_overflow(value, cin);

        if op.overflow {
            let overflowed = self.addition_overflow(lhs, rhs, value);
            self.update_xer_ov(overflowed);
        }

        if op.carry {
            let carry = self.bd.ins().bor(cout_a, cout_b);
            self.update_xer_ca(carry);
        }

        if op.record {
            self.update_cr0_cmpz(value);
        }

        self.set(ins.gpr_d(), value);

        INT_INFO
    }

    pub fn add(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::RB,
                extend: false,
                record: ins.field_rc(),
                carry: false,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn addc(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::RB,
                extend: false,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn adde(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::RB,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn addze(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::Zero,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn addi(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::ZeroOrRA,
                rhs: AddRhs::Imm,
                extend: false,
                record: false,
                carry: false,
                overflow: false,
            },
        )
    }

    pub fn addis(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::ZeroOrRA,
                rhs: AddRhs::ShiftedImm,
                extend: false,
                record: false,
                carry: false,
                overflow: false,
            },
        )
    }

    pub fn addic(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::Imm,
                extend: false,
                record: false,
                carry: true,
                overflow: false,
            },
        )
    }

    pub fn addic_record(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::Imm,
                extend: false,
                record: true,
                carry: true,
                overflow: false,
            },
        )
    }

    pub fn addme(&mut self, ins: Ins) -> InstructionInfo {
        self.addition(
            ins,
            AddOp {
                lhs: AddLhs::RA,
                rhs: AddRhs::MinusOne,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }
}

#[derive(Clone, Copy)]
enum SubLhs {
    RB,
    Imm,
    MinusOne,
    Zero,
}

#[derive(Clone, Copy)]
struct SubOp {
    lhs: SubLhs,
    extend: bool,
    record: bool,
    carry: bool,
    overflow: bool,
}

/// Integer sub from operations
impl BlockBuilder<'_> {
    fn subtraction_get_lhs(&mut self, ins: Ins, lhs: SubLhs) -> ir::Value {
        match lhs {
            SubLhs::RB => self.get(ins.gpr_b()),
            SubLhs::Imm => self.ir_value(ins.field_simm() as i32),
            SubLhs::MinusOne => self.ir_value(-1i32),
            SubLhs::Zero => self.ir_value(0i32),
        }
    }

    fn subtraction_overflow(
        &mut self,
        lhs: ir::Value,
        rhs: ir::Value,
        result: ir::Value,
    ) -> ir::Value {
        let lhs_sign = self.bd.ins().band_imm(lhs, 0b1 << 31);
        let rhs_sign = self.bd.ins().band_imm(rhs, 0b1 << 31);
        let result_sign = self.bd.ins().band_imm(result, 0b1 << 31);

        let rhs_eq_value = self.bd.ins().icmp(IntCC::Equal, rhs_sign, result_sign);
        let lhs_sign_diff = self.bd.ins().icmp(IntCC::NotEqual, lhs_sign, rhs_sign);

        self.bd.ins().band(rhs_eq_value, lhs_sign_diff)
    }

    fn subtraction(&mut self, ins: Ins, op: SubOp) -> InstructionInfo {
        let lhs = self.subtraction_get_lhs(ins, op.lhs);
        let rhs = self.get(ins.gpr_a());

        let cin = if op.extend {
            let xer = self.get(SPR::XER);
            let ca = self.get_bit(xer, 29);
            self.bd.ins().uextend(ir::types::I32, ca)
        } else {
            self.ir_value(1i32)
        };

        let not_rhs = self.bd.ins().bnot(rhs);
        let (value, cout_a) = self.bd.ins().uadd_overflow(lhs, not_rhs);
        let (value, cout_b) = self.bd.ins().uadd_overflow(value, cin);

        if op.carry {
            let carry = self.bd.ins().bor(cout_a, cout_b);
            self.update_xer_ca(carry);
        }

        if op.overflow {
            let overflowed = self.subtraction_overflow(lhs, rhs, value);
            self.update_xer_ov(overflowed);
        }

        if op.record {
            self.update_cr0_cmpz(value);
        }

        self.set(ins.gpr_d(), value);

        INT_INFO
    }

    pub fn subf(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::RB,
                extend: false,
                record: ins.field_rc(),
                carry: false,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn subfe(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::RB,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn subfc(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::RB,
                extend: false,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn subfic(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::Imm,
                extend: false,
                record: false,
                carry: true,
                overflow: false,
            },
        )
    }

    pub fn subfme(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::MinusOne,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }

    pub fn subfze(&mut self, ins: Ins) -> InstructionInfo {
        self.subtraction(
            ins,
            SubOp {
                lhs: SubLhs::Zero,
                extend: true,
                record: ins.field_rc(),
                carry: true,
                overflow: ins.field_oe(),
            },
        )
    }
}

const MUL_INFO: InstructionInfo = InstructionInfo {
    cycles: 3,
    auto_pc: true,
    action: Action::Continue,
};

const DIV_INFO: InstructionInfo = InstructionInfo {
    cycles: 19,
    auto_pc: true,
    action: Action::Continue,
};

/// Integer multiplication and division operations
impl BlockBuilder<'_> {
    pub fn neg(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let value = self.bd.ins().ineg(ra);
        let overflowed = self.bd.ins().icmp_imm(IntCC::Equal, ra, i32::MIN as i64);

        if ins.field_oe() {
            self.update_xer_ov(overflowed);
        }

        if ins.field_rc() {
            self.update_cr0_cmpz(value);
        }

        self.set(ins.gpr_d(), value);

        INT_INFO
    }

    pub fn divw(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        // division by zero: undefined, just avoid it by using 1 as denom instead
        let one = self.ir_value(1i32);
        let is_div_by_zero = self.bd.ins().icmp_imm(IntCC::Equal, rb, 0);
        let denom = self.bd.ins().select(is_div_by_zero, one, rb);

        // special case: if dividing 0x8000_0000 by -1, replace the denom with 1 too
        let is_min_neg = self.bd.ins().icmp_imm(IntCC::Equal, ra, 0x8000_0000);
        let is_div_by_minus_one = self.bd.ins().icmp_imm(IntCC::Equal, rb, -1);
        let is_special_case = self.bd.ins().band(is_min_neg, is_div_by_minus_one);
        let denom = self.bd.ins().select(is_special_case, one, denom);

        let result = self.bd.ins().sdiv(ra, denom);

        if ins.field_oe() {
            let overflow = self.bd.ins().bor(is_div_by_zero, is_special_case);
            self.update_xer_ov(overflow);
        }

        if ins.field_rc() {
            self.update_cr0_cmpz(result);
        }

        self.set(ins.gpr_d(), result);

        DIV_INFO
    }

    pub fn divwu(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        // division by zero: undefined, just avoid it by using 1 as denom instead
        let one = self.ir_value(1i32);
        let is_div_by_zero = self.bd.ins().icmp_imm(IntCC::Equal, rb, 0);
        let denom = self.bd.ins().select(is_div_by_zero, one, rb);

        let result = self.bd.ins().udiv(ra, denom);

        if ins.field_oe() {
            self.update_xer_ov(is_div_by_zero);
        }

        if ins.field_rc() {
            self.update_cr0_cmpz(result);
        }

        self.set(ins.gpr_d(), result);

        DIV_INFO
    }

    pub fn mullw(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        let (result, overflowed) = self.bd.ins().smul_overflow(ra, rb);

        if ins.field_oe() {
            self.update_xer_ov(overflowed);
        }

        if ins.field_rc() {
            self.update_cr0_cmpz(result);
        }

        self.set(ins.gpr_d(), result);

        MUL_INFO
    }

    pub fn mulli(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let imm = self.ir_value(ins.field_simm() as i32);

        let result = self.bd.ins().imul(ra, imm);
        self.set(ins.gpr_d(), result);

        MUL_INFO
    }

    pub fn mulhw(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        let result = self.bd.ins().smulhi(ra, rb);

        if ins.field_rc() {
            self.update_cr0_cmpz(result);
        }

        self.set(ins.gpr_d(), result);

        MUL_INFO
    }

    pub fn mulhwu(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        let result = self.bd.ins().umulhi(ra, rb);

        if ins.field_rc() {
            self.update_cr0_cmpz(result);
        }

        self.set(ins.gpr_d(), result);

        MUL_INFO
    }
}

/// Floating point addition operations
impl BlockBuilder<'_> {
    pub fn fadd(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fadd(fpr_a, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fadds(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fadd(fpr_a, fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_add(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fadd(fpr_a, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }
}

/// Floating point subtraction operations
impl BlockBuilder<'_> {
    pub fn fsub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fsub(fpr_a, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fsubs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fsub(fpr_a, fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_sub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fsub(fpr_a, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }
}

/// Floating point multiply and divide operations
impl BlockBuilder<'_> {
    pub fn fneg(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fneg(fpr_b);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmuls(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fmul(fpr_a, fpr_c);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmul(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fmul(fpr_a, fpr_c);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmadds(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmadd(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmsubs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fmsub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fnmadd(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        let value = self.bd.ins().fneg(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fnmadds(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        let value = self.bd.ins().fneg(value);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fnmsub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        let value = self.bd.ins().fneg(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fnmsubs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        let value = self.bd.ins().fneg(value);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fdivs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fdiv(fpr_a, fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fdiv(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fdiv(fpr_a, fpr_b);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_neg(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fneg(fpr_b);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_mul(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fmul(fpr_a, fpr_c);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_madd(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_madds0(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let fpr_c = self.get(ins.fpr_c());
        let fpr_c_ps0 = self.copy_ps0_to_ps1(fpr_c);

        let value = self.bd.ins().fma(fpr_a, fpr_c_ps0, fpr_b);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_madds1(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let fpr_c = self.get(ins.fpr_c());
        let fpr_c_ps1 = self.copy_ps1_to_ps0(fpr_c);

        let value = self.bd.ins().fma(fpr_a, fpr_c_ps1, fpr_b);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_msub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_nmadd(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let value = self.bd.ins().fma(fpr_a, fpr_c, fpr_b);
        let value = self.bd.ins().fneg(value);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_nmsub(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let neg_fpr_b = self.bd.ins().fneg(fpr_b);
        let value = self.bd.ins().fma(fpr_a, fpr_c, neg_fpr_b);
        let value = self.bd.ins().fneg(value);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_muls0(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_c = self.get(ins.fpr_c());
        let fpr_c_ps0 = self.copy_ps0_to_ps1(fpr_c);

        let value = self.bd.ins().fmul(fpr_a, fpr_c_ps0);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_muls1(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_c = self.get(ins.fpr_c());
        let fpr_c_ps1 = self.copy_ps1_to_ps0(fpr_c);

        let value = self.bd.ins().fmul(fpr_a, fpr_c_ps1);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_div(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fdiv(fpr_a, fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }
}
