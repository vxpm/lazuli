//! Data types used for CPU-GPU communication.

use bitos::bitos;
use glam::{Mat4, Vec2, Vec3};
use lazuli::system::gx::color::Rgba;
use lazuli::system::gx::xform::DiffuseAttenuation;
use lazuli::system::gx::{tev, xform};
use zerocopy::{Immutable, IntoBytes};

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub config_idx: u32,
    pub normal: Vec3,
    pub _pad0: u32,

    pub position_mtx_idx: u32,
    pub normal_mtx_idx: u32,
    pub _pad1: u32,
    pub _pad2: u32,

    pub chan0: Rgba,
    pub chan1: Rgba,

    pub tex_coord: [Vec2; 8],
    pub tex_coord_mtx_idx: [u32; 8],
}

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct Light {
    pub color: Rgba,

    pub cos_attenuation: Vec3,
    pub _pad0: u32,

    pub dist_attenuation: Vec3,
    pub _pad1: u32,

    pub position: Vec3,
    pub _pad2: u32,

    pub direction: Vec3,
    pub _pad3: u32,
}

impl Light {
    pub fn update(&mut self, light: xform::Light) {
        self.color = light.color.into();
        self.cos_attenuation = light.cos_attenuation;
        self.dist_attenuation = light.dist_attenuation;
        self.position = light.position;
        self.direction = light.direction;
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Default)]
pub struct Channel {
    #[bits(0)]
    pub material_from_vertex: bool,
    #[bits(1)]
    pub ambient_from_vertex: bool,
    #[bits(2)]
    pub lighting_enabled: bool,
    #[bits(3..5)]
    pub diffuse_atten: DiffuseAttenuation,
    #[bits(5)]
    pub position_atten: bool,
    #[bits(6)]
    pub specular: bool,
    #[bits(7..15)]
    pub light_mask: [bool; 8],
}

impl Channel {
    pub fn update(&mut self, channel: xform::Channel) {
        self.set_material_from_vertex(channel.material_from_vertex());
        self.set_ambient_from_vertex(channel.ambient_from_vertex());
        self.set_lighting_enabled(channel.lighting_enabled());
        self.set_diffuse_atten(channel.diffuse_atten());
        self.set_position_atten(channel.position_atten());
        self.set_specular(!channel.not_specular());

        let a = channel.lights0to3();
        let b = channel.lights4to7();
        self.set_light_mask([a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]]);
    }
}

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct FogParams {
    pub color: Rgba,
    pub a: f32,
    pub b_mag: u32,
    pub b_shift: u32,
    pub c: f32,
}

impl FogParams {
    pub fn update(&mut self, fog: tev::Fog) {
        self.color = fog.color.into();
        self.a = fog.value_a();
        self.b_mag = fog.b0.magnitude().value();
        self.b_shift = fog.b1.shift().value() as u32;
        self.c = fog.value_c();
    }
}

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct Config {
    pub ambient: [Rgba; 2],
    pub material: [Rgba; 2],
    pub lights: [Light; 8],
    pub color_channels: [Channel; 2],
    pub alpha_channels: [Channel; 2],
    pub regs: [Rgba; 4],
    pub consts: [Rgba; 4],
    pub projection_mtx: Mat4,
    pub post_transform_mtx: [Mat4; 8],
    pub constant_alpha: u32,
    pub alpha_refs: [u32; 2],
    pub _pad0: u32,
    pub fog: FogParams,
}
