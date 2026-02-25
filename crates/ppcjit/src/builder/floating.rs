use cranelift::codegen::ir;
use cranelift::prelude::{FloatCC, InstBuilder};
use gekko::disasm::Ins;
use gekko::{InsExt, Reg};

use super::BlockBuilder;
use crate::builder::{Action, InstructionInfo};

const FLOAT_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

impl BlockBuilder<'_> {
    pub fn fmr(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());
        self.set(ins.fpr_d(), fpr_b);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn frsp(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let value = self.round_to_single(fpr_b);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fctiw(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());
        let fpr_b_ps0 = self.bd.ins().extractlane(fpr_b, 0);

        let int32 = self.bd.ins().fcvt_to_sint_sat(ir::types::I32, fpr_b_ps0);
        let int64 = self.bd.ins().sextend(ir::types::I64, int32);
        let float = self
            .bd
            .ins()
            .bitcast(ir::types::F64, ir::MemFlags::new(), int64);
        let vector = self.bd.ins().scalar_to_vector(ir::types::F64X2, float);
        self.set(ins.fpr_d(), vector);

        self.update_fprf_cmpz(vector);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fctiwz(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        // TODO: maybe manually round towards zero

        let fpr_b = self.get(ins.fpr_b());
        let fpr_b_ps0 = self.bd.ins().extractlane(fpr_b, 0);

        let int32 = self.bd.ins().fcvt_to_sint_sat(ir::types::I32, fpr_b_ps0);
        let int64 = self.bd.ins().sextend(ir::types::I64, int32);
        let float = self
            .bd
            .ins()
            .bitcast(ir::types::F64, ir::MemFlags::new(), int64);
        let vector = self.bd.ins().scalar_to_vector(ir::types::F64X2, float);
        self.set(ins.fpr_d(), vector);

        self.update_fprf_cmpz(vector);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fres(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let one = self.ir_value(1.0f64);
        let one = self.bd.ins().splat(ir::types::F64X2, one);
        let value = self.bd.ins().fdiv(one, fpr_b);
        let value = self.round_to_single(value);
        let value = self.copy_ps0_to_ps1(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn frsqrte(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let one = self.ir_value(1.0f64);
        let one = self.bd.ins().splat(ir::types::F64X2, one);
        let sqrt = self.bd.ins().sqrt(fpr_b);
        let value = self.bd.ins().fdiv(one, sqrt);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fabs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let value = self.bd.ins().fabs(fpr_b);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_rsqrte(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let one = self.ir_value(1.0f64);
        let one = self.bd.ins().splat(ir::types::F64X2, one);
        let sqrt = self.bd.ins().sqrt(fpr_b);
        let value = self.bd.ins().fdiv(one, sqrt);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_res(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());

        let one = self.ir_value(1.0f64);
        let one = self.bd.ins().splat(ir::types::F64X2, one);
        let value = self.bd.ins().fdiv(one, fpr_b);
        let value = self.round_to_single(value);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_mr(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_b = self.get(ins.fpr_b());
        self.set(ins.fpr_d(), fpr_b);

        FLOAT_INFO
    }

    pub fn ps_sum0(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        // ps0 = a0 + b1
        // ps1 = c1

        let ac = self.ps_merge(fpr_a, fpr_c, false, true);
        let b1 = self.bd.ins().extractlane(fpr_b, 1);
        let b1 = self.bd.ins().scalar_to_vector(ir::types::F64X2, b1);

        let value = self.bd.ins().fadd(ac, b1);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_sum1(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        // ps0 = c0
        // ps1 = a0 + b1

        let zero = self.ir_value(0.0f64);
        let ca = self.ps_merge(fpr_c, fpr_a, false, false);
        let b1 = self.bd.ins().insertlane(fpr_b, zero, 0);

        let value = self.bd.ins().fadd(ca, b1);
        self.set(ins.fpr_d(), value);

        self.update_fprf_cmpz(value);
        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_merge00(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        // ps0 = a0
        // ps1 = b0

        let value = self.ps_merge(fpr_a, fpr_b, false, false);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_merge01(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        // ps0 = a0
        // ps1 = b1

        let value = self.ps_merge(fpr_a, fpr_b, false, true);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_merge10(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        // ps0 = a1
        // ps1 = b0

        let value = self.ps_merge(fpr_a, fpr_b, true, false);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_merge11(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());

        // ps0 = a1
        // ps1 = b1

        let value = self.ps_merge(fpr_a, fpr_b, true, true);
        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn mffs(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let fpscr = self.get(Reg::FPSCR);
        let extended = self.bd.ins().uextend(ir::types::I64, fpscr);
        let float = self
            .bd
            .ins()
            .bitcast(ir::types::F64, ir::MemFlags::new(), extended);
        let paired = self.bd.ins().splat(ir::types::F64X2, float);

        self.set(ins.fpr_d(), paired);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn fsel(&mut self, ins: Ins) -> InstructionInfo {
        self.check_floats();

        let zero = self.ir_value(0.0);
        let fpr_a = self.get(ins.fpr_a());
        let fpr_b = self.get(ins.fpr_b());
        let fpr_c = self.get(ins.fpr_c());

        let fpr_a_ps0 = self.bd.ins().extractlane(fpr_a, 0);
        let cond = self
            .bd
            .ins()
            .fcmp(FloatCC::GreaterThanOrEqual, fpr_a_ps0, zero);
        let value = self.bd.ins().select(cond, fpr_c, fpr_b);

        self.set(ins.fpr_d(), value);

        if ins.field_rc() {
            self.update_cr1_float();
        }

        FLOAT_INFO
    }

    pub fn ps_sel(&mut self, ins: Ins) -> InstructionInfo {
        // self.check_floats();
        //
        // let zero = self.ir_value(0.0);
        // let zero = self.bd.ins().splat(ir::types::F64X2, zero);
        //
        // let fpr_a = self.get(ins.fpr_a());
        // let fpr_b = self.get(ins.fpr_b());
        // let fpr_c = self.get(ins.fpr_c());
        //
        // let mask = self.bd.ins().fcmp(FloatCC::GreaterThanOrEqual, fpr_a, zero);
        // let mask_inverse = self.bd.ins().fcmp(FloatCC::GreaterThanOrEqual, fpr_a, zero);
        //
        // let value = self.bd.ins().select(cond, fpr_c, fpr_b);
        // self.set(ins.fpr_d(), value);
        //
        // if ins.field_rc() {
        //     self.update_cr1_float();
        // }

        FLOAT_INFO
    }
}
