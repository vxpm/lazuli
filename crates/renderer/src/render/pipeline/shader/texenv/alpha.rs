use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev;
use wesl_quote::quote_expression;

use super::{comparison_target, constant, input_channel, sample_tex};

fn input(stage: &TexEnvStage, src: tev::alpha::InputSrc) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match src {
        tev::alpha::InputSrc::R3Alpha => quote_expression! { regs[3].a },
        tev::alpha::InputSrc::R0Alpha => quote_expression! { regs[0].a },
        tev::alpha::InputSrc::R1Alpha => quote_expression! { regs[1].a },
        tev::alpha::InputSrc::R2Alpha => quote_expression! { regs[2].a },
        tev::alpha::InputSrc::TexAlpha => {
            let texel = sample_tex(stage);
            quote_expression! { #texel.a }
        }
        tev::alpha::InputSrc::ChanAlpha => {
            let channel = input_channel(stage);
            quote_expression! { #channel.a }
        }
        tev::alpha::InputSrc::Constant => {
            let constant = constant(stage.alpha_const);
            quote_expression! { #constant.a }
        }
        tev::alpha::InputSrc::Zero => quote_expression! { 0f },
    }
}

fn comparative_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = input(stage, stage.ops.alpha.input_a());
    let input_b = input(stage, stage.ops.alpha.input_b());
    let input_c = input(stage, stage.ops.alpha.input_c());
    let input_d = input(stage, stage.ops.alpha.input_d());

    let target = stage.ops.alpha.compare_target();
    let op = stage.ops.alpha.compare_op();
    let clamp = stage.ops.alpha.clamp();
    let output = stage.ops.alpha.output().index();

    let compare_target_a = comparison_target(
        quote_expression!(input_a),
        quote_expression!(input_a_components),
        target,
    );
    let compare_target_b = comparison_target(
        quote_expression!(input_b),
        quote_expression!(input_b_components),
        target,
    );
    let comparison = match op {
        tev::ComparisonOp::GreaterThan => {
            quote_expression! { #compare_target_a > #compare_target_b }
        }
        tev::ComparisonOp::Equal => quote_expression! { #compare_target_a == #compare_target_b },
    };

    let clamped = if clamp {
        quote_expression! { alpha_compare }
    } else {
        quote_expression! { clamp(alpha_compare, 0f, 1f) }
    };

    wesl_quote::quote_statement! {
        {
            let input_a = #input_a;
            let input_a_components = vec3u(u32(#input_a * 255.0));
            let input_b = #input_b;
            let input_b_components = vec3u(u32(#input_b * 255.0));

            let input_c = #input_c;
            let input_d = #input_d;

            let alpha_compare = input_d + select(0, input_c, #comparison);
            let alpha_result = #clamped;

            regs[#output] = vec4f(regs[#output].rgb, alpha_result);
            last_alpha_output = #output;
        }
    }
}

fn regular_stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let input_a = input(stage, stage.ops.alpha.input_a());
    let input_b = input(stage, stage.ops.alpha.input_b());
    let input_c = input(stage, stage.ops.alpha.input_c());
    let input_d = input(stage, stage.ops.alpha.input_d());

    let sign = if stage.ops.alpha.negate() { -1.0 } else { 1.0 };
    let bias = stage.ops.alpha.bias().value();
    let scale = stage.ops.alpha.scale().value();
    let clamp = stage.ops.alpha.clamp();
    let output = stage.ops.alpha.output().index();

    let clamped = if clamp {
        quote_expression! { alpha_add_mul }
    } else {
        quote_expression! { clamp(alpha_add_mul, 0f, 1f) }
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

            let alpha_interpolation = sign * mix(input_a, input_b, input_c);
            let alpha_add_mul = scale * (alpha_interpolation + input_d + bias);
            let alpha_result = #clamped;

            regs[#output] = vec4f(regs[#output].rgb, alpha_result);
            last_alpha_output = #output;
        }
    }
}

pub fn stage(stage: &TexEnvStage) -> wesl::syntax::Statement {
    if stage.ops.alpha.is_comparative() {
        comparative_stage(stage)
    } else {
        regular_stage(stage)
    }
}
