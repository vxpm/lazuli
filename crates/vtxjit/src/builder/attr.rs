use cranelift::codegen::ir;
use cranelift::prelude::InstBuilder;
use lazuli::system::gx::Vertex;
use lazuli::system::gx::cmd::attributes::{
    self, Attribute, AttributeDescriptor, ColorFormat, ColorKind, CoordsFormat, PositionKind,
    TexCoordsKind,
};
use lazuli::system::gx::cmd::{ArrayDescriptor, Arrays};
use util::offset_of;
use zerocopy::IntoBytes;

use crate::builder::{MEMFLAGS, MEMFLAGS_READONLY, ParserBuilder};

/// Parses a single vector coordinate encoded as U8/I8/U16/I16.
fn coord_int(
    parser: &mut ParserBuilder,
    ptr: ir::Value,
    ty: ir::Type,
    signed: bool,
    scale: ir::Value,
) -> ir::Value {
    // 01. load the integer value
    let value = parser.bd.ins().load(ty, MEMFLAGS_READONLY, ptr, 0);

    // 02. byteswap and extend
    let value = parser.bd.ins().bswap(value);
    let value = if signed {
        parser.bd.ins().sextend(ir::types::I32, value)
    } else {
        parser.bd.ins().uextend(ir::types::I32, value)
    };

    // 03. convert to F32
    let value = if signed {
        parser.bd.ins().fcvt_from_sint(ir::types::F32, value)
    } else {
        parser.bd.ins().fcvt_from_uint(ir::types::F32, value)
    };

    // 05. multiply by scale

    parser.bd.ins().fmul(value, scale)
}

/// Parses a single vector coordinate encoded as F32.
fn coord_float(parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    // 01. load the value as an integer
    let value = parser
        .bd
        .ins()
        .load(ir::types::I32, MEMFLAGS_READONLY, ptr, 0);

    // 02. byteswap and bitcast
    let value = parser.bd.ins().bswap(value);

    parser
        .bd
        .ins()
        .bitcast(ir::types::F32, ir::MemFlags::new(), value)
}

