pub mod alpha;
pub mod color;

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

fn input_channel(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match stage.refs.input() {
        tev::InputChannel::Channel0 => quote_expression! { in.chan0 },
        tev::InputChannel::Channel1 => quote_expression! { in.chan1 },
        tev::InputChannel::AlphaBump => quote_expression! { vec4f(base::PLACEHOLDER_RGB, 0f) },
        tev::InputChannel::AlphaBumpNormalized => {
            quote_expression! { vec4f(base::PLACEHOLDER_RGB, 0f) }
        }
        tev::InputChannel::Zero => quote_expression! { vec4f(0f) },
        _ => panic!("reserved color channel"),
    }
}

fn constant(stage: &TexEnvStage) -> wesl::syntax::Expression {
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
        tev::Constant::Const0 => quote_expression! { consts[0] },
        tev::Constant::Const1 => quote_expression! { consts[1] },
        tev::Constant::Const2 => quote_expression! { consts[2] },
        tev::Constant::Const3 => quote_expression! { consts[3] },
        tev::Constant::Const0R => quote_expression! { consts[0].rrrr },
        tev::Constant::Const1R => quote_expression! { consts[1].rrrr },
        tev::Constant::Const2R => quote_expression! { consts[2].rrrr },
        tev::Constant::Const3R => quote_expression! { consts[3].rrrr },
        tev::Constant::Const0G => quote_expression! { consts[0].gggg },
        tev::Constant::Const1G => quote_expression! { consts[1].gggg },
        tev::Constant::Const2G => quote_expression! { consts[2].gggg },
        tev::Constant::Const3G => quote_expression! { consts[3].gggg },
        tev::Constant::Const0B => quote_expression! { consts[0].bbbb },
        tev::Constant::Const1B => quote_expression! { consts[1].bbbb },
        tev::Constant::Const2B => quote_expression! { consts[2].bbbb },
        tev::Constant::Const3B => quote_expression! { consts[3].bbbb },
        tev::Constant::Const0A => quote_expression! { consts[0].aaaa },
        tev::Constant::Const1A => quote_expression! { consts[1].aaaa },
        tev::Constant::Const2A => quote_expression! { consts[2].aaaa },
        tev::Constant::Const3A => quote_expression! { consts[3].aaaa },
        _ => panic!("reserved constant"),
    }
}

fn comparison_target(
    input_float: wesl::syntax::Expression,
    input_uint: wesl::syntax::Expression,
    target: tev::ComparisonTarget,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    match target {
        tev::ComparisonTarget::R8 => quote_expression! { (#input_uint).r },
        tev::ComparisonTarget::GR16 => {
            quote_expression! { pack4xU8(vec4u((#input_uint).r, (#input_uint).g, 0, 0)) }
        }
        tev::ComparisonTarget::BGR16 => {
            quote_expression! { pack4xU8(vec4u((#input_uint).r, (#input_uint).g, (#input_uint).b, 0)) }
        }
        tev::ComparisonTarget::Component => input_float,
    }
}

fn get_alpha_comparison_component(
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

pub fn compute_alpha_comparison(settings: &AlphaFuncSettings) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    let a = get_alpha_comparison_component(settings.comparison[0], 0);
    let b = get_alpha_comparison_component(settings.comparison[1], 1);

    match settings.logic {
        tev::alpha::CompareLogic::And => quote_expression! { (#a) && (#b) },
        tev::alpha::CompareLogic::Or => quote_expression! { (#a) || (#b) },
        tev::alpha::CompareLogic::Xor => quote_expression! { (#a) != (#b) },
        tev::alpha::CompareLogic::Xnor => quote_expression! { (#a) == (#b) },
    }
}

pub fn compute_depth_texture(settings: &TexEnvSettings) -> wesl::syntax::Statement {
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
            frag_depth = out.depth;
        }
    }
}

pub fn compute_fog(settings: &TexEnvSettings) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    if settings.fog.mode == tev::FogMode::None {
        return Statement::Void;
    }

    let distance = if settings.fog.orthographic {
        quote_statement! {
            distance = clamp(config.fog.a * frag_depth - config.fog.c, 0.0, 1.0);
        }
    } else {
        quote_statement! {
            {
                let depth_max = f32((1 << 24) - 1);
                let depth = u32(frag_depth * depth_max);
                let a = config.fog.a * depth_max;
                let denom = f32(config.fog.b_mag - (depth >> config.fog.b_shift));
                distance = clamp(a / denom - config.fog.c, 0.0, 1.0);
            }
        }
    };

    let adjust = match settings.fog.mode {
        tev::FogMode::None => unreachable!(),
        tev::FogMode::Linear => Statement::Void,
        tev::FogMode::Exponential => quote_statement! {
            distance = 1f - pow(2f, -8f * distance);
        },
        tev::FogMode::ExponentialSquared => quote_statement! {
            {
                let squared = pow(distance, 2f);
                distance = 1f - pow(2f, -8f * squared);
            }
        },
        tev::FogMode::InverseExponential => quote_statement! {
            {
                let inverse = (1f - distance);
                distance = pow(2f, -8f * inverse);
            }
        },
        tev::FogMode::InverseExponentialSquared => quote_statement! {
            {
                let inverse_squared = pow(1f - distance, 2f);
                distance = pow(2f, -8f * inverse_squared);
            }
        },
        _ => panic!("reserved fog mode"),
    };

    quote_statement! {
        {
            var distance: f32;
            @#distance {}
            @#adjust {}
            out.color = mix(out.color, config.fog.color, distance);
        }
    }
}
