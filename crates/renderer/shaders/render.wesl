import package::common;
import package::render::fog;

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

struct Config {
    ambient: array<vec4f, 2>,
    material: array<vec4f, 2>,
    lights: array<Light, 8>,
    color_channels: array<Channel, 2>,
    alpha_channels: array<Channel, 2>,
    regs: array<vec4f, 4>,
    consts: array<vec4f, 4>,
    projection_mtx: mat4x4f,
    post_transform_mtx: array<mat4x4f, 8>,
    constant_alpha: u32,
    alpha_refs: array<u32, 2>,
    _pad0: u32,
    fog: fog::Params,
}

struct VertexInput {
    position: vec3f,
    config_idx: u32,
    normal: vec3f,
    _pad0: u32,

    position_mtx_idx: u32,
    normal_mtx_idx: u32,
    _pad1: u32,
    _pad2: u32,

    chan0: vec4f,
    chan1: vec4f,

    tex_coord: array<vec2f, 8>,
    tex_coord_mtx_idx: array<u32, 8>,
};

struct PipelineImmediates {
    scaling: array<vec4f, 4>,
    lodbias: array<vec4f, 2>,
}

var<push_constant> pipeline_immediates: PipelineImmediates;

// Data group
@group(0) @binding(0) var<storage> vertices: array<VertexInput>;
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

@if(sample_shading)
struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) config_idx: u32,
    @interpolate(perspective, sample) @location(1) chan0: vec4f,
    @interpolate(perspective, sample) @location(2) chan1: vec4f,
    @interpolate(perspective, sample) @location(3) tex_coord0: vec3f,
    @interpolate(perspective, sample) @location(4) tex_coord1: vec3f,
    @interpolate(perspective, sample) @location(5) tex_coord2: vec3f,
    @interpolate(perspective, sample) @location(6) tex_coord3: vec3f,
    @interpolate(perspective, sample) @location(7) tex_coord4: vec3f,
    @interpolate(perspective, sample) @location(8) tex_coord5: vec3f,
    @interpolate(perspective, sample) @location(9) tex_coord6: vec3f,
    @interpolate(perspective, sample) @location(10) tex_coord7: vec3f,
};

@if(!sample_shading)
struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) config_idx: u32,
    @interpolate(perspective, centroid) @location(1) chan0: vec4f,
    @interpolate(perspective, centroid) @location(2) chan1: vec4f,
    @interpolate(perspective, centroid) @location(3) tex_coord0: vec3f,
    @interpolate(perspective, centroid) @location(4) tex_coord1: vec3f,
    @interpolate(perspective, centroid) @location(5) tex_coord2: vec3f,
    @interpolate(perspective, centroid) @location(6) tex_coord3: vec3f,
    @interpolate(perspective, centroid) @location(7) tex_coord4: vec3f,
    @interpolate(perspective, centroid) @location(8) tex_coord5: vec3f,
    @interpolate(perspective, centroid) @location(9) tex_coord6: vec3f,
    @interpolate(perspective, centroid) @location(10) tex_coord7: vec3f,
};

@if(frag_depth)
struct FragmentOutput {
    @location(0) @blend_src(0) color: vec4f,
    @location(0) @blend_src(1) blend: vec4f,
    @builtin(frag_depth) depth: f32,
}

@if(!frag_depth)
struct FragmentOutput {
    @location(0) @blend_src(0) color: vec4f,
    @location(0) @blend_src(1) blend: vec4f,
}

fn concat_texgen_color(value: vec4f) -> vec3f {
    let int = common::vec4f_to_vec4u(value);
    let s = int.r;
    // yagcd says to concat green and blue..?
    let t = int.g;
    return vec3f(f32(s) / 255, f32(t) / 255, 1.0);
}