/// Parses a vec2/vec3 with components encoded as U8/I8/U16/I16.
fn vec_int(
    parser: &mut ParserBuilder,
    ptr: ir::Value,
    ty: ir::Type,
    signed: bool,
    triplet: bool,
    scale: ir::Value,
) -> [ir::Value; 3] {
    // 01. load the packed value bytes
    let bytes = parser
        .bd
        .ins()
        .load(ir::types::I8X16, MEMFLAGS_READONLY, ptr, 0);

    // 02. convert to little endian and prepare for sign extension if needed
    let vector = if ty == ir::types::I16 {
        const ZEROED: u8 = 0xFF;
        const SHUFFLE_CONST_SIGNED_VEC2: [u8; 16] = [
            ZEROED, ZEROED, 1, 0, // lane 0 (first value)
            ZEROED, ZEROED, 3, 2, // lane 1 (second value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 2 (zeroed)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_UNSIGNED_VEC2: [u8; 16] = [
            1, 0, ZEROED, ZEROED, // lane 0 (first value)
            3, 2, ZEROED, ZEROED, // lane 1 (second value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 2 (zeroed)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_SIGNED_VEC3: [u8; 16] = [
            ZEROED, ZEROED, 1, 0, // lane 0 (first value)
            ZEROED, ZEROED, 3, 2, // lane 1 (second value)
            ZEROED, ZEROED, 5, 4, // lane 2 (third value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_UNSIGNED_VEC3: [u8; 16] = [
            1, 0, ZEROED, ZEROED, // lane 0 (first value)
            3, 2, ZEROED, ZEROED, // lane 1 (second value)
            5, 4, ZEROED, ZEROED, // lane 2 (third value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];

        let shuffle_const = match (signed, triplet) {
            (true, true) => SHUFFLE_CONST_SIGNED_VEC3,
            (true, false) => SHUFFLE_CONST_SIGNED_VEC2,
            (false, true) => SHUFFLE_CONST_UNSIGNED_VEC3,
            (false, false) => SHUFFLE_CONST_UNSIGNED_VEC2,
        };

        let shuffle_const = parser
            .bd
            .func
            .dfg
            .constants
            .insert(ir::ConstantData::from(shuffle_const.as_bytes()));

        let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
        let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);

        parser.bd.ins().bitcast(
            ir::types::I32X4,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            shuffled,
        )
    } else {
        const ZEROED: u8 = 0xFF;
        const SHUFFLE_CONST_SIGNED_VEC2: [u8; 16] = [
            ZEROED, ZEROED, ZEROED, 0, // lane 0 (first value)
            ZEROED, ZEROED, ZEROED, 1, // lane 1 (second value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 2 (zeroed)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_UNSIGNED_VEC2: [u8; 16] = [
            0, ZEROED, ZEROED, ZEROED, // lane 0 (first value)
            1, ZEROED, ZEROED, ZEROED, // lane 1 (second value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 2 (zeroed)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_SIGNED_VEC3: [u8; 16] = [
            ZEROED, ZEROED, ZEROED, 0, // lane 0 (first value)
            ZEROED, ZEROED, ZEROED, 1, // lane 1 (second value)
            ZEROED, ZEROED, ZEROED, 2, // lane 2 (third value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];
        const SHUFFLE_CONST_UNSIGNED_VEC3: [u8; 16] = [
            0, ZEROED, ZEROED, ZEROED, // lane 0 (first value)
            1, ZEROED, ZEROED, ZEROED, // lane 1 (second value)
            2, ZEROED, ZEROED, ZEROED, // lane 2 (third value)
            ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
        ];

        let shuffle_const = match (signed, triplet) {
            (true, true) => SHUFFLE_CONST_SIGNED_VEC3,
            (true, false) => SHUFFLE_CONST_SIGNED_VEC2,
            (false, true) => SHUFFLE_CONST_UNSIGNED_VEC3,
            (false, false) => SHUFFLE_CONST_UNSIGNED_VEC2,
        };

        let shuffle_const = parser
            .bd
            .func
            .dfg
            .constants
            .insert(ir::ConstantData::from(shuffle_const.as_bytes()));

        let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
        let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);

        parser.bd.ins().bitcast(
            ir::types::I32X4,
            ir::MemFlags::new().with_endianness(ir::Endianness::Little),
            shuffled,
        )
    };

    // 03. sign extend if needed and convert to F32X4
    let vector = if signed {
        let vector = parser.bd.ins().sshr_imm(vector, 32 - ty.bits() as i64);
        parser.bd.ins().fcvt_from_sint(ir::types::F32X4, vector)
    } else {
        parser.bd.ins().fcvt_from_uint(ir::types::F32X4, vector)
    };

    // 04. multiply by scale
    let scale = parser.bd.ins().splat(ir::types::F32X4, scale);
    let vector = parser.bd.ins().fmul(vector, scale);

    // 05. split it
    let first = parser.bd.ins().extractlane(vector, 0);
    let second = parser.bd.ins().extractlane(vector, 1);
    let third = parser.bd.ins().extractlane(vector, 2);

    [first, second, third]
}

/// Parses a vec2/vec3 with components encoded as F32.
fn vec_float(parser: &mut ParserBuilder, ptr: ir::Value, triplet: bool) -> [ir::Value; 3] {
    // 01. load the float values as I32s
    let vector = parser
        .bd
        .ins()
        .load(ir::types::I32X4, MEMFLAGS_READONLY, ptr, 0);

    // 02. convert to little endian
    const ZEROED: u8 = 0xFF;
    const SHUFFLE_CONST_VEC2: [u8; 16] = [
        3, 2, 1, 0, // lane 0 (first value)
        7, 6, 5, 4, // lane 1 (second value)
        ZEROED, ZEROED, ZEROED, ZEROED, // lane 2 (zeroed)
        ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
    ];
    const SHUFFLE_CONST_VEC3: [u8; 16] = [
        3, 2, 1, 0, // lane 0 (first value)
        7, 6, 5, 4, // lane 1 (second value)
        11, 10, 9, 8, // lane 2 (third value)
        ZEROED, ZEROED, ZEROED, ZEROED, // lane 3 (dont care)
    ];

    let bytes = parser.bd.ins().bitcast(
        ir::types::I8X16,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        vector,
    );

    let shuffle_const = if triplet {
        SHUFFLE_CONST_VEC3
    } else {
        SHUFFLE_CONST_VEC2
    };

    let shuffle_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(shuffle_const.as_bytes()));

    let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
    let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);

    // 03. convert to F32X4
    let vector = parser.bd.ins().bitcast(
        ir::types::F32X4,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        shuffled,
    );

    // 04. split it
    let first = parser.bd.ins().extractlane(vector, 0);
    let second = parser.bd.ins().extractlane(vector, 1);
    let third = parser.bd.ins().extractlane(vector, 2);

    [first, second, third]
}

