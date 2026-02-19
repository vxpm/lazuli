use cranelift::codegen::isa;
use cranelift::prelude::Configurable;

use crate::block::Meta;
use crate::hooks::Hooks;
use crate::{Artifact, CodegenSettings, Jit, Sequence, Settings};

macro_rules! ppc {
    ($($mnemonic:ident $($arg:expr)*);* $(;)?) => {
        {
            let mut sequence = vec![];

            #[allow(unused_variables, unused_mut, unused_assignments, unused_imports, dead_code)]
            {
                use powerpc_asm::Argument;

                fn u(value: u32) -> Argument {
                    Argument::Unsigned(value)
                }

                fn i(value: i32) -> Argument {
                    Argument::Signed(value)
                }

                fn gpr(index: u32) -> Argument {
                    u(index)
                }

                fn fpr(index: u32) -> Argument {
                    u(index)
                }

                fn off(value: i32) -> Argument {
                    i(value)
                }

                $(
                    let mut i = 0;
                    let mut arguments = [Argument::None; 5];

                    $(
                        arguments[i] = $arg;
                        i += 1;
                    )*

                    let ins = gekko::disasm::Ins::new(
                        powerpc_asm::assemble(stringify!($mnemonic), &arguments).unwrap(),
                        gekko::disasm::Extensions::gekko_broadway(),
                    );

                    sequence.push(ins);
                )*
            }

            Sequence(sequence)
        }
    };
}

fn compile_sequence(sequence: Sequence) -> (Artifact, Meta) {
    let mut isa = isa::lookup_by_name("x86_64").expect("tests should compile for x86_64");

    isa.enable("has_sse3").unwrap();
    isa.enable("has_ssse3").unwrap();
    isa.enable("has_sse41").unwrap();
    isa.enable("has_sse42").unwrap();
    isa.enable("has_fma").unwrap();
    isa.enable("has_lzcnt").unwrap();
    isa.enable("has_popcnt").unwrap();
    isa.enable("has_bmi1").unwrap();
    isa.enable("has_bmi2").unwrap();
    isa.enable("has_avx").unwrap();
    isa.enable("has_avx2").unwrap();

    let mut jit = Jit::with_isa(
        isa,
        Settings {
            codegen: CodegenSettings {
                nop_syscalls: false,
                force_fpu: false,
                ignore_unimplemented: false,
                round_to_single: false,
            },
            cache_path: None,
        },
        unsafe { Hooks::stub() },
    );

    jit.build_artifact(sequence.0.into_iter()).unwrap()
}

fn test_sequence(name: &str, sequence: Sequence) {
    let (artifact, meta) = compile_sequence(sequence);
    let clir = meta.clir.unwrap();
    let disasm = artifact.disasm.unwrap();
    insta::assert_snapshot!(format!("{}_clir", name), clir);
    insta::assert_snapshot!(format!("{}_disasm", name), disasm);
}

#[test]
fn fcmpu() {
    test_sequence(
        "fcmpu",
        ppc! {
            fcmpu u(0) fpr(1) fpr(2)
        },
    );
}

#[test]
fn ps_add_acc() {
    test_sequence(
        "ps_add_acc",
        ppc! {
            ps_add fpr(0) fpr(0) fpr(1);
            ps_add fpr(0) fpr(0) fpr(2);
            ps_add fpr(0) fpr(0) fpr(3);
            ps_add fpr(0) fpr(0) fpr(4);
        },
    );
}

#[test]
fn gu_vec_scale() {
    // ps_guVecScale:
    // 	psq_l		fr2,0(r3),0,0
    // 	psq_l		fr3,8(r3),1,0
    // 	ps_muls0	fr4,fr2,fr1
    // 	psq_st		fr4,0(r4),0,0
    // 	ps_muls0	fr4,fr3,fr1
    // 	psq_st		fr4,8(r4),1,0

    test_sequence(
        "gu_vec_scale",
        ppc! {
            psq_l fpr(2) off(0) gpr(3) u(0) u(0);
            psq_l fpr(3) off(8) gpr(3) u(1) u(0);
            ps_muls0 fpr(4) fpr(2) fpr(1);
            psq_st fpr(4) off(0) gpr(4) u(0) u(0);
            ps_muls0 fpr(4) fpr(3) fpr(1);
            psq_st fpr(4) off(8) gpr(4) u(0) u(0);
        },
    );
}

#[test]
fn gu_vec_add() {
    // #define V1_XY	fr2
    // #define V1_Z		fr3
    // #define V2_XY	fr4
    // #define V2_Z		fr5
    // #define D1_XY	fr6
    // #define D1_Z		fr7
    // #define D2_XY	fr8
    // #define D2_Z		fr9
    //
    // ps_guVecAdd:
    // 	psq_l		V1_XY,0(r3),0,0
    // 	psq_l		V2_XY,0(r4),0,0
    // 	ps_add		D1_XY,V1_XY,V2_XY
    // 	psq_st		D1_XY,0(r5),0,0
    // 	psq_l		V1_Z,8(r3),1,0
    // 	psq_l		V2_Z,8(r4),1,0
    // 	ps_add		D1_Z,V1_Z,V2_Z
    // 	psq_st		D1_Z,8(r5),1,0

    test_sequence(
        "gu_vec_add",
        ppc! {
            psq_l fpr(2) off(0) gpr(3) u(0) u(0);
            psq_l fpr(4) off(0) gpr(4) u(0) u(0);
            ps_add fpr(6) fpr(2) fpr(4);
            psq_st fpr(6) off(0) gpr(5) u(0) u(0);
            psq_l fpr(3) off(8) gpr(3) u(1) u(0);
            psq_l fpr(5) off(8) gpr(4) u(1) u(0);
            ps_add fpr(7) fpr(3) fpr(5);
            psq_st fpr(7) off(8) gpr(5) u(1) u(0);
        },
    );
}

#[test]
fn gu_mtx_identity() {
    // ps_guMtxIdentity:
    // 	lfs		fr0,Unit01@sdarel(r13)
    // 	lfs		fr1,Unit01+4@sdarel(r13)
    // 	psq_st		fr0,8(r3),0,0
    // 	ps_merge01	fr2,fr0,fr1
    // 	psq_st		fr0,24(r3),0,0
    // 	ps_merge10	fr3,fr1,fr0
    // 	psq_st		fr0,32(r3),0,0
    // 	psq_st		fr2,16(r3),0,0
    // 	psq_st		fr3,0(r3),0,0
    // 	psq_st		fr3,40(r3),0,0

    test_sequence(
        "gu_mtx_identity",
        ppc! {
            lfs fpr(0) off(0) gpr(31);
            lfs fpr(1) off(4) gpr(31);
            psq_st fpr(0) off(8) gpr(3) u(0) u(0);
            ps_merge01 fpr(2) fpr(0) fpr(1);
            psq_st fpr(0) off(24) gpr(3) u(0) u(0);
            ps_merge10 fpr(3) fpr(1) fpr(0);
            psq_st fpr(0) off(32) gpr(3) u(0) u(0);
            psq_st fpr(2) off(16) gpr(3) u(0) u(0);
            psq_st fpr(3) off(0) gpr(3) u(0) u(0);
            psq_st fpr(3) off(40) gpr(3) u(0) u(0);
        },
    );
}
