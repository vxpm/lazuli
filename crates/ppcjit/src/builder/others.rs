use bitos::BitUtils;
use cranelift::codegen::ir;
use cranelift::prelude::InstBuilder;
use gekko::disasm::Ins;
use gekko::{InsExt, Reg, SPR};

use super::BlockBuilder;
use crate::builder::{Action, InstructionInfo};

const SPR_INFO: InstructionInfo = InstructionInfo {
    cycles: 1,
    auto_pc: true,
    action: Action::Continue,
};

const MSR_INFO: InstructionInfo = InstructionInfo {
    cycles: 1,
    auto_pc: true,
    action: Action::Continue,
};

const CR_INFO: InstructionInfo = InstructionInfo {
    cycles: 1,
    auto_pc: true,
    action: Action::Continue,
};

const SR_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

const TB_INFO: InstructionInfo = InstructionInfo {
    cycles: 1,
    auto_pc: true,
    action: Action::Continue,
};

const DCACHE_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::Continue,
};

const INV_ICACHE_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::FlushAndPrologue,
};

const SYNC_ICACHE_INFO: InstructionInfo = InstructionInfo {
    cycles: 2,
    auto_pc: true,
    action: Action::FlushAndPrologue,
};

fn generate_mask(control: u8) -> u32 {
    let mut mask = 0;
    for i in 0..8 {
        if control.bit(i) {
            mask |= (0xF) << (4 * i);
        }
    }

    mask
}

