use lazuli::system::gx::xform::{TexGenInputKind, TexGenKind, TexGenOutputKind, TexGenSource};
use wesl_quote::quote_expression;

pub fn get_source(source: TexGenSource, kind: TexGenKind) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match source {
        TexGenSource::Position => quote_expression! { vertex.position },
        TexGenSource::Normal => quote_expression! { vertex.normal },
        TexGenSource::Color => match kind {
            TexGenKind::ColorDiffuse => quote_expression! { vertex.chan0 },
            TexGenKind::ColorSpecular => quote_expression! { vertex.chan1 },
            _ => panic!("invalid texgen config"),
        },
        TexGenSource::TexCoord0 => quote_expression! { vec3f(vertex.tex_coord[0], 1.0) },
        TexGenSource::TexCoord1 => quote_expression! { vec3f(vertex.tex_coord[1], 1.0) },
        TexGenSource::TexCoord2 => quote_expression! { vec3f(vertex.tex_coord[2], 1.0) },
        TexGenSource::TexCoord3 => quote_expression! { vec3f(vertex.tex_coord[3], 1.0) },
        TexGenSource::TexCoord4 => quote_expression! { vec3f(vertex.tex_coord[4], 1.0) },
        TexGenSource::TexCoord5 => quote_expression! { vec3f(vertex.tex_coord[5], 1.0) },
        TexGenSource::TexCoord6 => quote_expression! { vec3f(vertex.tex_coord[6], 1.0) },
        TexGenSource::TexCoord7 => quote_expression! { vec3f(vertex.tex_coord[7], 1.0) },
        TexGenSource::BinormalT => todo!(),
        TexGenSource::BinormalB => todo!(),
        _ => panic!("reserved texgen source"),
    }
}

pub fn get_input(
    format: TexGenInputKind,
    source: wesl::syntax::Expression,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match format {
        TexGenInputKind::AB11 => quote_expression! { vec4f(#source.xy, 1.0, 1.0) },
        TexGenInputKind::ABC1 => quote_expression! { vec4f(#source, 1.0) },
    }
}

pub fn transform(kind: TexGenKind, input: wesl::syntax::Expression) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match kind {
        TexGenKind::Transform => quote_expression! { (matrix * #input).xyz },
        // TODO: terrible stub (emboss)
        TexGenKind::Emboss => quote_expression! { (#input).xyz },
        TexGenKind::ColorDiffuse | TexGenKind::ColorSpecular => quote_expression! {
            render::concat_texgen_color(#input)
        },
    }
}

pub fn get_output(
    format: TexGenOutputKind,
    transformed: wesl::syntax::Expression,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match format {
        TexGenOutputKind::Vec2 => quote_expression! { vec3f(#transformed.xy, 1.0) },
        TexGenOutputKind::Vec3 => transformed,
    }
}

pub fn normalize(normalize: bool, output: wesl::syntax::Expression) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    if normalize {
        quote_expression! { normalize(#output) }
    } else {
        output
    }
}

pub fn post_transform(
    stage_index: u32,
    normalized: wesl::syntax::Expression,
) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    quote_expression! { (config.post_transform_mtx[#stage_index] * vec4f(#normalized, 1.0)).xyz }
}
