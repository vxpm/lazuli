use cranelift::codegen::{self, isa};
use cranelift::frontend::FunctionBuilderContext;
use lazuli::system::gx::cmd::VertexDescriptor;
use lazuli::system::gx::cmd::attributes::{
    AttributeMode, ColorDescriptor, ColorFormat, ColorKind, CoordsFormat, PositionDescriptor,
    PositionKind, VertexAttributeTable, VertexAttributeTableA,
};

use crate::Codegen;
use crate::parser::Config;

fn test_config(name: &str, config: Config) {
    fn inner(name: &str, config: Config, isa: isa::Builder, isa_name: &str) {
        let mut codegen = Codegen::with_isa(isa);
        let mut code_ctx = codegen::Context::new();
        let mut func_ctx = FunctionBuilderContext::new();
        let parser = codegen.compile(&mut code_ctx, &mut func_ctx, config);

        let clir = parser.meta().clir.clone().unwrap();
        let disasm = parser.meta().disasm.clone().unwrap();
        insta::assert_snapshot!(format!("{isa_name}_{}_clir", name), clir);
        insta::assert_snapshot!(format!("{isa_name}_{}_disasm", name), disasm);
    }

    inner(name, config, jitclif::isa::x86_64_v1(), "x86_64_v1");
    inner(name, config, jitclif::isa::x86_64_v3(), "x86_64_v3");
    inner(name, config, jitclif::isa::aarch64(), "aarch64");
}

#[test]
fn basic() {
    let pos = PositionDescriptor::default()
        .with_kind(PositionKind::Vec3)
        .with_format(CoordsFormat::I16);

    let chan0 = ColorDescriptor::default()
        .with_kind(ColorKind::Rgba)
        .with_format(ColorFormat::Rgb565);

    let vcd = VertexDescriptor::default()
        .with_position(AttributeMode::Direct)
        .with_chan0(AttributeMode::Direct);

    let vat = VertexAttributeTable {
        a: VertexAttributeTableA::default()
            .with_position(pos)
            .with_chan0(chan0),
        ..Default::default()
    };

    let config = Config { vcd, vat };
    test_config("pos(vec3_i16)_chan0(rgba_rgb565)", config);
}
