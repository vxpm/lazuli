use crate::JitVertexModule;
use crate::parser::Config;

fn test_config(config: Config) {
    let mut jit = JitVertexModule::new();
    let compiled = jit
        .codegen
        .compile(&mut jit.code_ctx, &mut jit.func_ctx, config);
}
