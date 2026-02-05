use std::ops::Deref;

use gekko::disasm::{Ins, Opcode, ParsedIns};
use gekko::{Address, InsExt};

use crate::block::Pattern;

/// A sequence of PowerPC instructions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct Sequence(pub Vec<Ins>);

impl Sequence {
    fn is_simple_idle_loop(&self) -> bool {
        self.len() == 1 && self[0].code == 0x4800_0000
    }

    fn is_generic_volatile_read(&self) -> bool {
        if self.len() < 3 {
            return false;
        }

        let is_load = matches!(
            self[0].op,
            Opcode::Lbz | Opcode::Lha | Opcode::Lhz | Opcode::Lwz
        );
        let is_cmp_imm = matches!(self[1].op, Opcode::Cmpi | Opcode::Cmpli);
        let is_branch_cond = matches!(self[2].op, Opcode::Bc);
        let load_dst_is_cmp_src = self[0].gpr_d() == self[1].gpr_a();
        let is_rel_jmp_to_start = !self[2].field_aa() && self[2].field_bd() == -8;

        is_load && is_cmp_imm && is_branch_cond && load_dst_is_cmp_src && is_rel_jmp_to_start
    }

    // fn is_call_check_loop(&self) -> bool {
    //     if self.len() < 3 {
    //         return false;
    //     }
    //
    //     let i0_is_branch = matches!(self[0].op, Opcode::B);
    //     let i0_lk = self[0].field_lk();
    //
    //     let i0_is_call = i0_is_branch && i0_lk;
    //
    //     let i1_is_cmpli = matches!(self[1].op, Opcode::Cmpli);
    //     let i1_ra = self[1].gpr_a();
    //     // let i1_crfd = self[1].field_crfd();
    //     // let i1_imm = self[1].field_uimm();
    //
    //     let i1_is_cmp = i1_is_cmpli && i1_ra == GPR::R3;
    //
    //     let i2_is_bc = matches!(self[2].op, Opcode::Bc);
    //     let i2_is_branch_to_start = !self[2].field_aa() && self[2].field_bd() == -8;
    //
    //     let i2_is_check = i2_is_bc && i2_is_branch_to_start;
    //
    //     i0_is_call && i1_is_cmp && i2_is_check
    // }

    fn is_get_mailbox_status_func(&self) -> bool {
        if self.len() != 4 {
            return false;
        }

        let i0_is_addis = matches!(self[0].op, Opcode::Addis);
        let i0_imm = self[0].field_uimm();
        let i0_dst = self[0].gpr_d();

        let i0_is_setting_to_cc00 = i0_is_addis && i0_imm == 0xCC00;

        let i1_is_lhz = matches!(self[1].op, Opcode::Lhz);
        let i1_src = self[1].gpr_a();
        let i1_offset = self[1].field_uimm();

        let i1_is_loading_from_mailbox = i1_is_lhz && i1_src == i0_dst && i1_offset == 0x5000;

        let i2_is_rlwinm = matches!(self[2].op, Opcode::Rlwinm);
        let i2_sh = self[2].field_sh();
        let i2_mb = self[2].field_mb();
        let i2_me = self[2].field_me();

        let i2_is_getting_status = i2_is_rlwinm && i2_sh == 17 && i2_mb == 31 && i2_me == 31;

        let i3_is_branch_lr = matches!(self[3].op, Opcode::Bclr);
        let i3_is_branch_always = self[3].field_bo() == 20;

        let i3_is_return = i3_is_branch_lr && i3_is_branch_always;

        i0_is_setting_to_cc00 && i1_is_loading_from_mailbox && i2_is_getting_status && i3_is_return
    }

    pub fn is_call(&self, pc: Address) -> Option<Address> {
        if self.len() != 1 {
            return None;
        }

        let ins = self[0];
        let is_branch = matches!(ins.op, Opcode::B);
        let links = ins.field_lk();

        let is_call = is_branch && links;
        if !is_call {
            return None;
        }

        Some(if ins.field_aa() {
            Address(ins.field_li() as u32)
        } else {
            Address(pc.0.wrapping_add_signed(ins.field_li()))
        })
    }

    pub fn detect_pattern(&self) -> Pattern {
        if self.is_simple_idle_loop() {
            return Pattern::IdleBasic;
        }

        if self.is_call(Address(0)).is_some() {
            return Pattern::Call;
        }

        if self.is_get_mailbox_status_func() {
            return Pattern::GetMailboxStatusFunc;
        }

        if self.is_generic_volatile_read() {
            return Pattern::IdleVolatileRead;
        }

        Pattern::None
    }
}

impl Deref for Sequence {
    type Target = [Ins];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parsed = ParsedIns::new();
        for ins in &self.0 {
            ins.parse_basic(&mut parsed);
            writeln!(f, "{parsed}")?;
        }

        Ok(())
    }
}

// #[cfg(test)]
// mod test {
//     use crate::Sequence;
//     use gekko::disasm::{Extensions, Ins};
//     use powerpc_asm::{Argument, Arguments, assemble};
//
//     macro_rules! ppc_asm {
//         ($($mnemonic:ident $($arg:expr),*);* $(;)?) => {
//             #[allow(unused_assignments)]
//             {
//                 use Argument::*;
//                 let mut vec = vec![];
//                 $(
//                     let mut i = 0;
//                     let mut args: Arguments = [Argument::None; 5];
//
//                     $(
//                         args[i] = $arg;
//                         i += 1;
//                     )*
//
//                     let code = assemble(stringify!($mnemonic), &args).expect("Invalid arguments");
//                     vec.push(Ins::new(code, Extensions::gekko_broadway()));
//                 )*
//
//                 vec
//             }
//         };
//     }
// }
