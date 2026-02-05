#![feature(debug_closure_helpers)]

mod builder;
mod cache;
mod module;
mod sequence;
mod unwind;

#[cfg(test)]
mod test;

pub mod block;
pub mod hooks;

use std::alloc::Layout;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;

use cranelift::codegen::binemit::Reloc;
use cranelift::codegen::{self, ir};
use cranelift::prelude::isa::TargetIsa;
use cranelift::prelude::isa::unwind::UnwindInfo;
use cranelift::prelude::{Configurable, InstBuilder};
use cranelift::{frontend, native};
use cranelift_codegen::entity::PrimaryMap;
use cranelift_codegen::ir::{UserExternalName, UserExternalNameRef};
use cranelift_codegen::{FinalizedMachReloc, FinalizedRelocTarget};
use easyerr::{Error, ResultExt};
use gekko::disasm::Ins;
use gekko::{Cpu, Exception};
use serde::{Deserialize, Serialize};

use crate::block::{BlockFn, Info, LinkData, Meta, Trampoline};
use crate::builder::BlockBuilder;
use crate::cache::{ArtifactKey, Cache};
use crate::hooks::{Context, HookKind, Hooks};
use crate::module::Module;
use crate::unwind::UnwindHandle;

#[rustfmt::skip]
pub use crate::{
    block::Block,
    sequence::Sequence,
};

#[derive(Debug, Clone, PartialEq, Default, Hash)]
pub struct CodegenSettings {
    /// Whether to treat `sc` instructions as no-ops.
    pub nop_syscalls: bool,
    /// Whether to ignore the FPU enabled bit in MSR.
    pub force_fpu: bool,
    /// Whether to ignore unimplemented instructions instead of panicking.
    pub ignore_unimplemented: bool,
    /// Whether to perform round to single operations.
    pub round_to_single: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Codegen settings
    pub codegen: CodegenSettings,
    /// Path to the block cache directory
    pub cache_path: Option<PathBuf>,
}

pub const FASTMEM_LUT_COUNT: usize = 1 << 15;
pub type FastmemLut = [Option<NonNull<u8>>; FASTMEM_LUT_COUNT];

const NAMESPACE_USER_HOOKS: u32 = 0;
const NAMESPACE_INTERNALS: u32 = 1;
const NAMESPACE_LINK_DATA: u32 = 2;

const INTERNAL_RAISE_EXCEPTION: u32 = 0;

struct Codegen {
    settings: CodegenSettings,
    hooks: Hooks,
    isa: Arc<dyn TargetIsa>,
    module: Module,
}

impl Codegen {
    fn new(isa: codegen::isa::Builder, settings: CodegenSettings, hooks: Hooks) -> Self {
        let verifier = if cfg!(debug_assertions) {
            "true"
        } else {
            "false"
        };

        let mut codegen = codegen::settings::builder();
        codegen.set("preserve_frame_pointers", "true").unwrap();
        codegen.set("use_colocated_libcalls", "false").unwrap();
        codegen.set("stack_switch_model", "basic").unwrap();
        codegen.set("unwind_info", "true").unwrap();
        codegen.set("is_pic", "false").unwrap();

        // affect runtime performance
        codegen.set("opt_level", "speed").unwrap();
        codegen.set("enable_verifier", verifier).unwrap();
        codegen.set("enable_alias_analysis", "true").unwrap();
        codegen.set("regalloc_algorithm", "backtracking").unwrap();
        codegen.set("regalloc_checker", "false").unwrap();
        codegen.set("enable_pinned_reg", "false").unwrap();
        codegen
            .set("enable_heap_access_spectre_mitigation", "false")
            .unwrap();
        codegen
            .set("enable_table_access_spectre_mitigation", "false")
            .unwrap();

        let flags = codegen::settings::Flags::new(codegen);
        let isa = isa.finish(flags).unwrap();

        Codegen {
            settings,
            hooks,
            isa,
            module: Module::new(),
        }
    }

    fn block_signature(&self) -> ir::Signature {
        let ptr = self.isa.pointer_type();
        ir::Signature {
            // info, ctx, regs, fastmem
            params: vec![ir::AbiParam::new(ptr); 4],
            returns: vec![],
            call_conv: codegen::isa::CallConv::Tail,
        }
    }

