mod texenv;
mod texgen;

use lazuli::system::gx::tev;
use wesl::{VirtualResolver, Wesl};
use wesl_quote::quote_declaration;

use crate::render::pipeline::ShaderSettings;
use crate::render::pipeline::settings::{TexEnvSettings, TexGenSettings};

fn base_module(settings: &ShaderSettings) -> wesl::syntax::TranslationUnit {
    use wesl::syntax::*;

    let interpolate = if settings.texenv.alpha_func.is_noop() {
        InterpolateAttribute {
            ty: InterpolationType::Perspective,
            sampling: Some(InterpolationSampling::Centroid),
        }
    } else {
        // although not needed, drastically improves the quality of alpha testing
        InterpolateAttribute {
            ty: InterpolationType::Perspective,
            sampling: Some(InterpolationSampling::Sample),
        }
    };

    let has_frag_depth = match settings.texenv.depth_tex.mode.op() {
        tev::depth::Op::Disabled | tev::depth::Op::Add => false,
        tev::depth::Op::Replace => true,
        _ => panic!("reserved depth tex mode"),
    };

    let fragment_out_struct = if has_frag_depth {
        quote_declaration! {
            struct FragmentOutput {
                @location(0) @blend_src(0) color: vec4f,
                @location(0) @blend_src(1) blend: vec4f,
                @builtin(frag_depth) depth: f32,
            }
        }
    } else {
        quote_declaration! {
            struct FragmentOutput {
                @location(0) @blend_src(0) color: vec4f,
                @location(0) @blend_src(1) blend: vec4f,
            }
        }
    };

    wesl_quote::quote_module! {
        alias MtxIdx = u32;

        const PLACEHOLDER_RGB: vec3f = vec3f(1.0, 0.0, 0.8627);
        const PLACEHOLDER_RGBA: vec4f = vec4f(1.0, 0.0, 0.8627, 0.5);

        struct Light {
            color: vec4f,

            cos_atten: vec3f,
            _pad0: u32,

            dist_atten: vec3f,
            _pad1: u32,

            position: vec3f,
            _pad2: u32,

            direction: vec3f,
            _pad3: u32,
        }

        struct Channel {
            material_from_vertex: u32,
            ambient_from_vertex: u32,
            lighting_enabled: u32,
            diffuse_attenuation: u32,
            attenuation: u32,
            specular: u32,
            light_mask: array<u32, 8>,
        }

        struct FogParams {
            color: vec4f,
            a: f32,
            b_mag: u32,
            b_shift: u32,
            c: f32,
        }

        struct Config {
            ambient: array<vec4f, 2>,
            material: array<vec4f, 2>,
            lights: array<Light, 8>,
            color_channels: array<Channel, 2>,
            alpha_channels: array<Channel, 2>,
            regs: array<vec4f, 4>,
            consts: array<vec4f, 4>,
            projection_mat: mat4x4f,
            post_transform_mat: array<mat4x4f, 8>,
            constant_alpha: u32,
            alpha_refs: array<u32, 2>,
            _pad0: u32,
            fog: FogParams,
        }

        // An input vertex
        struct Vertex {
            position: vec3f,
            config_idx: u32,
            normal: vec3f,
            _pad0: u32,

            position_mat: MtxIdx,
            normal_mat: MtxIdx,
            _pad1: u32,
            _pad2: u32,

            chan0: vec4f,
            chan1: vec4f,

            tex_coord: array<vec2f, 8>,
            tex_coord_mat: array<MtxIdx, 8>,
        };

        // Data group
        @group(0) @binding(0) var<storage> vertices: array<Vertex>;
        @group(0) @binding(1) var<storage> matrices: array<mat4x4f>;
        @group(0) @binding(2) var<storage> configs: array<Config>;

        // Textures group
        @group(1) @binding(0) var texture0: texture_2d<f32>;
        @group(1) @binding(1) var sampler0: sampler;
        @group(1) @binding(2) var texture1: texture_2d<f32>;
        @group(1) @binding(3) var sampler1: sampler;
        @group(1) @binding(4) var texture2: texture_2d<f32>;
        @group(1) @binding(5) var sampler2: sampler;
        @group(1) @binding(6) var texture3: texture_2d<f32>;
        @group(1) @binding(7) var sampler3: sampler;

        @group(1) @binding(8) var texture4: texture_2d<f32>;
        @group(1) @binding(9) var sampler4: sampler;
        @group(1) @binding(10) var texture5: texture_2d<f32>;
        @group(1) @binding(11) var sampler5: sampler;
        @group(1) @binding(12) var texture6: texture_2d<f32>;
        @group(1) @binding(13) var sampler6: sampler;
        @group(1) @binding(14) var texture7: texture_2d<f32>;
        @group(1) @binding(15) var sampler7: sampler;

        // Pipeline immediates
        struct PipelineImmediates {
            scaling: array<vec4f, 4>,
            lodbias: array<vec4f, 2>,
        }
        var<push_constant> pipeline_immediates: PipelineImmediates;

        // A vertex stage output
        struct VertexOutput {
            @builtin(position) clip: vec4f,
            @location(0) config_idx: u32,
            @#interpolate @location(1) chan0: vec4f,
            @#interpolate @location(2) chan1: vec4f,
            @#interpolate @location(3) tex_coord0: vec3f,
            @#interpolate @location(4) tex_coord1: vec3f,
            @#interpolate @location(5) tex_coord2: vec3f,
            @#interpolate @location(6) tex_coord3: vec3f,
            @#interpolate @location(7) tex_coord4: vec3f,
            @#interpolate @location(8) tex_coord5: vec3f,
            @#interpolate @location(9) tex_coord6: vec3f,
            @#interpolate @location(10) tex_coord7: vec3f,
        };

        const #fragment_out_struct: u32 = 0;

        fn vec3f_to_vec3u(value: vec3f) -> vec3u {
            return vec3u(
                u32(value.r * 255.0),
                u32(value.g * 255.0),
                u32(value.b * 255.0),
            );
        }

        fn vec4f_to_vec4u(value: vec4f) -> vec4u {
            return vec4u(
                u32(value.r * 255.0),
                u32(value.g * 255.0),
                u32(value.b * 255.0),
                u32(value.a * 255.0),
            );
        }

        fn concat_texgen_color(value: vec4f) -> vec3f {
            let int = vec4f_to_vec4u(value);
            let s = int.r;
            // yagcd says to concat green and blue..?
            let t = int.g;
            return vec3f(f32(s) / 255, f32(t) / 255, 1.0);
        }
    }
}

