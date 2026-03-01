mod arithmetic;
mod branch;
mod compare;
mod exception;
mod floating;
mod logic;
mod memory;
mod others;
mod util;

use std::mem::offset_of;

use cranelift::codegen::ir;
use cranelift::frontend;
use cranelift::prelude::InstBuilder;
use easyerr::Error;
use gekko::disasm::{Ins, Opcode};
use gekko::{Reg, SPR};
use rustc_hash::FxHashMap;

use crate::block::Info;
use crate::builder::util::IntoIrValue;
use crate::hooks::{HookKind, Hooks};
use crate::{
    Codegen, INTERNAL_RAISE_EXCEPTION, NAMESPACE_INTERNALS, NAMESPACE_USER_HOOKS, Sequence,
};

const MEMFLAGS: ir::MemFlags = ir::MemFlags::trusted();
const MEMFLAGS_READONLY: ir::MemFlags = MEMFLAGS.with_can_move().with_readonly();

// NOTE: make sure to keep this up to date if anything else is not just 32 bits
fn reg_ir_ty(reg: Reg) -> ir::Type {
    match reg {
        Reg::FPR(_) => ir::types::F64X2,
        _ => ir::types::I32,
    }
}

fn is_cacheable(reg: Reg) -> bool {
    match reg {
        Reg::MSR => false,
        Reg::SPR(spr) => match spr {
            SPR::LR
            | SPR::DEC
            | SPR::TBL
            | SPR::TBU
            | SPR::WPAR
            | SPR::DMAL
            | SPR::DMAU
            | SPR::SRR0
            | SPR::SRR1
            | SPR::DAR => false,
            spr if spr.is_bat() => false,
            spr if spr.is_gqr() => false,
            _ => true,
        },
        _ => true,
    }
}

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("illegal instruction {f0:?}")]
    Illegal(Ins),
    #[error("unimplemented instruction {f0:?}")]
    Unimplemented(Ins),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Continue emitting instructions.
    Continue,
    /// Flush registers and emit the prologue (returns).
    FlushAndPrologue,
    /// Just emit the prologue (returns).
    Prologue,
    /// Just return, no need to do anything else.
    Finish,
}

#[derive(Clone, Copy)]
pub(crate) struct InstructionInfo {
    cycles: u8,
    auto_pc: bool,
    action: Action,
}

struct Signatures {
    block: ir::SigRef,

    follow_link_hook: ir::SigRef,
    try_link_hook: ir::SigRef,
    read_i8_hook: ir::SigRef,
    read_i16_hook: ir::SigRef,
    read_i32_hook: ir::SigRef,
    read_i64_hook: ir::SigRef,
    write_i8_hook: ir::SigRef,
    write_i16_hook: ir::SigRef,
    write_i32_hook: ir::SigRef,
    write_i64_hook: ir::SigRef,
    read_quant_hook: ir::SigRef,
    write_quant_hook: ir::SigRef,
    invalidate_icache_hook: ir::SigRef,
    generic_hook: ir::SigRef,

    raise_exception: ir::SigRef,
}

struct HookFuncs {
    follow_link: ir::FuncRef,
    try_link: ir::FuncRef,
    read_i8: ir::FuncRef,
    read_i16: ir::FuncRef,
    read_i32: ir::FuncRef,
    read_i64: ir::FuncRef,
    write_i8: ir::FuncRef,
    write_i16: ir::FuncRef,
    write_i32: ir::FuncRef,
    write_i64: ir::FuncRef,
    read_quant: ir::FuncRef,
    write_quant: ir::FuncRef,
    inv_icache: ir::FuncRef,

    // generic
    clear_icache: ir::FuncRef,
    dcache_dma: ir::FuncRef,
    msr_changed: ir::FuncRef,
    ibat_changed: ir::FuncRef,
    dbat_changed: ir::FuncRef,
    tb_read: ir::FuncRef,
    tb_changed: ir::FuncRef,
    dec_read: ir::FuncRef,
    dec_changed: ir::FuncRef,

    // special
    raise_exception: ir::FuncRef,
}

/// Constants used through block building.
struct Consts {
    ptr_type: ir::Type,

    info_ptr: ir::Value,
    ctx_ptr: ir::Value,
    regs_ptr: ir::Value,
    fmem_ptr: ir::Value,

