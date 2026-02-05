use lazuli::system::gx::cmd::VertexDescriptor;
use lazuli::system::gx::cmd::attributes::{
    AttributeMode, ColorDescriptor, ColorFormat, ColorKind, CoordsFormat, PositionDescriptor,
    PositionKind, VertexAttributeTable, VertexAttributeTableA,
};

use crate::JitVertexModule;
use crate::parser::Config;

fn test_config(name: &str, config: Config) {
    let mut jit = JitVertexModule::new();
    let parser = jit
        .codegen
        .compile(&mut jit.code_ctx, &mut jit.func_ctx, config);

    let clir = parser.meta().clir.clone().unwrap();
    let disasm = parser.meta().disasm.clone().unwrap();
    insta::assert_snapshot!(format!("{}_clir", name), clir);
    insta::assert_snapshot!(format!("{}_disasm", name), disasm);
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
