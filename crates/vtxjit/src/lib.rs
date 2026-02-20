mod builder;
mod parser;

#[cfg(test)]
mod test;

use std::collections::hash_map::Entry;
use std::mem::MaybeUninit;
use std::sync::Arc;

use cranelift::codegen::isa::{CallConv, TargetIsa};
use cranelift::codegen::settings::Configurable;
use cranelift::codegen::{self, ir};
use cranelift::prelude::isa;
use cranelift::{frontend, native};
use jitalloc::{Allocator, ReadExec};
use lazuli::modules::vertex::{Ctx, VertexModule};
use lazuli::system::gx::cmd::attributes::VertexAttributeTable;
use lazuli::system::gx::cmd::{VertexAttributeStream, VertexDescriptor};
use lazuli::system::gx::xform::DefaultMatrices;
use lazuli::system::gx::{MatrixId, MatrixSet, Vertex};
use parser::VertexParser;
use rustc_hash::FxHashMap;

use crate::builder::ParserBuilder;
use crate::parser::{Config, Meta};

#[repr(C)]
struct UnpackedDefaultMatrices {
    pub view: u8,
    pub tex: [u8; 8],
}

impl UnpackedDefaultMatrices {
    pub fn new(packed: DefaultMatrices) -> Self {
        Self {
            view: packed.view().value(),
            tex: packed.tex().map(|x| x.value()),
        }
    }
}

struct Codegen {
    isa: Arc<dyn TargetIsa>,
    allocator: Allocator<ReadExec>,
}

impl Codegen {
    fn with_isa(isa: isa::Builder) -> Self {
        let verifier = if cfg!(debug_assertions) {
            "true"
        } else {
            "false"
        };

        let mut codegen = codegen::settings::builder();
        codegen.set("preserve_frame_pointers", "true").unwrap();
        codegen.set("use_colocated_libcalls", "false").unwrap();
        codegen.set("stack_switch_model", "basic").unwrap();
        codegen.set("unwind_info", "false").unwrap();
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

        let isa = isa.finish(codegen::settings::Flags::new(codegen)).unwrap();

        Codegen {
            isa,
            allocator: Allocator::new(),
        }
    }

    fn new() -> Self {
        let isa = native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {}", msg);
        });

        Self::with_isa(isa)
    }

    fn parser_signature(&self, call_conv: CallConv) -> ir::Signature {
        let ptr = self.isa.pointer_type();
        ir::Signature {
            // ram, arrays, default matrices, data, vertices, matrix map, count
            params: vec![
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ptr),
                ir::AbiParam::new(ir::types::I32),
            ],
            returns: vec![],
            call_conv,
        }
    }

    fn apply_relocations(&mut self, code: &mut [u8], relocs: &[codegen::FinalizedMachReloc]) {
        for reloc in relocs {
            let codegen::FinalizedRelocTarget::ExternalName(ext_name) = &reloc.target else {
                unreachable!()
            };

            match ext_name {
                ir::ExternalName::LibCall(libcall) => {
                    let addr = jitclif::libcall(*libcall);
                    jitclif::write_relocation(code, reloc, addr);
                }
                _ => unimplemented!("external reloc name: {ext_name:?}"),
            }
        }
    }

    /// Compiles and returns a parser.
    fn compile(
        &mut self,
        code_ctx: &mut codegen::Context,
        func_ctx: &mut frontend::FunctionBuilderContext,
        config: Config,
    ) -> VertexParser {
        let mut func = ir::Function::new();
        func.signature = self.parser_signature(self.isa.default_call_conv());

        let func_builder = frontend::FunctionBuilder::new(&mut func, func_ctx);
        let builder = ParserBuilder::new(self, func_builder, config);
        builder.build();

        let clir = cfg!(test).then(|| func.display().to_string());
        code_ctx.clear();
        code_ctx.want_disasm = cfg!(test);
        code_ctx.func = func;
        code_ctx
            .compile(&*self.isa, &mut Default::default())
            .unwrap();

        let compiled = code_ctx.take_compiled_code().unwrap();
        let relocs = compiled.buffer.relocs().to_owned();
        let mut code = compiled.code_buffer().to_owned();

        self.apply_relocations(&mut code, &relocs);

        let alloc = self.allocator.allocate(64, &code);
        let disasm = compiled.vcode;
        let meta = Meta { clir, disasm };

        VertexParser::new(alloc, meta)
    }
}

pub struct JitVertexModule {
    codegen: Codegen,
    code_ctx: codegen::Context,
    func_ctx: frontend::FunctionBuilderContext,
    parsers: FxHashMap<Config, VertexParser>,
}

unsafe impl Send for JitVertexModule {}

impl JitVertexModule {
    pub fn new() -> Self {
        Self {
            codegen: Codegen::new(),
            code_ctx: codegen::Context::new(),
            func_ctx: frontend::FunctionBuilderContext::new(),
            parsers: FxHashMap::default(),
        }
    }
}

impl VertexModule for JitVertexModule {
    fn parse(
        &mut self,
        ctx: Ctx,
        vcd: &VertexDescriptor,
        vat: &VertexAttributeTable,
        stream: &VertexAttributeStream,
        vertices: &mut [MaybeUninit<Vertex>],
        matrix_set: &mut MatrixSet,
    ) {
        let config = Config {
            vcd: *vcd,
            vat: *vat,
        }
        .canonicalize();

        let parser = match self.parsers.entry(config) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                let parser = self
                    .codegen
                    .compile(&mut self.code_ctx, &mut self.func_ctx, config);

                v.insert(parser)
            }
        };

        let unpacked_default_matrices = UnpackedDefaultMatrices::new(*ctx.default_matrices);
        let view = MatrixId::from_position_idx(unpacked_default_matrices.view);
        matrix_set.include(view);
        matrix_set.include(view.normal());
        for tex in unpacked_default_matrices.tex {
            matrix_set.include(MatrixId::from_position_idx(tex));
        }

        let parser = parser.as_ptr();
        parser(
            ctx.ram.as_ptr(),
            ctx.arrays,
            &raw const unpacked_default_matrices,
            stream.data().as_ptr(),
            vertices.as_mut_ptr().cast(),
            matrix_set,
            stream.count() as u32,
        );
    }
}