fn rgba4444(parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    // 01. load the packed bytes
    let bytes = parser
        .bd
        .ins()
        .load(ir::types::I8X16, MEMFLAGS_READONLY, ptr, 0);

    // 02. unpack into lanes
    const ZEROED: u8 = 0xFF;
    const SHUFFLE_CONST: [u8; 16] = [
        0, ZEROED, ZEROED, ZEROED, // lane 0 (rg)
        0, ZEROED, ZEROED, ZEROED, // lane 1 (rg)
        1, ZEROED, ZEROED, ZEROED, // lane 2 (ba)
        1, ZEROED, ZEROED, ZEROED, // lane 3 (ba)
    ];

    let shuffle_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(SHUFFLE_CONST.as_bytes()));

    let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
    let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);
    let vector = parser.bd.ins().bitcast(
        ir::types::I32X4,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        shuffled,
    );

    // 03. unpack nibbles
    const LOW_LANE: u32 = 0;
    const HIGH_LANE: u32 = u32::MAX;
    const BLEND_CONST: [u32; 4] = [LOW_LANE, HIGH_LANE, LOW_LANE, HIGH_LANE];

    let blend_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(BLEND_CONST.as_bytes()));

    let blend_mask = parser.bd.ins().vconst(ir::types::I32X4, blend_const);
    let band_value = parser.bd.ins().iconst(ir::types::I32, 15);
    let band_value = parser
        .bd
        .ins()
        .scalar_to_vector(ir::types::I32X4, band_value);
    let low_nibbles = parser.bd.ins().band(vector, band_value);
    let high_nibbles = parser.bd.ins().ushr_imm(vector, 4);
    let rgba = parser
        .bd
        .ins()
        .x86_blendv(blend_mask, high_nibbles, low_nibbles);

    // 04. convert to F32X4
    let vector = parser.bd.ins().fcvt_from_uint(ir::types::F32X4, rgba);
    let recip = parser.bd.ins().f32const(1.0 / 15.0);
    let recip = parser.bd.ins().splat(ir::types::F32X4, recip);
    parser.bd.ins().fmul(vector, recip)
}

fn rgb6666(parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    // 01. load the packed bytes
    let bytes = parser
        .bd
        .ins()
        .load(ir::types::I8X16, MEMFLAGS_READONLY, ptr, 0);

    // 02. unpack into lanes
    const ZEROED: u8 = 0xFF;
    const SHUFFLE_CONST: [u8; 16] = [
        0, 1, 2, ZEROED, // lane 0 (r)
        0, 1, 2, ZEROED, // lane 1 (g)
        0, 1, 2, ZEROED, // lane 2 (b)
        0, 1, 2, ZEROED, // lane 3 (a)
    ];

    let shuffle_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(SHUFFLE_CONST.as_bytes()));

    let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
    let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);
    let vector = parser.bd.ins().bitcast(
        ir::types::I32X4,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        shuffled,
    );

    // 03. unpack nibbles
    // since you can't shift each lane by differen amounts, we first multiply by powers of 2
    // (shift left) by different amounts, then shift right by the same amount
    //
    // we could avoid the mul by instead using division by 2 (shift right), but i bet thats way
    // slower than a mul
    #[allow(clippy::eq_op)]
    const MUL_CONST: [u32; 4] = [1 << 18, 1 << (18 - 6), 1 << (18 - 12), 1 << (18 - 18)];
    let mul_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(MUL_CONST.as_bytes()));

    let mul_const = parser.bd.ins().vconst(ir::types::I32X4, mul_const);
    let vector = parser.bd.ins().imul(vector, mul_const);
    let vector = parser.bd.ins().ushr_imm(vector, 18);

    let mask = parser.bd.ins().iconst(ir::types::I32, 0x3F);
    let mask = parser.bd.ins().splat(ir::types::I32X4, mask);
    let vector = parser.bd.ins().band(vector, mask);

    // 04. convert to F32X4
    let vector = parser.bd.ins().fcvt_from_uint(ir::types::F32X4, vector);
    let recip = parser.bd.ins().f32const(1.0 / 63.0);
    let recip = parser.bd.ins().splat(ir::types::F32X4, recip);
    parser.bd.ins().fmul(vector, recip)
}