    read_stack_slot: ir::StackSlot,
    signatures: Signatures,
}

/// A cached value.
struct CachedValue {
    value: ir::Value,
    modified: bool,
}

/// Structure to build JIT blocks.
pub struct BlockBuilder<'ctx> {
    codegen: &'ctx mut Codegen,
    bd: frontend::FunctionBuilder<'ctx>,
    cache: FxHashMap<Reg, CachedValue>,
    consts: Consts,
    hooks: HookFuncs,
    current_bb: ir::Block,

    executed_cycles: u32,
    executed_instructions: u32,

    link_index: u32,
    last_updated_cycles: u32,
    last_updated_instructions: u32,

    ibat_changed: bool,
    dbat_changed: bool,
    floats_checked: bool,
}

impl<'ctx> BlockBuilder<'ctx> {
    pub fn new(codegen: &'ctx mut Codegen, mut builder: frontend::FunctionBuilder<'ctx>) -> Self {
        let entry_bb = builder.create_block();
        builder.append_block_params_for_function_params(entry_bb);
        builder.switch_to_block(entry_bb);
        builder.seal_block(entry_bb);

        let read_stack_slot = builder.create_sized_stack_slot(ir::StackSlotData::new(
            ir::StackSlotKind::ExplicitSlot,
            size_of::<u64>() as u32,
            align_of::<u64>().ilog2() as u8,
        ));

        let ptr_type = codegen.isa.pointer_type();
        let default = codegen.isa.default_call_conv();
        let params = builder.block_params(entry_bb);
        let info_ptr = params[0];
        let ctx_ptr = params[1];
        let regs_ptr = params[2];
        let fmem_ptr = params[3];

        let sigs = Signatures {
            block: builder.import_signature(builder.func.signature.clone()),

            follow_link_hook: builder.import_signature(Hooks::follow_link_sig(ptr_type, default)),
            try_link_hook: builder.import_signature(Hooks::try_link_sig(ptr_type, default)),
            read_i8_hook: builder.import_signature(Hooks::read_sig(
                ptr_type,
                ir::types::I8,
                default,
            )),
            read_i16_hook: builder.import_signature(Hooks::read_sig(
                ptr_type,
                ir::types::I16,
                default,
            )),
            read_i32_hook: builder.import_signature(Hooks::read_sig(
                ptr_type,
                ir::types::I32,
                default,
            )),
            read_i64_hook: builder.import_signature(Hooks::read_sig(
                ptr_type,
                ir::types::I64,
                default,
            )),
            write_i8_hook: builder.import_signature(Hooks::write_sig(
                ptr_type,
                ir::types::I8,
                default,
            )),
            write_i16_hook: builder.import_signature(Hooks::write_sig(
                ptr_type,
                ir::types::I16,
                default,
            )),
            write_i32_hook: builder.import_signature(Hooks::write_sig(
                ptr_type,
                ir::types::I32,
                default,
            )),
            write_i64_hook: builder.import_signature(Hooks::write_sig(
                ptr_type,
                ir::types::I64,
                default,
            )),
            read_quant_hook: builder.import_signature(Hooks::read_quantized_sig(ptr_type, default)),
            write_quant_hook: builder
                .import_signature(Hooks::write_quantized_sig(ptr_type, default)),
            invalidate_icache_hook: builder
                .import_signature(Hooks::invalidate_icache_sig(ptr_type, default)),
            generic_hook: builder.import_signature(Hooks::generic_hook_sig(ptr_type, default)),

            raise_exception: builder
                .import_signature(exception::raise_exception_sig(ptr_type, default)),
        };

        let raise_exception = {
            let name = builder
                .func
                .declare_imported_user_function(ir::UserExternalName::new(
                    NAMESPACE_INTERNALS,
                    INTERNAL_RAISE_EXCEPTION,
                ));

            builder.import_function(ir::ExtFuncData {
                name: ir::ExternalName::User(name),
                signature: sigs.raise_exception,
                colocated: false,
                patchable: false,
            })
        };

        let mut hook = |sig, kind| {
            let name = builder
                .func
                .declare_imported_user_function(ir::UserExternalName::new(
                    NAMESPACE_USER_HOOKS,
                    kind as u32,
                ));

            builder.import_function(ir::ExtFuncData {
                name: ir::ExternalName::User(name),
                signature: sig,
                colocated: false,
                patchable: false,
            })
        };

        let hooks = HookFuncs {
            follow_link: hook(sigs.follow_link_hook, HookKind::FollowLink),
            try_link: hook(sigs.try_link_hook, HookKind::TryLink),
            read_i8: hook(sigs.read_i8_hook, HookKind::ReadI8),
            read_i16: hook(sigs.read_i16_hook, HookKind::ReadI16),
            read_i32: hook(sigs.read_i32_hook, HookKind::ReadI32),
            read_i64: hook(sigs.read_i64_hook, HookKind::ReadI64),
            write_i8: hook(sigs.write_i8_hook, HookKind::WriteI8),
            write_i16: hook(sigs.write_i16_hook, HookKind::WriteI16),
            write_i32: hook(sigs.write_i32_hook, HookKind::WriteI32),
            write_i64: hook(sigs.write_i64_hook, HookKind::WriteI64),
            read_quant: hook(sigs.read_quant_hook, HookKind::ReadQuant),
            write_quant: hook(sigs.write_quant_hook, HookKind::WriteQuant),
            inv_icache: hook(sigs.invalidate_icache_hook, HookKind::InvICache),
            clear_icache: hook(sigs.generic_hook, HookKind::ClearICache),
            dcache_dma: hook(sigs.generic_hook, HookKind::DCacheDma),
            msr_changed: hook(sigs.generic_hook, HookKind::MsrChanged),
            ibat_changed: hook(sigs.generic_hook, HookKind::IBatChanged),
            dbat_changed: hook(sigs.generic_hook, HookKind::DBatChanged),
            tb_read: hook(sigs.generic_hook, HookKind::TbRead),
            tb_changed: hook(sigs.generic_hook, HookKind::TbChanged),
            dec_read: hook(sigs.generic_hook, HookKind::DecRead),
            dec_changed: hook(sigs.generic_hook, HookKind::DecChanged),
            raise_exception,
        };

        let consts = Consts {
            ptr_type,

            info_ptr,
            ctx_ptr,
            regs_ptr,
            fmem_ptr,

            read_stack_slot,

            signatures: sigs,
        };

        Self {
            codegen,
            bd: builder,
            cache: FxHashMap::default(),
            consts,
            hooks,
            current_bb: entry_bb,

            link_index: 0,
            executed_cycles: 0,
            executed_instructions: 0,

            last_updated_cycles: 0,
            last_updated_instructions: 0,

            ibat_changed: false,
            dbat_changed: false,
            floats_checked: false,
        }
    }