    fn trampoline_signature(&self) -> ir::Signature {
        let ptr = self.isa.pointer_type();
        ir::Signature {
            params: vec![ir::AbiParam::new(ptr); 3],
            returns: vec![],
            call_conv: codegen::isa::CallConv::SystemV,
        }
    }

    /// Compiles and returns a trampoline to call blocks.
    fn trampoline(
        &mut self,
        code_ctx: &mut codegen::Context,
        func_ctx: &mut frontend::FunctionBuilderContext,
    ) -> Trampoline {
        let block_sig = self.block_signature();

        let mut func = ir::Function::new();
        func.signature = self.trampoline_signature();

        let mut builder = frontend::FunctionBuilder::new(&mut func, func_ctx);
        let entry_bb = builder.create_block();
        builder.append_block_params_for_function_params(entry_bb);
        builder.switch_to_block(entry_bb);
        builder.seal_block(entry_bb);

        let params = builder.block_params(entry_bb);
        let info_ptr = params[0];
        let ctx_ptr = params[1];
        let block_ptr = params[2];
        let ptr_type = self.isa.pointer_type();

        // extract regs ptr
        let get_regs_sig = builder.import_signature(Hooks::get_registers_sig(ptr_type));
        let get_registers = builder
            .ins()
            .iconst(ptr_type, self.hooks.get_registers as usize as i64);
        let inst = builder
            .ins()
            .call_indirect(get_regs_sig, get_registers, &[ctx_ptr]);
        let regs_ptr = builder.inst_results(inst)[0];

        // extract fastmem ptr
        let get_fmem_sig = builder.import_signature(Hooks::get_fastmem_sig(ptr_type));
        let get_fmem = builder
            .ins()
            .iconst(ptr_type, self.hooks.get_fastmem as usize as i64);
        let inst = builder
            .ins()
            .call_indirect(get_fmem_sig, get_fmem, &[ctx_ptr]);
        let fmem_ptr = builder.inst_results(inst)[0];

        // call the block
        let block_sig = builder.import_signature(block_sig);
        builder.ins().call_indirect(
            block_sig,
            block_ptr,
            &[info_ptr, ctx_ptr, regs_ptr, fmem_ptr],
        );

        builder.ins().return_(&[]);
        builder.finalize();

        code_ctx.clear();
        code_ctx.func = func;
        code_ctx
            .compile(&*self.isa, &mut Default::default())
            .unwrap();

        let compiled = code_ctx.take_compiled_code().unwrap();
        let alloc = self.module.allocate_code(compiled.code_buffer());

        Trampoline(alloc)
    }

    fn write_relocation(code: &mut [u8], reloc: &FinalizedMachReloc, addr: usize) {
        match reloc.kind {
            Reloc::Abs8 => {
                let base = reloc.offset;
                code[base as usize..][..size_of::<usize>()].copy_from_slice(&addr.to_ne_bytes());
            }
            _ => todo!("relocation kind {:?}", reloc.kind),
        }
    }