fn rgba8888(parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    // 01. load the packed bytes
    let bytes = parser
        .bd
        .ins()
        .load(ir::types::I8X16, MEMFLAGS_READONLY, ptr, 0);

    // 02. unpack into lanes
    const ZEROED: u8 = 0xFF;
    const SHUFFLE_CONST: [u8; 16] = [
        0, ZEROED, ZEROED, ZEROED, // lane 0 (r)
        1, ZEROED, ZEROED, ZEROED, // lane 1 (g)
        2, ZEROED, ZEROED, ZEROED, // lane 2 (b)
        3, ZEROED, ZEROED, ZEROED, // lane 3 (a)
    ];

    let shuffle_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(SHUFFLE_CONST.as_bytes()));

    let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
    let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);

    let vector = parser.bd.ins().bitcast(
        ir::types::I32X4,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        shuffled,
    );

    // 03. convert to F32X4
    let vector = parser.bd.ins().fcvt_from_uint(ir::types::F32X4, vector);
    let recip = parser.bd.ins().f32const(1.0 / 255.0);
    let recip = parser.bd.ins().splat(ir::types::F32X4, recip);
    parser.bd.ins().fmul(vector, recip)
}

fn rgb565(parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    // 01. load the packed bytes
    let bytes = parser
        .bd
        .ins()
        .load(ir::types::I8X16, MEMFLAGS_READONLY, ptr, 0);

    // 02. unpack into lanes
    const ZEROED: u8 = 0xFF;
    const SHUFFLE_CONST: [u8; 16] = [
        0, 1, ZEROED, ZEROED, // lane 0 (r)
        0, 1, ZEROED, ZEROED, // lane 1 (g)
        0, 1, ZEROED, ZEROED, // lane 2 (b)
        0, 1, ZEROED, ZEROED, // lane 3 (a)
    ];

    let shuffle_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(SHUFFLE_CONST.as_bytes()));

    let shuffle_mask = parser.bd.ins().vconst(ir::types::I8X16, shuffle_const);
    let shuffled = parser.bd.ins().swizzle(bytes, shuffle_mask);

    let vector = parser.bd.ins().bitcast(
        ir::types::I32X4,
        ir::MemFlags::new().with_endianness(ir::Endianness::Little),
        shuffled,
    );

    // 03. unpack nibbles
    // since you can't shift each lane by differen amounts, we first multiply by powers of 2
    // (shift left) by different amounts, then shift right by the same amount
    //
    // we could avoid the mul by instead using division by 2 (shift right), but i bet thats way
    // slower than a mul
    #[allow(clippy::eq_op)]
    const MUL_CONST: [u32; 4] = [1 << 11, 1 << (11 - 5), 1 << (11 - 11), 0];
    const AND_CONST: [u32; 4] = [0x1F, 0x3F, 0x1F, 0];
    const RECIP_CONST: [f32; 4] = [1.0 / 31.0, 1.0 / 63.0, 1.0 / 31.0, 0.0];

    let mul_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(MUL_CONST.as_bytes()));
    let and_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(AND_CONST.as_bytes()));
    let recip_const = parser
        .bd
        .func
        .dfg
        .constants
        .insert(ir::ConstantData::from(RECIP_CONST.as_bytes()));

    let mul_const = parser.bd.ins().vconst(ir::types::I32X4, mul_const);
    let and_const = parser.bd.ins().vconst(ir::types::I32X4, and_const);

    let vector = parser.bd.ins().imul(vector, mul_const);
    let vector = parser.bd.ins().ushr_imm(vector, 11);
    let vector = parser.bd.ins().band(vector, and_const);

    // 04. convert to F32X4
    let vector = parser.bd.ins().fcvt_from_uint(ir::types::F32X4, vector);
    let recip = parser.bd.ins().vconst(ir::types::F32X4, recip_const);
    let vector = parser.bd.ins().fmul(vector, recip);
    let max = parser.bd.ins().f32const(1.0);
    parser.bd.ins().insertlane(vector, max, 3)
}

