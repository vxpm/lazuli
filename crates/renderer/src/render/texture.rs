use std::collections::hash_map::Entry;

use lazuli::modules::render::{Clut, ClutAddress, Texture, TextureId};
use lazuli::system::gx::color::Rgba8;
use lazuli::system::gx::tex::{ClutFormat, MipmapData};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureSettings {
    pub raw_id: TextureId,
    pub clut_addr: ClutAddress,
    pub clut_fmt: ClutFormat,
}

struct WithDeps<T> {
    value: T,
    deps: FxHashSet<TextureSettings>,
}

const TMEM_HIGH_LEN: usize = 512 * 1024 / 2;

type TmemHigh = Box<[u16; TMEM_HIGH_LEN]>;

pub struct Cache {
    tmem: TmemHigh,
    raws: FxHashMap<TextureId, WithDeps<Texture>>,
    textures: FxHashMap<TextureSettings, wgpu::TextureView>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            tmem: util::boxed_array(0),
            raws: Default::default(),
            textures: Default::default(),
        }
    }
}

impl Cache {
    fn create_texture_data_indirect(
        indirect: &Vec<u16>,
        palette: &[u16],
        format: ClutFormat,
    ) -> Vec<Rgba8> {
        let convert = match format {
            ClutFormat::IA8 => Rgba8::from_ia8,
            ClutFormat::RGB565 => Rgba8::from_rgb565,
            ClutFormat::RGB5A3 => Rgba8::from_rgb5a3,
            _ => panic!("reserved clut format"),
        };

        indirect
            .iter()
            .copied()
            .map(|index| {
                let color = palette[index as usize];
                convert(color)
            })
            .collect()
    }

    fn create_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raws: &mut FxHashMap<TextureId, WithDeps<Texture>>,
        tmem: &mut TmemHigh,
        settings: TextureSettings,
    ) -> wgpu::TextureView {
        let raw = raws.get_mut(&settings.raw_id).unwrap();
        raw.deps.insert(settings);

        let owned_data;
        let data: Vec<&[u8]> = match &raw.value.data {
            MipmapData::Direct(data) => data
                .iter()
                .map(|lod| zerocopy::transmute_ref!(lod.as_slice()))
                .collect::<Vec<_>>(),
            MipmapData::Indirect(data) => {
                let clut_base = settings.clut_addr.to_tmem_addr();
                let clut = &tmem[clut_base..];

                owned_data = data
                    .iter()
                    .map(|lod| Self::create_texture_data_indirect(lod, clut, settings.clut_fmt))
                    .collect::<Vec<_>>();

                owned_data
                    .iter()
                    .map(|lod| zerocopy::transmute_ref!(lod.as_slice()))
                    .collect::<Vec<_>>()
            }
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            dimension: wgpu::TextureDimension::D2,
            size: wgpu::Extent3d {
                width: raw.value.width,
                height: raw.value.height,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            mip_level_count: raw.value.data.lod_count(),
            sample_count: 1,
        });

        let mut current_width = raw.value.width;
        let mut current_height = raw.value.height;
        for (idx, lod) in data.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: idx as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::default(),
                },
                lod,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(current_width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: current_width,
                    height: current_height,
                    depth_or_array_layers: 1,
                },
            );

            current_width = (current_width / 2).max(1);
            current_height = (current_height / 2).max(1);
        }

        texture.create_view(&Default::default())
    }

    /// Returns whether this is texture ID was already present in the cache.
    pub fn update_raw(&mut self, id: TextureId, texture: Texture) -> bool {
        let old = self.raws.insert(
            id,
            WithDeps {
                value: texture,
                deps: Default::default(),
            },
        );

        if let Some(old) = old {
            for dep in old.deps {
                self.textures.remove(&dep);
            }

            true
        } else {
            false
        }
    }

    pub fn update_clut(&mut self, addr: ClutAddress, clut: Clut) {
        let mut current = addr.to_tmem_addr();

        // each clut is replicated sequentially 16 times
        for _ in 0..16 {
            self.tmem[current..][..clut.0.len()].copy_from_slice(&clut.0);
            current += clut.0.len();
        }
    }

    pub fn get(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        settings: TextureSettings,
    ) -> &wgpu::TextureView {
        match self.textures.entry(settings) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                let texture =
                    Self::create_texture(device, queue, &mut self.raws, &mut self.tmem, settings);

                v.insert(texture)
            }
        }
    }
}
