mod texenv;
mod texgen;

use std::borrow::Cow;

use lazuli::modules::render::TexEnvStage;
use lazuli::system::gx::tev::{self, FogMode};
use lazuli::system::gx::xform::BaseTexGen;
use wesl::{VirtualResolver, Wesl};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlphaComparisonValue {
    False,
    True,
    Unknown,
}

impl AlphaComparisonValue {
    pub fn new(comparison: tev::alpha::Comparison) -> Self {
        match comparison {
            tev::alpha::Comparison::Never => Self::False,
            tev::alpha::Comparison::Always => Self::True,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitAnd for AlphaComparisonValue {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, Self::True) => Self::True,
            (Self::False, _) => Self::False,
            (_, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitOr for AlphaComparisonValue {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, _) => Self::True,
            (_, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::BitXor for AlphaComparisonValue {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::True, Self::False) => Self::True,
            (Self::False, Self::True) => Self::True,
            (Self::True, Self::True) => Self::False,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

impl std::ops::Not for AlphaComparisonValue {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::False => Self::True,
            Self::True => Self::False,
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AlphaTestConfig {
    pub comparison: [tev::alpha::Comparison; 2],
    pub logic: tev::alpha::ComparisonLogic,
}

impl AlphaTestConfig {
    /// Returns whether this configuration is trivially passable (i.e. never discards).
    pub fn is_noop(&self) -> bool {
        let lhs = AlphaComparisonValue::new(self.comparison[0]);
        let rhs = AlphaComparisonValue::new(self.comparison[1]);

        let result = match self.logic {
            tev::alpha::ComparisonLogic::And => lhs & rhs,
            tev::alpha::ComparisonLogic::Or => lhs | rhs,
            tev::alpha::ComparisonLogic::Xor => lhs ^ rhs,
            tev::alpha::ComparisonLogic::Xnor => !(lhs ^ rhs),
        };

        result == AlphaComparisonValue::True
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FogConfig {
    pub mode: FogMode,
    pub orthographic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexEnvConfig {
    pub stages: Vec<TexEnvStage>,
    pub alpha_test: AlphaTestConfig,
    pub depth_tex: tev::depth::Texture,
    pub fog: FogConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexGenStageConfig {
    pub base: BaseTexGen,
    pub normalize: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TexGenConfig {
    pub stages: Vec<TexGenStageConfig>,
}

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Config {
    pub texenv: TexEnvConfig,
    pub texgen: TexGenConfig,
}

fn vertex_stage(texgen: &TexGenConfig) -> wesl::syntax::GlobalDeclaration {
    use wesl::syntax::*;

    let mut stages = vec![];
    for (index, stage) in texgen.stages.iter().enumerate() {
        let index = index as u32;

        let source = texgen::get_source(stage.base.source(), stage.base.kind());
        let input = texgen::get_input(stage.base.input_kind(), source);
        let transformed = texgen::transform(stage.base.kind(), input);
        let output = texgen::get_output(stage.base.output_kind(), transformed);
        let normalized = texgen::normalize(stage.normalize, output);
        let result = texgen::post_transform(index, normalized);

        stages.push(wesl_quote::quote_statement! {
            {
                let matrix = render::matrices[vertex.tex_coord_mtx_idx[#index]];
                tex_coords[#index] = #result;
            }
        });
    }

    stages.resize(16, wesl_quote::quote_statement!({}));
    let [
        s0,
        s1,
        s2,
        s3,
        s4,
        s5,
        s6,
        s7,
        s8,
        s9,
        s10,
        s11,
        s12,
        s13,
        s14,
        s15,
    ] = stages.try_into().unwrap();

    let compute_stages = wesl_quote::quote_statement!({
        @#s0 {}
        @#s1 {}
        @#s2 {}
        @#s3 {}
        @#s4 {}
        @#s5 {}
        @#s6 {}
        @#s7 {}
        @#s8 {}
        @#s9 {}
        @#s10 {}
        @#s11 {}
        @#s12 {}
        @#s13 {}
        @#s14 {}
        @#s15 {}
    });

    wesl_quote::quote_declaration! {
        @vertex
        fn vs_main(@builtin(vertex_index) index: u32) -> render::VertexOutput {
            var out: render::VertexOutput;

            let vertex = render::vertices[index];
            let config = render::configs[vertex.config_idx];
            out.config_idx = vertex.config_idx;

            let vertex_local_pos = vec4f(vertex.position, 1.0);
            let vertex_world_pos = render::matrices[vertex.position_mtx_idx] * vertex_local_pos;
            var vertex_view_pos = config.projection_mtx * vertex_world_pos;

            let vertex_local_norm = vec4f(vertex.normal, 0.0);
            let vertex_world_norm = normalize((render::matrices[vertex.normal_mtx_idx] * vertex_local_norm).xyz);

            // GameCube's normalized device coordinates are -1.0..1.0 in x/y and -1.0..0.0 in z,
            // while wgpu's normalized device coordinates are -1.0..1.0 in x/y and 0.0..1.0 in z.
            //
            // Therefore, we add the w component to z in order to convert it to the correct range.
            out.clip = vertex_view_pos;
            out.clip.z += out.clip.w;

            out.chan0 = vec4f(
                render::lighting::color_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan0.rgb, 0, config),
                render::lighting::alpha_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan0.a, 0, config),
            );
            out.chan1 = vec4f(
                render::lighting::color_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan1.rgb, 1, config),
                render::lighting::alpha_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan1.a, 1, config),
            );

            var tex_coords: array<vec3f, 8>;
            @#compute_stages {}

            out.tex_coord0 = tex_coords[0];
            out.tex_coord1 = tex_coords[1];
            out.tex_coord2 = tex_coords[2];
            out.tex_coord3 = tex_coords[3];
            out.tex_coord4 = tex_coords[4];
            out.tex_coord5 = tex_coords[5];
            out.tex_coord6 = tex_coords[6];
            out.tex_coord7 = tex_coords[7];

            return out;
        }
    }
}

fn fragment_stage(texenv: &TexEnvConfig) -> wesl::syntax::GlobalDeclaration {
    use wesl::syntax::*;

    let mut stages = vec![];
    for stage in texenv.stages.iter() {
        let color = texenv::color::stage(stage);
        let alpha = texenv::alpha::stage(stage);

        stages.push(wesl_quote::quote_statement! {
            {
                @#color {}
                @#alpha {}
            }
        });
    }

    stages.resize(16, Statement::Void);
    let [
        s0,
        s1,
        s2,
        s3,
        s4,
        s5,
        s6,
        s7,
        s8,
        s9,
        s10,
        s11,
        s12,
        s13,
        s14,
        s15,
    ] = stages.try_into().unwrap();

    let compute_stages = wesl_quote::quote_statement!({
        @#s0 {}
        @#s1 {}
        @#s2 {}
        @#s3 {}
        @#s4 {}
        @#s5 {}
        @#s6 {}
        @#s7 {}
        @#s8 {}
        @#s9 {}
        @#s10 {}
        @#s11 {}
        @#s12 {}
        @#s13 {}
        @#s14 {}
        @#s15 {}
    });

    let alpha_test = texenv::alpha::compute_test(&texenv.alpha_test);
    let depth_texture = texenv::compute_depth_texture(texenv);
    let fog = texenv::compute_fog(texenv);

    wesl_quote::quote_declaration! {
        @fragment
        fn fs_main(in: render::VertexOutput) -> render::FragmentOutput {
            let config = render::configs[in.config_idx];
            var last_color_output = 3u;
            var last_alpha_output = 3u;
            var regs: array<vec4f, 4> = config.regs;
            var consts: array<vec4f, 4> = config.consts;

            @#compute_stages {}

            let color = regs[last_color_output].rgb;
            let alpha = regs[last_alpha_output].a;

            let alpha_ref0 = f32(config.alpha_refs[0]) / 255.0;
            let alpha_ref1 = f32(config.alpha_refs[1]) / 255.0;

            if !(#alpha_test) {
                discard;
            }

            var out: render::FragmentOutput;
            out.blend = vec4f(regs[last_color_output].rgb, regs[last_alpha_output].a);
            if config.constant_alpha < 256 {
                out.color = vec4f(regs[last_color_output].rgb, f32(config.constant_alpha) / 255.0);
            } else {
                out.color = out.blend;
            }

            var frag_depth = in.clip.z;
            @#depth_texture {}
            @#fog {}

            return out;
        }
    }
}

fn main_module(config: &Config) -> wesl::syntax::TranslationUnit {
    use wesl::syntax::*;

    let extensions = wesl_quote::quote_directive!(enable dual_source_blending;);
    let vertex = vertex_stage(&config.texgen);
    let fragment = fragment_stage(&config.texenv);

    let mut module = wesl_quote::quote_module! {
        import package::common;
        import package::render;

        const #vertex = 0;
        const #fragment = 0;
    };
    module.global_directives.push(extensions);

    module
}

pub fn compile(config: &Config) -> String {
    let mut resolver = VirtualResolver::new();
    resolver.add_translation_unit("package::main".parse().unwrap(), main_module(config));
    resolver.add_module(
        "package::common".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/common.wesl")),
    );

    resolver.add_module(
        "package::render".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/render.wesl")),
    );
    resolver.add_module(
        "package::render::lighting".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/render/lighting.wesl")),
    );
    resolver.add_module(
        "package::render::fog".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/render/fog.wesl")),
    );

    let mut wesl = Wesl::new("shaders").set_custom_resolver(resolver);
    wesl.use_sourcemap(true);
    wesl.set_options(wesl::CompileOptions {
        imports: true,
        condcomp: true,
        generics: false,
        strip: true,
        lower: true,
        validate: true,
        ..Default::default()
    });

    let needs_frag_depth = match config.texenv.depth_tex.mode.op() {
        tev::depth::Op::Disabled | tev::depth::Op::Add => false,
        tev::depth::Op::Replace => true,
        _ => panic!("reserved depth tex mode"),
    };

    wesl.set_feature("sample_shading", config.texenv.alpha_test.is_noop());
    wesl.set_feature("frag_depth", needs_frag_depth);

    let compiled = match wesl.compile(&"package::main".parse().unwrap()) {
        Ok(ok) => ok,
        Err(e) => {
            panic!("{e}");
        }
    };

    compiled.syntax.to_string()
}
