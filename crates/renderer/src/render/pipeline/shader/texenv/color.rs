use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev;
use wesl_quote::quote_expression;

use super::{comparison_target, constant, input_channel, sample_tex};

fn input(stage: &TexEnvStage, src: tev::color::InputSrc) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match src {
        tev::color::InputSrc::R3Color => quote_expression! { regs[3].rgb },
        tev::color::InputSrc::R3Alpha => quote_expression! { regs[3].aaa },
        tev::color::InputSrc::R0Color => quote_expression! { regs[0].rgb },
        tev::color::InputSrc::R0Alpha => quote_expression! { regs[0].aaa },
        tev::color::InputSrc::R1Color => quote_expression! { regs[1].rgb },
        tev::color::InputSrc::R1Alpha => quote_expression! { regs[1].aaa },
        tev::color::InputSrc::R2Color => quote_expression! { regs[2].rgb },
        tev::color::InputSrc::R2Alpha => quote_expression! { regs[2].aaa },
        tev::color::InputSrc::TexColor => {
            let texel = sample_tex(stage);
            quote_expression! { #texel.rgb }
        }
        tev::color::InputSrc::TexAlpha => {
            let texel = sample_tex(stage);
            quote_expression! { #texel.aaa }
        }
        tev::color::InputSrc::ChanColor => {
            let channel = input_channel(stage);
            quote_expression! { #channel.rgb }
        }
        tev::color::InputSrc::ChanAlpha => {
            let channel = input_channel(stage);
            quote_expression! { #channel.aaa }
        }
        tev::color::InputSrc::One => quote_expression! { vec3f(1f) },
        tev::color::InputSrc::Half => quote_expression! { vec3f(0.5f) },
        tev::color::InputSrc::Constant => {
            let constant = constant(stage.color_const);
            quote_expression! { #constant.rgb }
        }
        tev::color::InputSrc::Zero => quote_expression! { vec3f(0f) },
    }
}

fn comparative_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = input(stage, stage.ops.color.input_a());
    let input_b = input(stage, stage.ops.color.input_b());
    let input_c = input(stage, stage.ops.color.input_c());
    let input_d = input(stage, stage.ops.color.input_d());

    let target = stage.ops.color.comparison_target();
    let op = stage.ops.color.comparison_op();
    let clamp = stage.ops.color.clamp();
    let output = stage.ops.color.output().index();

    let compare_target_a = comparison_target(
        target,
        quote_expression!(input_a),
        quote_expression!(input_a_components),
    );
    let compare_target_b = comparison_target(
        target,
        quote_expression!(input_b),
        quote_expression!(input_b_components),
    );
    let comparison = match op {
        tev::ComparisonOp::GreaterThan => {
            quote_expression! { #compare_target_a > #compare_target_b }
        }
        tev::ComparisonOp::Equal => quote_expression! { #compare_target_a == #compare_target_b },
    };

    let clamped = if clamp {
        quote_expression! { color_compare }
    } else {
        quote_expression! { clamp(color_compare, vec3f(0f), vec3f(1f)) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a;
            let input_a_components = render::vec3f_to_vec3u(#input_a);
            let input_b = #input_b;
            let input_b_components = render::vec3f_to_vec3u(#input_b);

            let input_c = #input_c;
            let input_d = #input_d;

            let color_compare = input_d + select(vec3f(0), input_c, #comparison);
            let color_result = #clamped;

            regs[#output] = vec4f(color_result, regs[#output].a);
            last_color_output = #output;
        }
    }
}

fn regular_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = input(stage, stage.ops.color.input_a());
    let input_b = input(stage, stage.ops.color.input_b());
    let input_c = input(stage, stage.ops.color.input_c());
    let input_d = input(stage, stage.ops.color.input_d());

    let sign = if stage.ops.color.negate() { -1.0 } else { 1.0 };
    let bias = stage.ops.color.bias().value();
    let scale = stage.ops.color.scale().value();
    let clamp = stage.ops.color.clamp();
    let output = stage.ops.color.output().index();

    let clamped = if clamp {
        quote_expression! { color_add_mul }
    } else {
        quote_expression! { clamp(color_add_mul, vec3f(0f), vec3f(1f)) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a;
            let input_b = #input_b;
            let input_c = #input_c;
            let input_d = #input_d;
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

pub fn stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    if stage.ops.color.is_comparative() {
        comparative_stage(stage)
    } else {
        regular_stage(stage)
    }
}
