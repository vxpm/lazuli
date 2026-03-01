use lazuli::system::gx::xform::{TexGenInputKind, TexGenKind, TexGenOutputKind, TexGenSource};
use wesl_quote::{quote_expression, quote_statement};

use crate::render::pipeline::shader::TexGenStageConfig;

fn source(source: TexGenSource, kind: TexGenKind) -> wesl::syntax::Expression {
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

fn input(format: TexGenInputKind) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match format {
        TexGenInputKind::AB11 => quote_expression! { vec4f(source.xy, 1.0, 1.0) },
        TexGenInputKind::ABC1 => quote_expression! { vec4f(source, 1.0) },
    }
}

fn transform(kind: TexGenKind) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match kind {
        TexGenKind::Transform => quote_expression! { (matrix * input).xyz },
        // TODO: terrible stub (emboss)
        TexGenKind::Emboss => quote_expression! { (input).xyz },
        TexGenKind::ColorDiffuse | TexGenKind::ColorSpecular => quote_expression! {
            render::concat_texgen_color(input)
        },
    }
}

fn output(format: TexGenOutputKind) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    match format {
        TexGenOutputKind::Vec2 => quote_expression! { vec3f(transformed.xy, 1.0) },
        TexGenOutputKind::Vec3 => quote_expression! { transformed },
    }
}

fn normalize(normalize: bool) -> wesl::syntax::Expression {
    use wesl::syntax::*;
    if normalize {
        quote_expression! { normalize(output) }
    } else {
        quote_expression! { output }
    }
}

pub fn stage(stage: &TexGenStageConfig, index: u32) -> wesl::syntax::Statement {
    use wesl::syntax::*;

    let source = source(stage.base.source(), stage.base.kind());
    let input = input(stage.base.input_kind());
    let transformed = transform(stage.base.kind());
    let output = output(stage.base.output_kind());
    let normalized = normalize(stage.normalize);

    quote_statement! {
        {
            let matrix = render::matrices[vertex.tex_coord_mtx_idx[#index]];
            let post_matrix = config.post_transform_mtx[#index];

            let source = #source;
            let input = #input;
            let transformed = #transformed;
            let output = #output;
            let normalized = #normalized;

            tex_coords[#index] = (post_matrix * vec4f(normalized, 1.0)).xyz;
        }
    }
}