fn read_color(format: ColorFormat, parser: &mut ParserBuilder, ptr: ir::Value) -> ir::Value {
    match format {
        ColorFormat::Rgb565 => rgb565(parser, ptr),
        ColorFormat::Rgb888 | ColorFormat::Rgb888x => {
            let rgba = rgba8888(parser, ptr);
            let max = parser.bd.ins().f32const(1.0);
            parser.bd.ins().insertlane(rgba, max, 3)
        }
        ColorFormat::Rgba4444 => rgba4444(parser, ptr),
        ColorFormat::Rgba6666 => rgb6666(parser, ptr),
        ColorFormat::Rgba8888 => rgba8888(parser, ptr),
        _ => panic!("reserved color format"),
    }
}

pub trait AttributeExt: Attribute {
    const ARRAY_OFFSET: usize;

    fn set_default(_parser: &mut ParserBuilder) {}
    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32;
}

impl AttributeExt for attributes::PosMatrixIndex {
    const ARRAY_OFFSET: usize = usize::MAX;

    fn set_default(parser: &mut ParserBuilder) {
        parser.bd.ins().store(
            MEMFLAGS,
            parser.consts.default_pos,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, pos_norm_matrix) as i32,
        );
    }

    fn parse(_: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let index = parser
            .bd
            .ins()
            .load(ir::types::I8, MEMFLAGS_READONLY, ptr, 0);

        parser.include_matrix(false, index);
        parser.include_matrix(true, index);

        parser.bd.ins().store(
            MEMFLAGS,
            index,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, pos_norm_matrix) as i32,
        );

        1
    }
}

impl<const N: usize> AttributeExt for attributes::TexMatrixIndex<N> {
    const ARRAY_OFFSET: usize = usize::MAX;

    fn parse(_: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let index = parser
            .bd
            .ins()
            .load(ir::types::I8, MEMFLAGS_READONLY, ptr, 0);

        parser.include_matrix(false, index);

        parser.bd.ins().store(
            MEMFLAGS,
            index,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, tex_coords_matrix) as i32 + size_of::<u8>() as i32 * N as i32,
        );

        1
    }
}

impl AttributeExt for attributes::Position {
    const ARRAY_OFFSET: usize = offset_of!(Arrays, position);

    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let (ty, signed) = match desc.format() {
            CoordsFormat::U8 => (ir::types::I8, false),
            CoordsFormat::I8 => (ir::types::I8, true),
            CoordsFormat::U16 => (ir::types::I16, false),
            CoordsFormat::I16 => (ir::types::I16, true),
            CoordsFormat::F32 => (ir::types::F32, true),
            _ => panic!("reserved format"),
        };

        let scale = 1.0 / 2.0f32.powi(desc.shift().value() as i32);
        let scale = parser.bd.ins().f32const(scale);
        let triplet = desc.kind() == PositionKind::Vec3;

        let [x, y, z] = match ty {
            ir::types::I8 | ir::types::I16 => vec_int(parser, ptr, ty, signed, triplet, scale),
            _ => vec_float(parser, ptr, triplet),
        };

        parser.bd.ins().store(
            MEMFLAGS,
            x,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, position.x) as i32,
        );

        parser.bd.ins().store(
            MEMFLAGS,
            y,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, position.y) as i32,
        );

        parser.bd.ins().store(
            MEMFLAGS,
            z,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, position.z) as i32,
        );

        desc.size()
    }
}

impl AttributeExt for attributes::Normal {
    const ARRAY_OFFSET: usize = offset_of!(Arrays, normal);

    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let (ty, signed) = match desc.format() {
            CoordsFormat::U8 => (ir::types::I8, false),
            CoordsFormat::I8 => (ir::types::I8, true),
            CoordsFormat::U16 => (ir::types::I16, false),
            CoordsFormat::I16 => (ir::types::I16, true),
            CoordsFormat::F32 => (ir::types::F32, true),
            _ => panic!("reserved format"),
        };

