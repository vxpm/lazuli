use cranelift::codegen::ir;
use cranelift::codegen::ir::InstBuilder;
use cranelift::codegen::isa::CallConv;
use gekko::disasm::Ins;
use gekko::{Exception, Reg, SPR};

use super::BlockBuilder;
use crate::builder::{Action, InstructionInfo};

const RFI_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: false,
    action: Action::FlushAndPrologue,
};

const EXCEPTION_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: false,
    action: Action::Prologue,
};

pub fn raise_exception_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
    ir::Signature {
        params: vec![
            ir::AbiParam::new(ptr_type),       // registers
            ir::AbiParam::new(ir::types::I16), // exception
        ],
        returns: vec![],
        call_conv,
    }
}

impl BlockBuilder<'_> {
    /// # Warning
    /// You should _always_ exit after raising an exception.
    pub fn raise_exception(&mut self, exception: Exception) {
        let exception = self
            .bd
            .ins()
            .iconst(ir::types::I16, exception as u64 as i64);

        self.flush();

        self.bd.ins().call(
            self.hooks.raise_exception,
            &[self.consts.regs_ptr, exception],
        );
    }

    /// Checks whether floating point operations are enabled in MSR and raises an exception if not.
    pub fn check_floats(&mut self) {
        if self.floats_checked || self.codegen.settings.force_fpu {
            return;
        }
        self.floats_checked = true;

        let msr = self.get(Reg::MSR);
        let fp_enabled = self.get_bit(msr, 13);

        let exit_block = self.bd.create_block();
        let continue_block = self.bd.create_block();

        self.bd.set_cold_block(exit_block);

        self.bd
            .ins()
            .brif(fp_enabled, continue_block, &[], exit_block, &[]);

        self.bd.seal_block(exit_block);
        self.bd.seal_block(continue_block);

        self.switch_to_bb(exit_block);
        self.raise_exception(Exception::FloatUnavailable);
        self.prologue();

        self.switch_to_bb(continue_block);
        self.current_bb = continue_block;
    }

    pub fn sc(&mut self, _: Ins) -> InstructionInfo {
        if self.codegen.settings.nop_syscalls {
            return self.nop(Action::FlushAndPrologue);
        }

        self.raise_exception(Exception::Syscall);
        EXCEPTION_INFO
    }

    pub fn rfi(&mut self, _: Ins) -> InstructionInfo {
        let msr = self.get(Reg::MSR);
        let srr0 = self.get(SPR::SRR0);
        let srr1 = self.get(SPR::SRR1);
        let mask = self.ir_value(Exception::SRR1_TO_MSR_MASK);

        // move only some bits from srr1
        let new_msr = self.bd.ins().bitselect(mask, srr1, msr);

        // clear bit 18
        let new_msr = self.bd.ins().band_imm(new_msr, !(1 << 18));

        // TODO: deal with new_msr exceptions enabled

        // set PC to SRR0
        let new_pc = self.bd.ins().band_imm(srr0, !0b11);
        self.set(Reg::PC, new_pc);
        self.set(Reg::MSR, new_msr);

        self.call_generic_hook(self.hooks.msr_changed);

        RFI_INFO
    }
}