    fn apply_relocations(
        &mut self,
        code: &mut [u8],
        mapping: &PrimaryMap<UserExternalNameRef, UserExternalName>,
        relocs: &[FinalizedMachReloc],
    ) {
        for reloc in relocs {
            let FinalizedRelocTarget::ExternalName(ext_name) = &reloc.target else {
                unreachable!()
            };

            let ir::ExternalName::User(name_ref) = ext_name else {
                unreachable!()
            };

            let name = mapping.get(*name_ref).unwrap();
            match name.namespace {
                NAMESPACE_USER_HOOKS => {
                    let hook_kind = HookKind::from_repr(name.index).unwrap();
                    let addr = match hook_kind {
                        HookKind::GetRegisters => self.hooks.get_registers as usize,
                        HookKind::GetFastmem => self.hooks.get_fastmem as usize,
                        HookKind::FollowLink => self.hooks.follow_link as usize,
                        HookKind::TryLink => self.hooks.try_link as usize,
                        HookKind::ReadI8 => self.hooks.read_i8 as usize,
                        HookKind::ReadI16 => self.hooks.read_i16 as usize,
                        HookKind::ReadI32 => self.hooks.read_i32 as usize,
                        HookKind::ReadI64 => self.hooks.read_i64 as usize,
                        HookKind::WriteI8 => self.hooks.write_i8 as usize,
                        HookKind::WriteI16 => self.hooks.write_i16 as usize,
                        HookKind::WriteI32 => self.hooks.write_i32 as usize,
                        HookKind::WriteI64 => self.hooks.write_i64 as usize,
                        HookKind::ReadQuant => self.hooks.read_quantized as usize,
                        HookKind::WriteQuant => self.hooks.write_quantized as usize,
                        HookKind::InvICache => self.hooks.invalidate_icache as usize,
                        HookKind::ClearICache => self.hooks.clear_icache as usize,
                        HookKind::DCacheDma => self.hooks.dcache_dma as usize,
                        HookKind::MsrChanged => self.hooks.msr_changed as usize,
                        HookKind::IBatChanged => self.hooks.ibat_changed as usize,
                        HookKind::DBatChanged => self.hooks.dbat_changed as usize,
                        HookKind::TbRead => self.hooks.tb_read as usize,
                        HookKind::TbChanged => self.hooks.tb_changed as usize,
                        HookKind::DecRead => self.hooks.dec_read as usize,
                        HookKind::DecChanged => self.hooks.dec_changed as usize,
                    };

                    Self::write_relocation(code, reloc, addr);
                }
                NAMESPACE_INTERNALS => {
                    assert_eq!(name.index, INTERNAL_RAISE_EXCEPTION);
                    extern "sysv64-unwind" fn raise_exception(
                        regs: &mut Cpu,
                        exception: Exception,
                    ) {
                        regs.raise_exception(exception);
                    }

                    let addr = raise_exception as extern "sysv64-unwind" fn(_, _) as usize;
                    Self::write_relocation(code, reloc, addr);
                }
                NAMESPACE_LINK_DATA => {
                    let link_data = self.module.allocate_data(Layout::new::<Option<LinkData>>());

                    // initialize as None
                    unsafe {
                        link_data.as_ptr().cast::<Option<LinkData>>().write(None);
                    }

                    let addr = unsafe { link_data.as_ptr().addr().get() };
                    Self::write_relocation(code, reloc, addr);
                }
                _ => unreachable!(),
            }
        }
    }
}

/// A JIT context, producing [`Block`]s.
pub struct Jit {
    codegen: Codegen,
    code_ctx: codegen::Context,
    func_ctx: frontend::FunctionBuilderContext,
    cache: Option<Cache>,
    compiled_count: u64,
    trampoline: Trampoline,
}

struct Translated {
    func: ir::Function,
    sequence: Sequence,
    cycles: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct Artifact {
    user_named_funcs: PrimaryMap<UserExternalNameRef, UserExternalName>,
    relocs: Vec<FinalizedMachReloc>,
    unwind: Option<UnwindInfo>,
    disasm: Option<String>,
    #[serde(with = "serde_bytes")]
    code: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("block contains no instructions")]
    EmptyBlock,
    #[error(transparent)]
    Builder { source: builder::BuilderError },
    #[error(transparent)]
    Codegen {
        source: codegen::CodegenError,
        sequence: Sequence,
        clir: Option<String>,
    },
}

impl Jit {
    pub(crate) fn with_isa(isa: codegen::isa::Builder, settings: Settings, hooks: Hooks) -> Self {
        let mut codegen = Codegen::new(isa, settings.codegen, hooks);
        let mut code_ctx = codegen::Context::new();
        let mut func_ctx = frontend::FunctionBuilderContext::new();
        let cache = settings.cache_path.map(Cache::new);
        let trampoline = codegen.trampoline(&mut code_ctx, &mut func_ctx);

        Self {
            codegen,
            code_ctx,
            func_ctx,
            cache,
            compiled_count: 0,
            trampoline,
        }
    }

    pub fn new(settings: Settings, hooks: Hooks) -> Self {
        Self::with_isa(native::builder().unwrap(), settings, hooks)
    }