fn compute_channels() -> [wesl::syntax::GlobalDeclaration; 2] {
    use wesl::syntax::*;
    let color = wesl_quote::quote_declaration! {
        fn compute_color_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_color: vec3f, index: u32, config: base::Config) -> vec3f {
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
        fn compute_alpha_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_alpha: f32, index: u32, config: base::Config) -> f32 {
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

fn vertex_stage(texgen: &TexGenSettings) -> wesl::syntax::GlobalDeclaration {
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
                let matrix = base::matrices[vertex.tex_coord_mat[#index]];
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
        fn vs_main(@builtin(vertex_index) index: u32) -> base::VertexOutput {
            var out: base::VertexOutput;

            let vertex = base::vertices[index];
            let config = base::configs[vertex.config_idx];
            out.config_idx = vertex.config_idx;

            let vertex_local_pos = vec4f(vertex.position, 1.0);
            let vertex_world_pos = base::matrices[vertex.position_mat] * vertex_local_pos;
            var vertex_view_pos = config.projection_mat * vertex_world_pos;

            let vertex_local_norm = vec4f(vertex.normal, 0.0);
            let vertex_world_norm = normalize((base::matrices[vertex.normal_mat] * vertex_local_norm).xyz);

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

fn fragment_stage(texenv: &TexEnvSettings) -> wesl::syntax::GlobalDeclaration {
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

    let alpha_comparison = texenv::compute_alpha_comparison(&texenv.alpha_func);
    let depth_texture = texenv::compute_depth_texture(texenv);
    let fog = texenv::compute_fog(texenv);

    wesl_quote::quote_declaration! {
        @fragment
        fn fs_main(in: base::VertexOutput) -> base::FragmentOutput {
            let config = base::configs[in.config_idx];
            var last_color_output = 3u;
            var last_alpha_output = 3u;
            var regs: array<vec4f, 4> = config.regs;
            var consts: array<vec4f, 4> = config.consts;

            @#compute_stages {}

            let color = regs[last_color_output].rgb;
            let alpha = regs[last_alpha_output].a;

            let alpha_ref0 = f32(config.alpha_refs[0]) / 255.0;
            let alpha_ref1 = f32(config.alpha_refs[1]) / 255.0;

            if !(#alpha_comparison) {
                discard;
            }

            var out: base::FragmentOutput;
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

fn main_module(settings: &ShaderSettings) -> wesl::syntax::TranslationUnit {
    use wesl::syntax::*;

    let extensions = wesl_quote::quote_directive!(enable dual_source_blending;);
    let [color_chan, alpha_chan] = compute_channels();
    let vertex = vertex_stage(&settings.texgen);
    let fragment = fragment_stage(&settings.texenv);

    let mut module = wesl_quote::quote_module! {
        import package::base;

        const #color_chan = 0;
        const #alpha_chan = 0;

        const #vertex = 0;
        const #fragment = 0;
    };
    module.global_directives.push(extensions);

    module
}

pub fn compile(settings: &ShaderSettings) -> String {
    let mut resolver = VirtualResolver::new();
    resolver.add_translation_unit("package::base".parse().unwrap(), base_module(settings));
    resolver.add_translation_unit("package::main".parse().unwrap(), main_module(settings));

    let mut wesl = Wesl::new("shaders").set_custom_resolver(resolver);
    wesl.use_sourcemap(true);
    wesl.set_options(wesl::CompileOptions {
        imports: true,
        condcomp: false,
        generics: false,
        strip: true,
        lower: true,
        validate: true,
        ..Default::default()
    });

    let compiled = match wesl.compile(&"package::main".parse().unwrap()) {
        Ok(ok) => ok,
        Err(e) => {
            panic!("{e}");
        }
    };

    compiled.syntax.to_string()
}