impl BlockBuilder<'_> {
    pub fn mfspr(&mut self, ins: Ins) -> InstructionInfo {
        let spr = ins.spr();
        match spr {
            SPR::DEC => self.call_generic_hook(self.hooks.dec_read),
            SPR::TBL | SPR::TBU => self.call_generic_hook(self.hooks.tb_read),
            SPR::WPAR => tracing::warn!("read from WPAR"),
            _ => (),
        }

        let value = self.get(spr);
        self.set(ins.gpr_d(), value);

        SPR_INFO
    }

    pub fn mtspr(&mut self, ins: Ins) -> InstructionInfo {
        let value = self.get(ins.gpr_s());
        let spr = ins.spr();
        self.set(spr, value);

        match spr {
            SPR::DEC => self.call_generic_hook(self.hooks.dec_changed),
            SPR::TBL | SPR::TBU => self.call_generic_hook(self.hooks.tb_changed),
            SPR::DMAL | SPR::DMAU => self.call_generic_hook(self.hooks.dcache_dma),
            SPR::WPAR => tracing::warn!("write to WPAR"),
            spr if spr.is_data_bat() => self.dbat_changed = true,
            spr if spr.is_instr_bat() => self.ibat_changed = true,
            _ => (),
        }

        SPR_INFO
    }

    pub fn mtsr(&mut self, ins: Ins) -> InstructionInfo {
        let value = self.get(ins.gpr_s());
        let sr = Reg::SR[ins.field_sr() as usize];
        self.set(sr, value);

        SR_INFO
    }

    pub fn mfsr(&mut self, ins: Ins) -> InstructionInfo {
        let sr = Reg::SR[ins.field_sr() as usize];
        let value = self.get(sr);
        self.set(ins.gpr_d(), value);

        SR_INFO
    }

    pub fn mfmsr(&mut self, ins: Ins) -> InstructionInfo {
        let value = self.get(Reg::MSR);
        self.set(ins.gpr_d(), value);

        MSR_INFO
    }

    pub fn mtmsr(&mut self, ins: Ins) -> InstructionInfo {
        // TODO: deal with exception stuff

        let value = self.get(ins.gpr_s());
        self.set(Reg::MSR, value);

        self.call_generic_hook(self.hooks.msr_changed);

        MSR_INFO
    }

    pub fn mfcr(&mut self, ins: Ins) -> InstructionInfo {
        let value = self.get(Reg::CR);
        self.set(ins.gpr_d(), value);

        CR_INFO
    }

    pub fn mtcrf(&mut self, ins: Ins) -> InstructionInfo {
        let rs = self.get(ins.gpr_s());
        let mask = self.ir_value(generate_mask(ins.field_crm()));

        let cr = self.get(Reg::CR);
        let value = self.bd.ins().bitselect(mask, rs, cr);

        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn mtfsf(&mut self, ins: Ins) -> InstructionInfo {
        let fpr_b = self.get(ins.fpr_b());
        let mask = self.ir_value(generate_mask(ins.field_mtfsf_fm()));

        let fpscr = self.get(Reg::FPSCR);
        let bits = self
            .bd
            .ins()
            .bitcast(ir::types::I64, ir::MemFlags::new(), fpr_b);
        let low = self.bd.ins().ireduce(ir::types::I32, bits);

        let value = self.bd.ins().bitselect(mask, low, fpscr);
        self.set(Reg::FPSCR, value);

        self.update_fpscr();

        if ins.field_rc() {
            self.update_cr1_float();
        }

        CR_INFO
    }

    pub fn mftb(&mut self, ins: Ins) -> InstructionInfo {
        self.call_generic_hook(self.hooks.tb_read);

        let tb = match ins.field_tbr() {
            268 => SPR::TBL,
            269 => SPR::TBU,
            _ => todo!(),
        };

        let value = self.get(tb);
        self.set(ins.gpr_d(), value);

        TB_INFO
    }

    pub fn crxor(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let xored = self.bd.ins().bxor(bit_a, bit_b);

        let value = self.set_bit(cr, bit_dest, xored);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn creqv(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let xored = self.bd.ins().bxor(bit_a, bit_b);
        let not = self.bd.ins().bxor_imm(xored, 1);

        let value = self.set_bit(cr, bit_dest, not);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn cror(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let ored = self.bd.ins().bor(bit_a, bit_b);

        let value = self.set_bit(cr, bit_dest, ored);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn crorc(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let not_b = self.bd.ins().bxor_imm(bit_b, 1);
        let ored = self.bd.ins().bor(bit_a, not_b);

        let value = self.set_bit(cr, bit_dest, ored);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn crnor(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let ored = self.bd.ins().bor(bit_a, bit_b);
        let nored = self.bd.ins().bxor_imm(ored, 1);

        let value = self.set_bit(cr, bit_dest, nored);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn crand(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let anded = self.bd.ins().band(bit_a, bit_b);

        let value = self.set_bit(cr, bit_dest, anded);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn crandc(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let not_b = self.bd.ins().bxor_imm(bit_b, 1);
        let anded = self.bd.ins().band(bit_a, not_b);

        let value = self.set_bit(cr, bit_dest, anded);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn crnand(&mut self, ins: Ins) -> InstructionInfo {
        let bit_a = 31 - ins.field_crba();
        let bit_b = 31 - ins.field_crbb();
        let bit_dest = 31 - ins.field_crbd();

        let cr = self.get(Reg::CR);
        let bit_a = self.get_bit(cr, bit_a);
        let bit_b = self.get_bit(cr, bit_b);
        let anded = self.bd.ins().band(bit_a, bit_b);
        let nanded = self.bd.ins().bxor_imm(anded, 1);

        let value = self.set_bit(cr, bit_dest, nanded);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn mcrf(&mut self, ins: Ins) -> InstructionInfo {
        let src_field = 7 - ins.field_crfs();
        let dst_field = 7 - ins.field_crfd();

        // get src
        let cr = self.get(Reg::CR);
        let src = self.bd.ins().ushr_imm(cr, 4 * src_field as u64 as i64);
        let src = self.bd.ins().band_imm(src, 0b1111u64 as i64);

        // place src in dst
        let new = self.bd.ins().ishl_imm(src, 4 * dst_field as u64 as i64);
        let dst_mask = self.ir_value(0b1111 << (4 * dst_field));
        let value = self.bd.ins().bitselect(dst_mask, new, cr);

        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn mcrx(&mut self, ins: Ins) -> InstructionInfo {
        let dst_field = 7 - ins.field_crfd();

        // get src
        let xer = self.get(SPR::XER);
        let src = self.bd.ins().band_imm(xer, 0b1111u64 as i64);
        let new_xer = self.bd.ins().band_imm(xer, !0b1111u64 as i64);

        // place src in dst
        let cr = self.get(Reg::CR);
        let new = self.bd.ins().ishl_imm(src, 4 * dst_field as u64 as i64);
        let dst_mask = self.ir_value(0b1111 << (4 * dst_field));
        let value = self.bd.ins().bitselect(dst_mask, new, cr);

        self.set(SPR::XER, new_xer);
        self.set(Reg::CR, value);

        CR_INFO
    }

    pub fn mtfsb0(&mut self, ins: Ins) -> InstructionInfo {
        let bit = 31 - ins.field_crbd();
        let fpscr = self.get(Reg::FPSCR);

        let value = self.set_bit(fpscr, bit, false);
        self.set(Reg::FPSCR, value);

        self.update_fpscr();

        if ins.field_rc() {
            self.update_cr1_float();
        }

        CR_INFO
    }

    pub fn mtfsb1(&mut self, ins: Ins) -> InstructionInfo {
        let bit = 31 - ins.field_crbd();
        let fpscr = self.get(Reg::FPSCR);

        let value = self.set_bit(fpscr, bit, true);
        self.set(Reg::FPSCR, value);

        self.update_fpscr();

        if ins.field_rc() {
            self.update_cr1_float();
        }

        CR_INFO
    }

    pub fn dcbz(&mut self, ins: Ins) -> InstructionInfo {
        let rb = self.get(ins.gpr_b());
        let addr = if ins.field_ra() == 0 {
            rb
        } else {
            let ra = self.get(ins.gpr_a());
            self.bd.ins().iadd(ra, rb)
        };

        let zero = self.ir_value(0u32);
        let block_start = self.bd.ins().band_imm(addr, !0b11111u64 as i64);
        for i in 0..8 {
            let current = self.bd.ins().iadd_imm(block_start, 4 * i);
            self.mem_store::<i32>(current, zero);
        }

        DCACHE_INFO
    }

    pub fn icbi(&mut self, ins: Ins) -> InstructionInfo {
        let rb = self.get(ins.gpr_b());
        let addr = if ins.field_ra() == 0 {
            rb
        } else {
            let ra = self.get(ins.gpr_a());
            self.bd.ins().iadd(ra, rb)
        };

        self.bd
            .ins()
            .call(self.hooks.inv_icache, &[self.consts.ctx_ptr, addr]);

        INV_ICACHE_INFO
    }

    pub fn isync(&mut self, _: Ins) -> InstructionInfo {
        self.bd
            .ins()
            .call(self.hooks.clear_icache, &[self.consts.ctx_ptr]);

        SYNC_ICACHE_INFO
    }
}