        let exp = if ty.bytes() == 1 { 6 } else { 14 };
        let scale = 1.0 / 2.0f32.powi(exp);
        let scale = parser.bd.ins().f32const(scale);

        let [x, y, z] = match ty {
            ir::types::I8 | ir::types::I16 => vec_int(parser, ptr, ty, signed, true, scale),
            _ => vec_float(parser, ptr, true),
        };

        parser.bd.ins().store(
            MEMFLAGS,
            x,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, normal.x) as i32,
        );

        parser.bd.ins().store(
            MEMFLAGS,
            y,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, normal.y) as i32,
        );

        parser.bd.ins().store(
            MEMFLAGS,
            z,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, normal.z) as i32,
        );

        desc.size()
    }
}

impl AttributeExt for attributes::Chan0 {
    const ARRAY_OFFSET: usize = offset_of!(Arrays, chan0);

    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let rgba = read_color(desc.format(), parser, ptr);
        let rgba = if desc.kind() == ColorKind::Rgb && desc.format().has_alpha() {
            let max = parser.bd.ins().f32const(1.0);
            parser.bd.ins().insertlane(rgba, max, 3)
        } else {
            rgba
        };

        parser.bd.ins().store(
            MEMFLAGS,
            rgba,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, chan0) as i32,
        );

        desc.size()
    }
}

impl AttributeExt for attributes::Chan1 {
    const ARRAY_OFFSET: usize = offset_of!(Arrays, chan1);

    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let rgba = read_color(desc.format(), parser, ptr);
        let rgba = if desc.kind() == ColorKind::Rgb && desc.format().has_alpha() {
            let max = parser.bd.ins().f32const(1.0);
            parser.bd.ins().insertlane(rgba, max, 3)
        } else {
            rgba
        };

        parser.bd.ins().store(
            MEMFLAGS,
            rgba,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, chan1) as i32,
        );

        desc.size()
    }
}

impl<const N: usize> AttributeExt for attributes::TexCoords<N> {
    const ARRAY_OFFSET: usize = offset_of!(Arrays, tex_coords) + size_of::<ArrayDescriptor>() * N;

    fn parse(desc: &Self::Descriptor, parser: &mut ParserBuilder, ptr: ir::Value) -> u32 {
        let (ty, signed) = match desc.format() {
            CoordsFormat::U8 => (ir::types::I8, false),
            CoordsFormat::I8 => (ir::types::I8, true),
            CoordsFormat::U16 => (ir::types::I16, false),
            CoordsFormat::I16 => (ir::types::I16, true),
            CoordsFormat::F32 => (ir::types::F32, true),
            _ => panic!("reserved format"),
        };

        let scale = 1.0 / 2.0f32.powi(desc.shift().value() as i32);
        let scale = parser.bd.ins().f32const(scale);

        let [s, t] = match desc.kind() {
            TexCoordsKind::Vec1 => {
                let s = match ty {
                    ir::types::I8 | ir::types::I16 => coord_int(parser, ptr, ty, signed, scale),
                    _ => coord_float(parser, ptr),
                };
                let t = parser.bd.ins().f32const(0.0);

                [s, t]
            }
            TexCoordsKind::Vec2 => {
                let [s, t, _] = match ty {
                    ir::types::I8 | ir::types::I16 => {
                        vec_int(parser, ptr, ty, signed, false, scale)
                    }
                    _ => vec_float(parser, ptr, false),
                };

                [s, t]
            }
        };

        parser.bd.ins().store(
            MEMFLAGS,
            s,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, tex_coords) as i32 + N as i32 * size_of::<[f32; 2]>() as i32,
        );

        parser.bd.ins().store(
            MEMFLAGS,
            t,
            parser.vars.vertex_ptr,
            offset_of!(Vertex, tex_coords) as i32
                + N as i32 * size_of::<[f32; 2]>() as i32
                + size_of::<f32>() as i32,
        );

        desc.size()
    }
}
