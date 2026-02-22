use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev;
use wesl_quote::{quote_expression, quote_statement};

use crate::render::pipeline::{AlphaFuncSettings, TexEnvSettings};

fn sample_tex(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    let map = stage.refs.map().value() as u32;
    let coord = stage.refs.coord().value() as u32;

    let tex_ident = wesl::syntax::Ident::new(format!("base::texture{map}"));
    let sampler_ident = wesl::syntax::Ident::new(format!("base::sampler{map}"));
    let coord_ident = wesl::syntax::Ident::new(format!("in.tex_coord{coord}"));
    let pipeline_immediates_ident = wesl::syntax::Ident::new("base::pipeline_immediates".into());

    let index = map / 2;
    let scaling_packed = quote_expression! { #pipeline_immediates_ident.scaling[#index] };
    let scaling = if map.is_multiple_of(2) {
        quote_expression!(#scaling_packed.xy)
    } else {
        quote_expression!(#scaling_packed.zw)
    };

    let index = map / 4;
    let lodbias_packed = quote_expression! { #pipeline_immediates_ident.lodbias[#index] };
    let lodbias = match map % 4 {
        0 => quote_expression!(#lodbias_packed.x),
        1 => quote_expression!(#lodbias_packed.y),
        2 => quote_expression!(#lodbias_packed.z),
        3 => quote_expression!(#lodbias_packed.w),
        _ => unreachable!(),
    };

    quote_expression! {
        textureSampleBias(#tex_ident, #sampler_ident, #scaling * #coord_ident.xy / #coord_ident.z, #lodbias)
    }
}

fn get_color_channel(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match stage.refs.input() {
        tev::Input::Channel0 => quote_expression! { in.chan0 },
        tev::Input::Channel1 => quote_expression! { in.chan1 },
        tev::Input::AlphaBump => quote_expression! { vec4f(base::PLACEHOLDER_RGB, 0f) },
        tev::Input::AlphaBumpNormalized => {
            quote_expression! { vec4f(base::PLACEHOLDER_RGB, 0f) }
        }
        tev::Input::Zero => quote_expression! { vec4f(0f) },
        _ => panic!("reserved color channel"),
    }
}

fn get_color_const(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match stage.color_const {
        tev::Constant::One => quote_expression! { vec4f(1f) },
        tev::Constant::SevenEights => quote_expression! { vec4f(7f / 8f) },
        tev::Constant::SixEights => quote_expression! { vec4f(6f / 8f) },
        tev::Constant::FiveEights => quote_expression! { vec4f(5f / 8f) },
        tev::Constant::FourEights => quote_expression! { vec4f(4f / 8f) },
        tev::Constant::ThreeEights => quote_expression! { vec4f(3f / 8f) },
        tev::Constant::TwoEights => quote_expression! { vec4f(2f / 8f) },
        tev::Constant::OneEight => quote_expression! { vec4f(1f / 8f) },
        tev::Constant::Const0 => quote_expression! { consts[R0] },
        tev::Constant::Const1 => quote_expression! { consts[R1] },
        tev::Constant::Const2 => quote_expression! { consts[R2] },
        tev::Constant::Const3 => quote_expression! { consts[R3] },
        tev::Constant::Const0R => quote_expression! { consts[R0].rrrr },
        tev::Constant::Const1R => quote_expression! { consts[R1].rrrr },
        tev::Constant::Const2R => quote_expression! { consts[R2].rrrr },
        tev::Constant::Const3R => quote_expression! { consts[R3].rrrr },
        tev::Constant::Const0G => quote_expression! { consts[R0].gggg },
        tev::Constant::Const1G => quote_expression! { consts[R1].gggg },
        tev::Constant::Const2G => quote_expression! { consts[R2].gggg },
        tev::Constant::Const3G => quote_expression! { consts[R3].gggg },
        tev::Constant::Const0B => quote_expression! { consts[R0].bbbb },
        tev::Constant::Const1B => quote_expression! { consts[R1].bbbb },
        tev::Constant::Const2B => quote_expression! { consts[R2].bbbb },
        tev::Constant::Const3B => quote_expression! { consts[R3].bbbb },
        tev::Constant::Const0A => quote_expression! { consts[R0].aaaa },
        tev::Constant::Const1A => quote_expression! { consts[R1].aaaa },
        tev::Constant::Const2A => quote_expression! { consts[R2].aaaa },
        tev::Constant::Const3A => quote_expression! { consts[R3].aaaa },
        _ => panic!("reserved color constant"),
    }
}

fn get_color_input(stage: &TexEnvStage, input: tev::color::InputSrc) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match input {
        tev::color::InputSrc::R3Color => quote_expression! { regs[R3].rgba },
        tev::color::InputSrc::R3Alpha => quote_expression! { regs[R3].aaaa },
        tev::color::InputSrc::R0Color => quote_expression! { regs[R0].rgba },
        tev::color::InputSrc::R0Alpha => quote_expression! { regs[R0].aaaa },
        tev::color::InputSrc::R1Color => quote_expression! { regs[R1].rgba },
        tev::color::InputSrc::R1Alpha => quote_expression! { regs[R1].aaaa },
        tev::color::InputSrc::R2Color => quote_expression! { regs[R2].rgba },
        tev::color::InputSrc::R2Alpha => quote_expression! { regs[R2].aaaa },
        tev::color::InputSrc::TexColor => {
            let tex = sample_tex(stage);
            quote_expression! { #tex.rgba }
        }
        tev::color::InputSrc::TexAlpha => {
            let tex = sample_tex(stage);
            quote_expression! { #tex.aaaa }
        }
        tev::color::InputSrc::ChanColor => {
            let color = get_color_channel(stage);
            quote_expression! { #color.rgba }
        }
        tev::color::InputSrc::ChanAlpha => {
            let color = get_color_channel(stage);
            quote_expression! { #color.aaaa }
        }
        tev::color::InputSrc::One => quote_expression! { vec4f(1f) },
        tev::color::InputSrc::Half => quote_expression! { vec4f(0.5f) },
        tev::color::InputSrc::Constant => get_color_const(stage),
        tev::color::InputSrc::Zero => quote_expression! { vec4f(0f) },
    }
}

fn get_compare_target(
    input_float: wesl::syntax::Expression,
    input_uint: wesl::syntax::Expression,
    target: tev::CompareTarget,
    alpha: bool,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    match target {
        tev::CompareTarget::R8 => quote_expression! { (#input_uint).r },
        tev::CompareTarget::GR16 => {
            quote_expression! { pack4xU8(vec4u((#input_uint).r, (#input_uint).g, 0, 0)) }
        }
        tev::CompareTarget::BGR16 => {
            quote_expression! { pack4xU8(vec4u((#input_uint).r, (#input_uint).g, (#input_uint).b, 0)) }
        }
        tev::CompareTarget::Component => {
            if alpha {
                quote_expression! { (#input_float).a }
            } else {
                quote_expression! { (#input_float).rgb }
            }
        }
    }
}

fn comparative_color_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = get_color_input(stage, stage.ops.color.input_a());
    let input_b = get_color_input(stage, stage.ops.color.input_b());
    let input_c = get_color_input(stage, stage.ops.color.input_c());
    let input_d = get_color_input(stage, stage.ops.color.input_d());

    let target = stage.ops.color.compare_target();
    let op = stage.ops.color.compare_op();
    let clamp = stage.ops.color.clamp();
    let output = stage.ops.color.output() as u32;

    let compare_target_a = get_compare_target(
        quote_expression!(input_a),
        quote_expression!(input_a_uint),
        target,
        false,
    );
    let compare_target_b = get_compare_target(
        quote_expression!(input_b),
        quote_expression!(input_b_uint),
        target,
        false,
    );
    let comparison = match op {
        tev::CompareOp::GreaterThan => quote_expression! { #compare_target_a > #compare_target_b },
        tev::CompareOp::Equal => quote_expression! { #compare_target_a == #compare_target_b },
    };

    let clamped = if clamp {
        quote_expression! { color_compare }
    } else {
        quote_expression! { clamp(color_compare, vec3f(0f), vec3f(1f)) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a;
            let input_a_uint = base::vec4f_to_vec4u(#input_a);
            let input_b = #input_b;
            let input_b_uint = base::vec4f_to_vec4u(#input_b);

            let input_c = #input_c.rgb;
            let input_d = #input_d.rgb;

            let color_compare = select(input_d, input_c, #comparison);
            let color_result = #clamped;

            regs[#output] = vec4f(color_result, regs[#output].a);
            last_color_output = #output;
        }
    }
}

fn regular_color_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = get_color_input(stage, stage.ops.color.input_a());
    let input_b = get_color_input(stage, stage.ops.color.input_b());
    let input_c = get_color_input(stage, stage.ops.color.input_c());
    let input_d = get_color_input(stage, stage.ops.color.input_d());

    let sign = if stage.ops.color.negate() { -1.0 } else { 1.0 };
    let bias = stage.ops.color.bias().value();
    let scale = stage.ops.color.scale().value();
    let clamp = stage.ops.color.clamp();
    let output = stage.ops.color.output() as u32;

    let clamped = if clamp {
        quote_expression! { color_add_mul }
    } else {
        quote_expression! { clamp(color_add_mul, vec3f(0f), vec3f(1f)) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a.rgb;
            let input_b = #input_b.rgb;
            let input_c = #input_c.rgb;
            let input_d = #input_d.rgb;
            let sign = #sign;
            let bias = #bias;
            let scale = #scale;

            let color_interpolation = sign * mix(input_a, input_b, input_c);
            let color_add_mul = scale * (color_interpolation + input_d + bias);
            let color_result = #clamped;

            regs[#output] = vec4f(color_result, regs[#output].a);
            last_color_output = #output;
        }
    }
}

pub fn color_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    if stage.ops.color.is_comparative() {
        comparative_color_stage(stage)
    } else {
        regular_color_stage(stage)
    }
}

fn get_alpha_const(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match stage.alpha_const {
        tev::Constant::One => quote_expression! { 1f },
        tev::Constant::SevenEights => quote_expression! { (7f / 8f) },
        tev::Constant::SixEights => quote_expression! { (6f / 8f) },
        tev::Constant::FiveEights => quote_expression! { (5f / 8f) },
        tev::Constant::FourEights => quote_expression! { (4f / 8f) },
        tev::Constant::ThreeEights => quote_expression! { (3f / 8f) },
        tev::Constant::TwoEights => quote_expression! { (2f / 8f) },
        tev::Constant::OneEight => quote_expression! { (1f / 8f) },
        tev::Constant::Const0 => quote_expression! { consts[R0].a },
        tev::Constant::Const1 => quote_expression! { consts[R1].a },
        tev::Constant::Const2 => quote_expression! { consts[R2].a },
        tev::Constant::Const3 => quote_expression! { consts[R3].a },
        tev::Constant::Const0R => quote_expression! { consts[R0].r },
        tev::Constant::Const1R => quote_expression! { consts[R1].r },
        tev::Constant::Const2R => quote_expression! { consts[R2].r },
        tev::Constant::Const3R => quote_expression! { consts[R3].r },
        tev::Constant::Const0G => quote_expression! { consts[R0].g },
        tev::Constant::Const1G => quote_expression! { consts[R1].g },
        tev::Constant::Const2G => quote_expression! { consts[R2].g },
        tev::Constant::Const3G => quote_expression! { consts[R3].g },
        tev::Constant::Const0B => quote_expression! { consts[R0].b },
        tev::Constant::Const1B => quote_expression! { consts[R1].b },
        tev::Constant::Const2B => quote_expression! { consts[R2].b },
        tev::Constant::Const3B => quote_expression! { consts[R3].b },
        tev::Constant::Const0A => quote_expression! { consts[R0].a },
        tev::Constant::Const1A => quote_expression! { consts[R1].a },
        tev::Constant::Const2A => quote_expression! { consts[R2].a },
        tev::Constant::Const3A => quote_expression! { consts[R3].a },
        _ => panic!("reserved alpha constant"),
    }
}

fn get_alpha_input(stage: &TexEnvStage, input: tev::alpha::InputSrc) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match input {
        tev::alpha::InputSrc::R3Alpha => quote_expression! { regs[R3].aaaa },
        tev::alpha::InputSrc::R0Alpha => quote_expression! { regs[R0].aaaa },
        tev::alpha::InputSrc::R1Alpha => quote_expression! { regs[R1].aaaa },
        tev::alpha::InputSrc::R2Alpha => quote_expression! { regs[R2].aaaa },
        tev::alpha::InputSrc::TexAlpha => {
            let tex = sample_tex(stage);
            quote_expression! { #tex.aaaa }
        }
        tev::alpha::InputSrc::ChanAlpha => {
            let color = get_color_channel(stage);
            quote_expression! { #color.aaaa }
        }
        tev::alpha::InputSrc::Constant => {
            let constant = get_alpha_const(stage);
            quote_expression! { vec4f(#constant) }
        }
        tev::alpha::InputSrc::Zero => quote_expression! { vec4f(0f) },
    }
}

fn comparative_alpha_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = get_alpha_input(stage, stage.ops.alpha.input_a());
    let input_b = get_alpha_input(stage, stage.ops.alpha.input_b());
    let input_c = get_alpha_input(stage, stage.ops.alpha.input_c());
    let input_d = get_alpha_input(stage, stage.ops.alpha.input_d());

    let target = stage.ops.alpha.compare_target();
    let op = stage.ops.alpha.compare_op();
    let clamp = stage.ops.alpha.clamp();
    let output = stage.ops.alpha.output() as u32;

    let compare_target_a = get_compare_target(
        quote_expression!(input_a),
        quote_expression!(input_a_uint),
        target,
        true,
    );
    let compare_target_b = get_compare_target(
        quote_expression!(input_b),
        quote_expression!(input_b_uint),
        target,
        true,
    );
    let comparison = match op {
        tev::CompareOp::GreaterThan => quote_expression! { #compare_target_a > #compare_target_b },
        tev::CompareOp::Equal => quote_expression! { #compare_target_a == #compare_target_b },
    };

    let clamped = if clamp {
        quote_expression! { alpha_compare }
    } else {
        quote_expression! { clamp(alpha_compare, 0f, 1f) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a;
            let input_a_uint = base::vec4f_to_vec4u(#input_a);
            let input_b = #input_b;
            let input_b_uint = base::vec4f_to_vec4u(#input_b);

            let input_c = #input_c.a;
            let input_d = #input_d.a;

            let alpha_compare = select(input_d, input_c, #comparison);
            let alpha_result = #clamped;

            regs[#output] = vec4f(regs[#output].rgb, alpha_result);
            last_alpha_output = #output;
        }
    }
}

fn regular_alpha_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = get_alpha_input(stage, stage.ops.alpha.input_a());
    let input_b = get_alpha_input(stage, stage.ops.alpha.input_b());
    let input_c = get_alpha_input(stage, stage.ops.alpha.input_c());
    let input_d = get_alpha_input(stage, stage.ops.alpha.input_d());

    let sign = if stage.ops.alpha.negate() { -1.0 } else { 1.0 };
    let bias = stage.ops.alpha.bias().value();
    let scale = stage.ops.alpha.scale().value();
    let clamp = stage.ops.alpha.clamp();
    let output = stage.ops.alpha.output() as u32;

    let clamped = if clamp {
        quote_expression! { alpha_add_mul }
    } else {
        quote_expression! { clamp(alpha_add_mul, 0f, 1f) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a.a;
            let input_b = #input_b.a;
            let input_c = #input_c.a;
            let input_d = #input_d.a;
            let sign = #sign;
            let bias = #bias;
            let scale = #scale;

            let alpha_interpolation = sign * mix(input_a, input_b, input_c);
            let alpha_add_mul = scale * (alpha_interpolation + input_d + bias);
            let alpha_result = #clamped;

            regs[#output] = vec4f(regs[#output].rgb, alpha_result);
            last_alpha_output = #output;
        }
    }
}

pub fn alpha_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    if stage.ops.alpha.is_comparative() {
        comparative_alpha_stage(stage)
    } else {
        regular_alpha_stage(stage)
    }
}

fn get_alpha_comparison_helper(
    compare: tev::alpha::Compare,
    idx: usize,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    let alpha_ref = wesl::syntax::Ident::new(format!("alpha_ref{idx}"));
    match compare {
        tev::alpha::Compare::Never => quote_expression! { false },
        tev::alpha::Compare::Less => quote_expression! { alpha < #alpha_ref },
        tev::alpha::Compare::Equal => quote_expression! { alpha == #alpha_ref },
        tev::alpha::Compare::LessOrEqual => quote_expression! { alpha <= #alpha_ref },
        tev::alpha::Compare::Greater => quote_expression! { alpha > #alpha_ref },
        tev::alpha::Compare::NotEqual => quote_expression! { alpha != #alpha_ref },
        tev::alpha::Compare::GreaterOrEqual => quote_expression! { alpha >= #alpha_ref },
        tev::alpha::Compare::Always => quote_expression! { true },
    }
}

pub fn get_alpha_comparison(settings: &AlphaFuncSettings) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    let a = get_alpha_comparison_helper(settings.comparison[0], 0);
    let b = get_alpha_comparison_helper(settings.comparison[1], 1);

    match settings.logic {
        tev::alpha::CompareLogic::And => quote_expression! { (#a) && (#b) },
        tev::alpha::CompareLogic::Or => quote_expression! { (#a) || (#b) },
        tev::alpha::CompareLogic::Xor => quote_expression! { (#a) != (#b) },
        tev::alpha::CompareLogic::Xnor => quote_expression! { (#a) == (#b) },
    }
}

pub fn get_depth_texture(settings: &TexEnvSettings) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    if matches!(
        settings.depth_tex.mode.op(),
        tev::depth::Op::Disabled | tev::depth::Op::Add
    ) {
        return Statement::Void;
    }

    let bias = settings.depth_tex.bias;
    let sampled = self::sample_tex(settings.stages.last().unwrap());
    let (depth_mid, depth_hi, depth_max) = match settings.depth_tex.mode.format() {
        tev::depth::Format::U8 => (
            quote_expression!(0),
            quote_expression!(0),
            quote_expression!(255),
        ),
        tev::depth::Format::U16 => (
            quote_expression!(depth_tex_sample.y),
            quote_expression!(0),
            quote_expression!(65535),
        ),
        tev::depth::Format::U24 => (
            quote_expression!(depth_tex_sample.y),
            quote_expression!(depth_tex_sample.z),
            quote_expression!(16777215),
        ),
        _ => panic!("reserved format"),
    };

    quote_statement! {
        {
            let depth_tex_sample = base::vec4f_to_vec4u(#sampled);
            let depth_tex_value = pack4xU8(vec4u(depth_tex_sample.x, #depth_mid, #depth_hi, 0)) + #bias;
            out.depth = clamp(f32(depth_tex_value) / f32(#depth_max), 0.0, 1.0);
        }
    }
}