    /// Translates a sequence of instructions into a cranelift function.
    fn translate(
        &mut self,
        instructions: impl Iterator<Item = Ins>,
    ) -> Result<Translated, BuildError> {
        let mut func = ir::Function::new();
        func.signature = self.codegen.block_signature();

        let func_builder = frontend::FunctionBuilder::new(&mut func, &mut self.func_ctx);
        let builder = BlockBuilder::new(&mut self.codegen, func_builder);

        let (sequence, cycles) = builder.build(instructions).context(BuildCtx::Builder)?;
        if sequence.is_empty() {
            return Err(BuildError::EmptyBlock);
        }

        Ok(Translated {
            func,
            sequence,
            cycles,
        })
    }

    /// Compiles a cranelift function in the code context into an artifact.
    fn compile(&mut self, disasm: bool) -> Result<Artifact, codegen::CodegenError> {
        self.code_ctx.want_disasm = disasm;
        self.code_ctx
            .compile(&*self.codegen.isa, &mut Default::default())
            .map_err(|e| e.inner)?;

        let compiled = self.code_ctx.take_compiled_code().unwrap();
        let code = compiled.code_buffer().to_owned();
        let unwind = compiled
            .create_unwind_info(&*self.codegen.isa)
            .ok()
            .flatten();
        let disasm = compiled.vcode;

        Ok(Artifact {
            code,
            user_named_funcs: self.code_ctx.func.params.user_named_funcs().clone(),
            relocs: compiled.buffer.relocs().to_owned(),
            unwind,
            disasm,
        })
    }

    pub(crate) fn build_artifact(
        &mut self,
        instructions: impl Iterator<Item = Ins>,
    ) -> Result<(Artifact, Meta), BuildError> {
        let translated = self.translate(instructions)?;
        let func = translated.func;
        let sequence = translated.sequence;
        let pattern = sequence.detect_pattern();

        let clir = cfg!(debug_assertions).then(|| func.display().to_string());
        let key = ArtifactKey::new(&*self.codegen.isa, &self.codegen.settings, &sequence);

        let artifact = if let Some(cache) = &mut self.cache
            && let Some(artifact) = cache.get(key)
        {
            artifact
        } else {
            self.code_ctx.clear();
            self.code_ctx.func = func;

            let artifact =
                self.compile(cfg!(debug_assertions))
                    .with_context(|_| BuildCtx::Codegen {
                        sequence: sequence.clone(),
                        clir: clir.clone(),
                    })?;

            if let Some(cache) = &mut self.cache {
                cache.insert(key, &artifact);
            }

            artifact
        };

        let meta = Meta {
            seq: sequence,
            clir,
            disasm: artifact.disasm.clone(),
            cycles: translated.cycles,
            pattern,
        };

        Ok((artifact, meta))
    }

    /// Builds a block with the given instructions (up until a terminal instruction or the end of
    /// the iterator).
    pub fn build(&mut self, instructions: impl Iterator<Item = Ins>) -> Result<Block, BuildError> {
        let (artifact, meta) = self.build_artifact(instructions)?;

        let mut code = artifact.code;
        self.codegen
            .apply_relocations(&mut code, &artifact.user_named_funcs, &artifact.relocs);

        let alloc = self.codegen.module.allocate_code(&code);
        let unwind_handle = if let Some(unwind) = artifact.unwind {
            unsafe { UnwindHandle::new(&*self.codegen.isa, alloc.as_ptr().addr().get(), &unwind) }
        } else {
            None
        };

        // TODO: remove this and deal with handles
        std::mem::forget(unwind_handle);

        let block = Block::new(alloc, meta);
        self.compiled_count += 1;

        Ok(block)
    }

    /// Calls the given block with the given context.
    ///
    /// # Safety
    /// `ctx` must match the type expected by the hooks of this JIT context.
    pub unsafe fn call(&mut self, ctx: *mut Context, block: BlockFn) -> Info {
        // SAFETY: the exclusive reference to the context guarantees the allocator is not being
        // used, keeping the allocations safe
        unsafe { self.trampoline.call(ctx, block) }
    }
}
