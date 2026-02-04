use cranelift::codegen::ir;
use cranelift::prelude::{FloatCC, InstBuilder, IntCC};
use gekko::disasm::Ins;
use gekko::{InsExt, SPR};

use super::BlockBuilder;
use crate::builder::{Action, InstructionInfo};

const CMP_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

/// Integer comparison operations
impl BlockBuilder<'_> {
    fn compare_signed(&mut self, a: ir::Value, b: ir::Value, index: u8) {
        let xer = self.get(SPR::XER);

        let lt = self.bd.ins().icmp(IntCC::SignedLessThan, a, b);
        let gt = self.bd.ins().icmp(IntCC::SignedGreaterThan, a, b);
        let eq = self.bd.ins().icmp(IntCC::Equal, a, b);

        let ov = self.bd.ins().ushr_imm(xer, 31);
        let ov = self.bd.ins().icmp_imm(IntCC::NotEqual, ov, 0);

        self.update_cr(index, lt, gt, eq, ov);
    }

    fn compare_unsigned(&mut self, a: ir::Value, b: ir::Value, index: u8) {
        let xer = self.get(SPR::XER);

        let lt = self.bd.ins().icmp(IntCC::UnsignedLessThan, a, b);
        let gt = self.bd.ins().icmp(IntCC::UnsignedGreaterThan, a, b);
        let eq = self.bd.ins().icmp(IntCC::Equal, a, b);

        let ov = self.bd.ins().ushr_imm(xer, 31);
        let ov = self.bd.ins().icmp_imm(IntCC::NotEqual, ov, 0);

        self.update_cr(index, lt, gt, eq, ov);
    }

    pub fn cmp(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        self.compare_signed(ra, rb, ins.field_crfd());

        CMP_INFO
    }

    pub fn cmpi(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let imm = self.ir_value(ins.field_simm() as i32);

        self.compare_signed(ra, imm, ins.field_crfd());

        CMP_INFO
    }

    pub fn cmpl(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let rb = self.get(ins.gpr_b());

        self.compare_unsigned(ra, rb, ins.field_crfd());

        CMP_INFO
    }

    pub fn cmpli(&mut self, ins: Ins) -> InstructionInfo {
        let ra = self.get(ins.gpr_a());
        let imm = self.ir_value(ins.field_uimm() as u32);

        self.compare_unsigned(ra, imm, ins.field_crfd());

        CMP_INFO
    }
}

/// Floating point comparison operations
impl BlockBuilder<'_> {
    pub fn fcmpu(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        self.update_fprf_cmpu(fpr_a, fpr_b);

        // TODO: make this lazy too
        let fpr_a = self.bd.ins().extractlane(fpr_a, 0);
        let fpr_b = self.bd.ins().extractlane(fpr_b, 0);

        let lt = self.bd.ins().fcmp(FloatCC::LessThan, fpr_a, fpr_b);
        let gt = self.bd.ins().fcmp(FloatCC::GreaterThan, fpr_a, fpr_b);
        let eq = self.bd.ins().fcmp(FloatCC::Equal, fpr_a, fpr_b);
        let un = self.bd.ins().fcmp(FloatCC::Unordered, fpr_a, fpr_b);

        self.update_cr(ins.field_crfd(), lt, gt, eq, un);

        CMP_INFO
    }

    pub fn fcmpo(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        self.update_fprf_cmpu(fpr_a, fpr_b);

        // TODO: make this lazy too
        let fpr_a = self.bd.ins().extractlane(fpr_a, 0);
        let fpr_b = self.bd.ins().extractlane(fpr_b, 0);

        let lt = self.bd.ins().fcmp(FloatCC::LessThan, fpr_a, fpr_b);
        let gt = self.bd.ins().fcmp(FloatCC::GreaterThan, fpr_a, fpr_b);
        let eq = self.bd.ins().fcmp(FloatCC::Equal, fpr_a, fpr_b);
        let un = self.bd.ins().fcmp(FloatCC::Unordered, fpr_a, fpr_b);
        self.update_cr(ins.field_crfd(), lt, gt, eq, un);

        CMP_INFO
    }

    pub fn ps_cmpo0(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        self.update_fprf_cmpu(fpr_a, fpr_b);

        // TODO: make this lazy too
        let fpr_a = self.bd.ins().extractlane(fpr_a, 0);
        let fpr_b = self.bd.ins().extractlane(fpr_b, 0);

        let lt = self.bd.ins().fcmp(FloatCC::LessThan, fpr_a, fpr_b);
        let gt = self.bd.ins().fcmp(FloatCC::GreaterThan, fpr_a, fpr_b);
        let eq = self.bd.ins().fcmp(FloatCC::Equal, fpr_a, fpr_b);
        let un = self.bd.ins().fcmp(FloatCC::Unordered, fpr_a, fpr_b);
        self.update_cr(ins.field_crfd(), lt, gt, eq, un);

        CMP_INFO
    }
}
