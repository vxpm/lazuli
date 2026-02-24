//! Data types used for CPU-GPU communication.

use glam::{Mat4, Vec2, Vec3};
use lazuli::system::gx::color::Rgba;
use lazuli::system::gx::{tev, xform};
use zerocopy::{Immutable, IntoBytes};

pub type MatrixIdx = u32;

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub config_idx: u32,
    pub normal: Vec3,
    pub _pad0: u32,

    pub position_mat: MatrixIdx,
    pub normal_mat: MatrixIdx,
    pub _pad1: u32,
    pub _pad2: u32,

    pub chan0: Rgba,
    pub chan1: Rgba,

    pub tex_coord: [Vec2; 8],
    pub tex_coord_mat: [MatrixIdx; 8],
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

#[derive(Debug, Clone, Immutable, IntoBytes, Default)]
#[repr(C)]
pub struct Channel {
    pub material_from_vertex: u32,
    pub ambient_from_vertex: u32,
    pub lighting_enabled: u32,
    pub diffuse_attenuation: u32,
    pub attenuation: u32,
    pub specular: u32,
    pub light_mask: [u32; 8],
}

impl Channel {
    pub fn update(&mut self, channel: xform::Channel) {
        self.material_from_vertex = channel.material_from_vertex() as u32;
        self.ambient_from_vertex = channel.ambient_from_vertex() as u32;
        self.lighting_enabled = channel.lighting_enabled() as u32;
        self.diffuse_attenuation = channel.diffuse_attenuation() as u32;
        self.attenuation = channel.attenuation() as u32;
        self.specular = !channel.not_specular() as u32;

        let a = channel.lights0to3();
        let b = channel.lights4to7();
        self.light_mask = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]].map(|b| b as u32);
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
    pub projection_mat: Mat4,
    pub post_transform_mat: [Mat4; 8],
    pub constant_alpha: u32,
    pub alpha_refs: [u32; 2],
    pub _pad0: u32,
    pub fog: FogParams,
}
