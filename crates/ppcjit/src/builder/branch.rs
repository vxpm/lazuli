use bitos::bitos;
use bitos::integer::u5;
use cranelift::codegen::ir;
use cranelift::prelude::InstBuilder;
use gekko::disasm::Ins;
use gekko::{Reg, SPR};

use super::BlockBuilder;
use crate::builder::util::IntoIrValue;
use crate::builder::{Action, InstructionInfo};

const UNCONDITIONAL_BRANCH_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: false,
    action: Action::Exit,
};

const CONDITIONAL_BRANCH_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

#[bitos(1)]
#[derive(Debug, Clone, Copy)]
enum CtrCond {
    NotEqZero = 0,
    EqZero    = 1,
}

#[bitos(5)]
#[derive(Debug)]
struct BranchOptions {
    #[bits(0)]
    likely: bool,
    #[bits(1)]
    ctr_cond: CtrCond,
    #[bits(2)]
    ignore_ctr: bool,
    #[bits(3)]
    desired_cr: bool,
    #[bits(4)]
    ignore_cr: bool,
}

impl BranchOptions {
    fn is_unconditional(&self) -> bool {
        self.ignore_ctr() && self.ignore_cr()
    }
}

impl BlockBuilder<'_> {
    fn jump(&mut self, relative: bool, link_register: bool, target: ir::Value) {
        let current_pc = self.get(Reg::PC);
        let destination = if relative {
            self.bd.ins().iadd(current_pc, target)
        } else {
            target
        };

        if link_register {
            let ret_addr = self.bd.ins().iadd_imm(current_pc, 4);
            self.set(SPR::LR, ret_addr);
        }

        self.set(Reg::PC, destination);
    }

    pub fn b(&mut self, ins: Ins) -> InstructionInfo {
        let destination = self.ir_value(ins.field_li());
        self.jump(!ins.field_aa(), ins.field_lk(), destination);
        UNCONDITIONAL_BRANCH_INFO
    }

    fn branch(&mut self, ins: Ins, relative: bool, target: impl IntoIrValue) -> InstructionInfo {
        let options = BranchOptions::from_bits(u5::new(ins.field_bo()));
        let target = self.ir_value(target);

        if options.is_unconditional() {
            self.jump(relative, ins.field_lk(), target);
            return UNCONDITIONAL_BRANCH_INFO;
        }

        let cond_bit = 31 - ins.field_bi();
        let current_pc = self.get(Reg::PC);

        let mut branch = self.ir_value(true);
        if !options.ignore_cr() {
            let cr = self.get(Reg::CR);

            let bit = self.get_bit(cr, cond_bit);
            let condition = if options.desired_cr() {
                bit
            } else {
                self.bd.ins().bnot(bit)
            };

            branch = self.bd.ins().band(branch, condition);
        }

        if !options.ignore_ctr() {
            let ctr = self.get(SPR::CTR);
            let ctr = self.bd.ins().iadd_imm(ctr, -1);
            self.set(SPR::CTR, ctr);

            let condition = match options.ctr_cond() {
                CtrCond::NotEqZero => ir::condcodes::IntCC::NotEqual,
                CtrCond::EqZero => ir::condcodes::IntCC::Equal,
            };

            let condition = self.bd.ins().icmp_imm(condition, ctr, 0);
            branch = self.bd.ins().band(branch, condition);
        }

        let exit_block = self.bd.create_block();
        let continue_block = self.bd.create_block();

        self.bd.set_cold_block(if options.likely() {
            continue_block
        } else {
            exit_block
        });

        self.bd
            .ins()
            .brif(branch, exit_block, &[], continue_block, &[]);

        self.bd.seal_block(exit_block);
        self.bd.seal_block(continue_block);

        // => exit (take branch)
        self.switch_to_bb(exit_block);
        let target = self.ir_value(target);
        self.jump(relative, ins.field_lk(), target);
        self.flush();
        self.exit();

        // => continue (do not take branch)
        self.switch_to_bb(continue_block);
        self.current_bb = continue_block;

        self.set(Reg::PC, current_pc);

        CONDITIONAL_BRANCH_INFO
    }

    pub fn bc(&mut self, ins: Ins) -> InstructionInfo {
        self.branch(ins, !ins.field_aa(), ins.field_bd() as i32)
    }

    pub fn bclr(&mut self, ins: Ins) -> InstructionInfo {
        let lr = self.get(SPR::LR);
        self.branch(ins, false, lr)
    }

    pub fn bcctr(&mut self, ins: Ins) -> InstructionInfo {
        let ctr = self.get(SPR::CTR);
        self.branch(ins, false, ctr)
    }
}
