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

fn compute_channels() -> [wesl::syntax::GlobalDeclaration; 2] {
    use wesl::syntax::*;
    let color = wesl_quote::quote_declaration! {
        fn compute_color_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_color: vec3f, index: u32, config: render::Config) -> vec3f {
            let channel = config.color_channels[index];

            // get material color
            var material = config.material[index].rgb;
            if channel.material_from_vertex != 0 {
                material = vertex_color;
            }

            // if no lighting, return
            if channel.lighting_enabled == 0 {
                return material;
            }

            // get ambient color
            var ambient = config.ambient[index].rgb;
            if channel.ambient_from_vertex != 0 {
                ambient = vertex_color;
            }

            var light_func = ambient;
            for (var light_idx = 0; light_idx < 8; light_idx += 1) {
                if channel.light_mask[light_idx] == 0 {
                    continue;
                }

                let light = config.lights[light_idx];

                // compute diffuse attenuation
                var diff_atten: f32;
                switch channel.diffuse_attenuation {
                    case 0: {
                        diff_atten = 1.0;
                    }
                    case 1: {
                        let vertex_to_light = light.position - vertex_pos;
                        let dot_product = dot(vertex_to_light, vertex_normal);
                        diff_atten = dot_product / length(vertex_to_light);
                    }
                    case 2: {
                        let vertex_to_light = light.position - vertex_pos;
                        let dot_product = dot(vertex_to_light, vertex_normal);
                        diff_atten = max(dot_product / length(vertex_to_light), 0.0);
                    }
                    default: {}
                }

                // compute angle and distance attenuation
                var atten: f32 = 1.0;
                if channel.attenuation != 0 {
                    if channel.specular == 0 {
                        let vertex_to_light = light.position - vertex_pos;
                        let vertex_to_light_dir = normalize(vertex_to_light);

                        let cos = max(dot(vertex_to_light_dir, light.direction), 0.0);
                        let dist = length(vertex_to_light);

                        let ang_atten = max(light.cos_atten.x + cos * light.cos_atten.y + cos * cos * light.cos_atten.z, 0.0);
                        let dist_atten = light.dist_atten.x + dist * light.dist_atten.y + dist * dist * light.dist_atten.z;

                        atten = ang_atten / dist_atten;
                    } else {
                        let l = normalize(light.position);
                        let h = light.direction;
                        let norm_dot_l = dot(vertex_normal, l);

                        var value = 0.0;
                        if norm_dot_l > 0 {
                            let norm_dot_h = dot(vertex_normal, h);
                            value = max(norm_dot_h, 0.0);
                        }

                        let ang_atten = max(light.cos_atten.x + value * light.cos_atten.y + value * value * light.cos_atten.z, 0.0);
                        let dist_atten = light.dist_atten.x + value * light.dist_atten.y + value * value * light.dist_atten.z;

                        atten = ang_atten / dist_atten;
                    }
                }

                light_func += light.color.rgb * diff_atten * atten;
            }

            return material * clamp(light_func, vec3f(0.0), vec3f(1.0));
        }
    };

    let alpha = wesl_quote::quote_declaration! {
        fn compute_alpha_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_alpha: f32, index: u32, config: render::Config) -> f32 {
            let channel = config.alpha_channels[index];

            // get material alpha
            var material = config.material[index].a;
            if channel.material_from_vertex != 0 {
                material = vertex_alpha;
            }

            // if no lighting, return
            if channel.lighting_enabled == 0 {
                return material;
            }

            // get ambient alpha
            var ambient = config.ambient[index].a;
            if channel.ambient_from_vertex != 0 {
                ambient = vertex_alpha;
            }

            var light_func = ambient;
            for (var light_idx = 0; light_idx < 8; light_idx += 1) {
                if channel.light_mask[light_idx] == 0 {
                    continue;
                }

                let light = config.lights[light_idx];

                // compute diffuse attenuation
                var diff_atten: f32;
                switch channel.diffuse_attenuation {
                    case 0: {
                        diff_atten = 1.0;
                    }
                    case 1: {
                        let vertex_to_light = light.position - vertex_pos;
                        let dot_product = dot(vertex_to_light, vertex_normal);
                        diff_atten = dot_product / length(vertex_to_light);
                    }
                    case 2: {
                        let vertex_to_light = light.position - vertex_pos;
                        let dot_product = dot(vertex_to_light, vertex_normal);
                        diff_atten = max(dot_product / length(vertex_to_light), 0.0);
                    }
                    default: {}
                }

                // compute angle and distance attenuation
                var atten: f32 = 1.0;
                if channel.attenuation != 0 {
                    if channel.specular == 0 {
                        let l = light.position - vertex_pos;
                        let cos = max(dot(normalize(l), light.direction), 0.0);
                        let dist = length(l);

                        let ang_atten = max(light.cos_atten.x + cos * light.cos_atten.y + cos * cos * light.cos_atten.z, 0.0);
                        let dist_atten = light.dist_atten.x + dist * light.dist_atten.y + dist * dist * light.dist_atten.z;

                        atten = ang_atten / dist_atten;
                    } else {
                        let l = normalize(light.position);
                        let h = light.direction;
                        let norm_dot_l = dot(vertex_normal, l);

                        var value = 0.0;
                        if norm_dot_l > 0 {
                            let norm_dot_h = dot(vertex_normal, h);
                            value = max(norm_dot_h, 0.0);
                        }

                        let ang_atten = max(light.cos_atten.x + value * light.cos_atten.y + value * value * light.cos_atten.z, 0.0);
                        let dist_atten = light.dist_atten.x + value * light.dist_atten.y + value * value * light.dist_atten.z;

                        atten = ang_atten / dist_atten;
                    }
                }

                light_func += light.color.a * diff_atten * atten;
            }

            return material * clamp(light_func, 0.0, 1.0);
        }
    };

    [color, alpha]
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
                compute_color_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan0.rgb, 0, config),
                compute_alpha_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan0.a, 0, config),
            );
            out.chan1 = vec4f(
                compute_color_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan1.rgb, 1, config),
                compute_alpha_channel(vertex_world_pos.xyz, vertex_world_norm, vertex.chan1.a, 1, config),
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
    let [color_chan, alpha_chan] = compute_channels();
    let vertex = vertex_stage(&config.texgen);
    let fragment = fragment_stage(&config.texenv);

    let mut module = wesl_quote::quote_module! {
        import package::common;
        import package::render;

        const #color_chan = 0;
        const #alpha_chan = 0;

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
        "package::render".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/render.wesl")),
    );
    resolver.add_module(
        "package::common".parse().unwrap(),
        Cow::Borrowed(include_str!("../../../shaders/common.wesl")),
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