    fn switch_to_bb(&mut self, bb: ir::Block) {
        self.bd.switch_to_block(bb);
        self.bd
            .set_srcloc(ir::SourceLoc::new(self.executed_instructions));
        self.current_bb = bb;
    }

    fn load_reg(&mut self, reg: Reg) -> ir::Value {
        let reg_ty = reg_ir_ty(reg);
        self.bd
            .ins()
            .load(reg_ty, MEMFLAGS, self.consts.regs_ptr, reg.offset() as i32)
    }

    fn store_reg(&mut self, reg: Reg, value: ir::Value) {
        self.bd
            .ins()
            .store(MEMFLAGS, value, self.consts.regs_ptr, reg.offset() as i32);
    }

    /// Gets the current value of the given register.
    fn get(&mut self, reg: impl Into<Reg>) -> ir::Value {
        let reg = reg.into();

        if let Some(reg) = self.cache.get(&reg) {
            return reg.value;
        }

        let dumped = self.load_reg(reg);
        if is_cacheable(reg) {
            self.cache.insert(
                reg,
                CachedValue {
                    value: dumped,
                    modified: false,
                },
            );
        }

        dumped
    }

    /// Sets the value of the given register.
    fn set(&mut self, reg: impl Into<Reg>, value: impl IntoIrValue) {
        let reg = reg.into();
        let value = self.ir_value(value);

        let value_ty = self.bd.func.dfg.value_type(value);
        match reg {
            Reg::FPR(_) => assert_eq!(value_ty, ir::types::F64X2),
            _ => assert_eq!(value_ty, ir::types::I32),
        }

        if let Some(reg) = self.cache.get_mut(&reg) {
            reg.value = value;
            reg.modified = true;
            return;
        }

        if is_cacheable(reg) {
            self.cache.insert(
                reg,
                CachedValue {
                    value,
                    modified: true,
                },
            );
        } else {
            self.store_reg(reg, value);
        }
    }

