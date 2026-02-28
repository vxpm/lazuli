pub mod alpha;
pub mod color;

use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev;
use wesl_quote::{quote_expression, quote_statement};

use crate::render::pipeline::shader::TexEnvConfig;

fn sample_tex(stage: &TexEnvStage) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    let map = stage.refs.map().value() as u32;
    let coord = stage.refs.coord().value() as u32;

    let tex_ident = wesl::syntax::Ident::new(format!("render::texture{map}"));
    let sampler_ident = wesl::syntax::Ident::new(format!("render::sampler{map}"));
    let coord_ident = wesl::syntax::Ident::new(format!("in.tex_coord{coord}"));
    let pipeline_immediates_ident = wesl::syntax::Ident::new("render::pipeline_immediates".into());

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
        tev::InputChannel::AlphaBump => quote_expression! { vec4f(common::PLACEHOLDER_RGB, 0f) },
        tev::InputChannel::AlphaBumpNormalized => {
            quote_expression! { vec4f(render::PLACEHOLDER_RGB, 0f) }
        }
        tev::InputChannel::Zero => quote_expression! { vec4f(0f) },
        _ => panic!("reserved color channel"),
    }
}

fn constant(constant: tev::Constant) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match constant {
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
    target: tev::ComparisonTarget,
    input: wesl::syntax::Expression,
    input_bytes: wesl::syntax::Expression,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;

    match target {
        tev::ComparisonTarget::R8 => quote_expression! { (#input_bytes).r },
        tev::ComparisonTarget::GR16 => {
            quote_expression! { pack4xU8(vec4u((#input_bytes).r, (#input_bytes).g, 0, 0)) }
        }
        tev::ComparisonTarget::BGR16 => {
            quote_expression! { pack4xU8(vec4u((#input_bytes).r, (#input_bytes).g, (#input_bytes).b, 0)) }
        }
        tev::ComparisonTarget::Component => input,
    }
}

pub fn compute_depth_texture(config: &TexEnvConfig) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    if matches!(
        config.depth_tex.mode.op(),
        tev::depth::Op::Disabled | tev::depth::Op::Add
    ) {
        return Statement::Void;
    }

    let bias = config.depth_tex.bias;
    let sampled = self::sample_tex(config.stages.last().unwrap());
    let (depth_mid, depth_hi, depth_max) = match config.depth_tex.mode.format() {
        tev::depth::Format::U8 => (
            quote_expression!(0),
            quote_expression!(0),
            quote_expression!(255.0),
        ),
        tev::depth::Format::U16 => (
            quote_expression!(depth_tex_sample.y),
            quote_expression!(0),
            quote_expression!(65535.0),
        ),
        tev::depth::Format::U24 => (
            quote_expression!(depth_tex_sample.y),
            quote_expression!(depth_tex_sample.z),
            quote_expression!(16777215.0),
        ),
        _ => panic!("reserved format"),
    };

    quote_statement! {
        {
            let depth_tex_sample = common::vec4f_to_vec4u(#sampled);
            let depth_tex_value = pack4xU8(vec4u(depth_tex_sample.x, #depth_mid, #depth_hi, 0)) + #bias;
            out.depth = clamp(f32(depth_tex_value) / #depth_max, 0.0, 1.0);
            frag_depth = out.depth;
        }
    }
}

pub fn compute_fog(config: &TexEnvConfig) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    if config.fog.mode == tev::FogMode::None {
        return Statement::Void;
    }

    let distance = if config.fog.orthographic {
        quote_statement! {
            distance = render::fog::orthographic_distance(config.fog, frag_depth);
        }
    } else {
        quote_statement! {
            distance = render::fog::perspective_distance(config.fog, frag_depth);
        }
    };

    let adjust = match config.fog.mode {
        tev::FogMode::None => unreachable!(),
        tev::FogMode::Linear => Statement::Void,
        tev::FogMode::Exponential => quote_statement! {
            distance = render::fog::exponential(distance);
        },
        tev::FogMode::ExponentialSquared => quote_statement! {
            distance = render::fog::exponential_squared(distance);
        },
        tev::FogMode::InverseExponential => quote_statement! {
            distance = render::fog::inverse_exponential(distance);
        },
        tev::FogMode::InverseExponentialSquared => quote_statement! {
            distance = render::fog::inverse_exponential_squared(distance);
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