    /// Flushes the register cache to the registers struct. This does not invalidate the register
    /// cache.
    fn flush(&mut self) {
        for (reg, val) in &self.cache {
            if !val.modified {
                continue;
            }

            self.bd.ins().store(
                MEMFLAGS,
                val.value,
                self.consts.regs_ptr,
                reg.offset() as i32,
            );
        }
    }

    /// Updates the Info struct.
    fn update_info(&mut self) {
        let cycles_delta = self.executed_cycles as i32 - self.last_updated_cycles as i32;
        let instruction_delta =
            self.executed_instructions as i32 - self.last_updated_instructions as i32;

        if cycles_delta == 0 && instruction_delta == 0 {
            return;
        }

        let instructions = self.bd.ins().load(
            ir::types::I32,
            MEMFLAGS,
            self.consts.info_ptr,
            offset_of!(Info, instructions) as i32,
        );
        let instructions = self
            .bd
            .ins()
            .iadd_imm(instructions, instruction_delta as i64);
        self.bd.ins().store(
            MEMFLAGS,
            instructions,
            self.consts.info_ptr,
            offset_of!(Info, instructions) as i32,
        );

        let cycles = self.bd.ins().load(
            ir::types::I32,
            MEMFLAGS,
            self.consts.info_ptr,
            offset_of!(Info, cycles) as i32,
        );
        let cycles = self.bd.ins().iadd_imm(cycles, cycles_delta as i64);
        self.bd.ins().store(
            MEMFLAGS,
            cycles,
            self.consts.info_ptr,
            offset_of!(Info, cycles) as i32,
        );

        self.last_updated_cycles = self.executed_cycles;
        self.last_updated_instructions = self.executed_instructions;
    }

    /// Calls a generic context hook.
    fn call_generic_hook(&mut self, hook: ir::FuncRef) {
        self.bd.ins().call(hook, &[self.consts.ctx_ptr]);
    }

    /// Emits the prologue:
    /// - Call BAT hooks if they were changed
    /// - Returns
    fn prologue(&mut self) {
        self.update_info();

        if self.dbat_changed {
            self.call_generic_hook(self.hooks.dbat_changed);
        }

        if self.ibat_changed {
            self.call_generic_hook(self.hooks.ibat_changed);
        }

        self.bd.ins().return_(&[]);
        self.bd
            .set_srcloc(ir::SourceLoc::new(self.executed_instructions));
    }

    /// Calls [`prologue`] as if an instruction with `info` had been executed.
    fn prologue_with(&mut self, info: InstructionInfo) {
        self.executed_instructions += 1;
        self.executed_cycles += info.cycles as u32;

        self.prologue();

        self.executed_instructions -= 1;
        self.executed_cycles -= info.cycles as u32;
    }

    /// Emits the given instruction into the block.
    fn emit(&mut self, ins: Ins) -> Result<Action, BuilderError> {
        self.bd
            .set_srcloc(ir::SourceLoc::new(self.executed_instructions));
        let info: InstructionInfo = match ins.op {
            Opcode::Add => self.add(ins),
            Opcode::Addc => self.addc(ins),
            Opcode::Adde => self.adde(ins),
            Opcode::Addi => self.addi(ins),
            Opcode::Addic => self.addic(ins),
            Opcode::Addic_ => self.addic_record(ins),
            Opcode::Addis => self.addis(ins),
            Opcode::Addme => self.addme(ins),
            Opcode::Addze => self.addze(ins),
            Opcode::And => self.and(ins),
            Opcode::Andc => self.andc(ins),
            Opcode::Andi_ => self.andi_record(ins),
            Opcode::Andis_ => self.andis_record(ins),
            Opcode::B => self.b(ins),
            Opcode::Bc => self.bc(ins),
            Opcode::Bcctr => self.bcctr(ins),
            Opcode::Bclr => self.bclr(ins),
            Opcode::Cmp => self.cmp(ins),
            Opcode::Cmpi => self.cmpi(ins),
            Opcode::Cmpl => self.cmpl(ins),
            Opcode::Cmpli => self.cmpli(ins),
            Opcode::Cntlzw => self.cntlzw(ins),
            Opcode::Crand => self.crand(ins),
            Opcode::Crandc => self.crandc(ins),
            Opcode::Creqv => self.creqv(ins),
            Opcode::Crnand => self.crnand(ins),
            Opcode::Crnor => self.crnor(ins),
            Opcode::Cror => self.cror(ins),
            Opcode::Crorc => self.crorc(ins),
            Opcode::Crxor => self.crxor(ins),
            Opcode::Dcbf => self.nop(Action::Continue),
            Opcode::Dcbi => self.nop(Action::Continue),
            Opcode::Dcbst => self.nop(Action::Continue),
            Opcode::Dcbt => self.nop(Action::Continue),
            Opcode::Dcbtst => self.nop(Action::Continue),
            Opcode::Dcbz => self.dcbz(ins),
            Opcode::DcbzL => self.stub(ins),
            Opcode::Divw => self.divw(ins),
            Opcode::Divwu => self.divwu(ins),
            Opcode::Eqv => self.eqv(ins),
            Opcode::Extsb => self.extsb(ins),
            Opcode::Extsh => self.extsh(ins),
            Opcode::Fabs => self.fabs(ins),
            Opcode::Fadd => self.fadd(ins),
            Opcode::Fadds => self.fadds(ins),
            Opcode::Fcmpo => self.fcmpo(ins),
            Opcode::Fcmpu => self.fcmpu(ins),
            Opcode::Fctiw => self.fctiw(ins),
            Opcode::Fctiwz => self.fctiwz(ins),
            Opcode::Fdiv => self.fdiv(ins),
            Opcode::Fdivs => self.fdivs(ins),
            Opcode::Fmadd => self.fmadd(ins),
            Opcode::Fmadds => self.fmadds(ins),
            Opcode::Fmr => self.fmr(ins),
            Opcode::Fmsub => self.fmsub(ins),
            Opcode::Fmsubs => self.fmsubs(ins),
            Opcode::Fmul => self.fmul(ins),
            Opcode::Fmuls => self.fmuls(ins),
            Opcode::Fneg => self.fneg(ins),
            Opcode::Fnmadd => self.fnmadd(ins),
            Opcode::Fnmadds => self.fnmadds(ins),
            Opcode::Fnmsub => self.fnmsub(ins),
            Opcode::Fnmsubs => self.fnmsubs(ins),
            Opcode::Fres => self.fres(ins),
            Opcode::Frsp => self.frsp(ins),
            Opcode::Frsqrte => self.frsqrte(ins),
            Opcode::Fsel => self.fsel(ins),
            Opcode::Fsub => self.fsub(ins),
            Opcode::Fsubs => self.fsubs(ins),
            Opcode::Icbi => self.icbi(ins),
            Opcode::Isync => self.isync(ins),
            Opcode::Lbz => self.lbz(ins),
            Opcode::Lbzu => self.lbzu(ins),
            Opcode::Lbzux => self.lbzux(ins),
            Opcode::Lbzx => self.lbzx(ins),
            Opcode::Lfd => self.lfd(ins),
            Opcode::Lfdu => self.lfdu(ins),
            Opcode::Lfdux => self.lfdux(ins),
            Opcode::Lfdx => self.lfdx(ins),
            Opcode::Lfs => self.lfs(ins),
            Opcode::Lfsu => self.lfsu(ins),
            Opcode::Lfsux => self.lfsux(ins),
            Opcode::Lfsx => self.lfsx(ins),
            Opcode::Lha => self.lha(ins),
            Opcode::Lhau => self.lhau(ins),
            Opcode::Lhaux => self.lhaux(ins),
            Opcode::Lhax => self.lhax(ins),
            Opcode::Lhbrx => self.lhbrx(ins),
            Opcode::Lhz => self.lhz(ins),
            Opcode::Lhzu => self.lhzu(ins),
            Opcode::Lhzux => self.lhzux(ins),
            Opcode::Lhzx => self.lhzx(ins),
            Opcode::Lmw => self.lmw(ins),
            Opcode::Lswi => self.lswi(ins),
            Opcode::Lwarx => self.lwzx(ins), // NOTE: same behaviour
            Opcode::Lwbrx => self.lwbrx(ins),
            Opcode::Lwz => self.lwz(ins),
            Opcode::Lwzu => self.lwzu(ins),
            Opcode::Lwzux => self.lwzux(ins),
            Opcode::Lwzx => self.lwzx(ins),
            Opcode::Mcrf => self.mcrf(ins),
            Opcode::Mcrxr => self.mcrx(ins),
            Opcode::Mfcr => self.mfcr(ins),
            Opcode::Mffs => self.mffs(ins),
            Opcode::Mfmsr => self.mfmsr(ins),
            Opcode::Mfspr => self.mfspr(ins),
            Opcode::Mfsr => self.mfsr(ins),
            Opcode::Mftb => self.mftb(ins),
            Opcode::Mtcrf => self.mtcrf(ins),
            Opcode::Mtfsb0 => self.mtfsb0(ins),
            Opcode::Mtfsb1 => self.mtfsb1(ins),
            Opcode::Mtfsf => self.mtfsf(ins),
            Opcode::Mtmsr => self.mtmsr(ins),
            Opcode::Mtspr => self.mtspr(ins),
            Opcode::Mtsr => self.mtsr(ins),
            Opcode::Mulhw => self.mulhw(ins),
            Opcode::Mulhwu => self.mulhwu(ins),
            Opcode::Mulli => self.mulli(ins),
            Opcode::Mullw => self.mullw(ins),
            Opcode::Nand => self.nand(ins),
            Opcode::Neg => self.neg(ins),
            Opcode::Nor => self.nor(ins),
            Opcode::Or => self.or(ins),
            Opcode::Orc => self.orc(ins),
            Opcode::Ori => self.ori(ins),
            Opcode::Oris => self.oris(ins),
            Opcode::PsAdd => self.ps_add(ins),
            Opcode::PsCmpo0 => self.ps_cmpo0(ins),
            Opcode::PsCmpo1 => self.ps_cmpo1(ins),
            Opcode::PsCmpu0 => self.ps_cmpu0(ins),
            Opcode::PsCmpu1 => self.ps_cmpu1(ins),
            Opcode::PsDiv => self.ps_div(ins),
            Opcode::PsMadd => self.ps_madd(ins),
            Opcode::PsMadds0 => self.ps_madds0(ins),
            Opcode::PsMadds1 => self.ps_madds1(ins),
            Opcode::PsMerge00 => self.ps_merge00(ins),
            Opcode::PsMerge01 => self.ps_merge01(ins),
            Opcode::PsMerge10 => self.ps_merge10(ins),
            Opcode::PsMerge11 => self.ps_merge11(ins),
            Opcode::PsMr => self.ps_mr(ins),
            Opcode::PsMsub => self.ps_msub(ins),
            Opcode::PsMul => self.ps_mul(ins),
            Opcode::PsMuls0 => self.ps_muls0(ins),
            Opcode::PsMuls1 => self.ps_muls1(ins),
            Opcode::PsNeg => self.ps_neg(ins),
            Opcode::PsNmadd => self.ps_nmadd(ins),
            Opcode::PsNmsub => self.ps_nmsub(ins),
            Opcode::PsRes => self.ps_res(ins),
            Opcode::PsRsqrte => self.ps_rsqrte(ins),
            Opcode::PsSel => self.ps_sel(ins),
            Opcode::PsSub => self.ps_sub(ins),
            Opcode::PsSum0 => self.ps_sum0(ins),
            Opcode::PsSum1 => self.ps_sum1(ins),
            Opcode::PsqL => self.psq_l(ins),
            Opcode::PsqLu => self.psq_lu(ins),
            Opcode::PsqLx => self.psq_lx(ins),
            Opcode::PsqSt => self.psq_st(ins),
            Opcode::PsqStu => self.psq_stu(ins),
            Opcode::PsqStx => self.psq_stx(ins),
            Opcode::Rfi => self.rfi(ins),
            Opcode::Rlwimi => self.rlwimi(ins),
            Opcode::Rlwinm => self.rlwinm(ins),
            Opcode::Rlwnm => self.rlwnm(ins),
            Opcode::Sc => self.sc(ins),
            Opcode::Slw => self.slw(ins),
            Opcode::Sraw => self.sraw(ins),
            Opcode::Srawi => self.srawi(ins),
            Opcode::Srw => self.srw(ins),
            Opcode::Stb => self.stb(ins),
            Opcode::Stbu => self.stbu(ins),
            Opcode::Stbux => self.stbux(ins),
            Opcode::Stbx => self.stbx(ins),
            Opcode::Stfd => self.stfd(ins),
            Opcode::Stfdu => self.stfdu(ins),
            Opcode::Stfdux => self.stfdux(ins),
            Opcode::Stfdx => self.stfdx(ins),
            Opcode::Stfiwx => self.stfiwx(ins),
            Opcode::Stfs => self.stfs(ins),
            Opcode::Stfsu => self.stfsu(ins),
            Opcode::Stfsux => self.stfsux(ins),
            Opcode::Stfsx => self.stfsx(ins),
            Opcode::Sth => self.sth(ins),
            Opcode::Sthbrx => self.sthbrx(ins),
            Opcode::Sthu => self.sthu(ins),
            Opcode::Sthux => self.sthux(ins),
            Opcode::Sthx => self.sthx(ins),
            Opcode::Stmw => self.stmw(ins),
            Opcode::Stswi => self.stswi(ins),
            Opcode::Stw => self.stw(ins),
            Opcode::Stwbrx => self.stwbrx(ins),
            Opcode::Stwcx_ => self.stwcx(ins),
            Opcode::Stwu => self.stwu(ins),
            Opcode::Stwux => self.stwux(ins),
            Opcode::Stwx => self.stwx(ins),
            Opcode::Subf => self.subf(ins),
            Opcode::Subfc => self.subfc(ins),
            Opcode::Subfe => self.subfe(ins),
            Opcode::Subfic => self.subfic(ins),
            Opcode::Subfme => self.subfme(ins),
            Opcode::Subfze => self.subfze(ins),
            Opcode::Sync => self.nop(Action::FlushAndPrologue),
            Opcode::Tlbie => self.nop(Action::Continue),
            Opcode::Tlbsync => self.nop(Action::Continue),
            Opcode::Xor => self.xor(ins),
            Opcode::Xori => self.xori(ins),
            Opcode::Xoris => self.xoris(ins),
            Opcode::Illegal => {
                if self.codegen.settings.ignore_unimplemented {
                    self.stub(ins)
                } else {
                    return Err(BuilderError::Illegal(ins));
                }
            }
            _ => {
                if self.codegen.settings.ignore_unimplemented {
                    self.stub(ins)
                } else {
                    todo!("unimplemented instruction {ins:?}")
                }
            }
        };

        self.executed_instructions += 1;
        self.executed_cycles += info.cycles as u32;

        if info.auto_pc {
            let old_pc = self.get(Reg::PC);
            let new_pc = self.bd.ins().iadd_imm(old_pc, 4);
            self.set(Reg::PC, new_pc);
        }

        Ok(info.action)
    }

    pub fn build(
        mut self,
        mut instructions: impl Iterator<Item = Ins>,
    ) -> Result<(Sequence, u32), BuilderError> {
        let mut sequence = Sequence::default();
        loop {
            let Some(ins) = instructions.next() else {
                self.bd.set_srcloc(ir::SourceLoc::new(u32::MAX));
                self.flush();
                self.prologue();
                self.bd.finalize();
                break;
            };

            sequence.0.push(ins);

            match self.emit(ins)? {
                Action::Continue => (),
                Action::FlushAndPrologue => {
                    self.bd.set_srcloc(ir::SourceLoc::new(u32::MAX));
                    self.flush();
                    self.prologue();
                    self.bd.finalize();
                    break;
                }
                Action::Prologue => {
                    self.bd.set_srcloc(ir::SourceLoc::new(u32::MAX));
                    self.prologue();
                    self.bd.finalize();
                    break;
                }
                Action::Finish => {
                    self.bd.set_srcloc(ir::SourceLoc::new(u32::MAX));
                    self.bd.finalize();
                    break;
                }
            }
        }

        Ok((sequence, self.executed_cycles))
    }
}
