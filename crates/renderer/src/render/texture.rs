use std::collections::hash_map::Entry;

use lazuli::modules::render::{ClutData, ClutId, ClutRef, Texture, TextureId};
use lazuli::system::gx::color::Rgba8;
use lazuli::system::gx::tex::{ClutFormat, TextureData};
use rustc_hash::FxHashMap;

/// Configuration of a processed texture.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureRef {
    pub id: TextureId,
    pub clut: ClutRef,
}

/// Processed textures derived from a parent raw texture.
enum Processed {
    Direct(Option<wgpu::TextureView>),
    Indirect(FxHashMap<ClutRef, wgpu::TextureView>),
}

/// A texture family.
struct Family {
    raw: Option<Texture>,
    processed: Processed,
}

const TMEM_HIGH_LEN: usize = 512 * 1024 / 2;

type TmemHigh = Box<[u16; TMEM_HIGH_LEN]>;

pub struct Cache {
    tmem: TmemHigh,
    families: FxHashMap<TextureId, Family>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            tmem: util::boxed_array(0),
            families: Default::default(),
        }
    }
}

impl Cache {
    fn create_texture_data_indirect(
        indirect: &[u16],
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
        tmem: &mut TmemHigh,
        raw: &Texture,
        clut: ClutRef,
    ) -> wgpu::TextureView {
        let owned_data;
        let data: Vec<&[u8]> = match &raw.data {
            TextureData::Direct(data) => data
                .iter()
                .map(|lod| zerocopy::transmute_ref!(lod.as_slice()))
                .collect::<Vec<_>>(),
            TextureData::Indirect(data) => {
                let clut_base = clut.id.to_tmem_addr();
                let clut_data = &tmem[clut_base..];

                owned_data = data
                    .iter()
                    .map(|lod| Self::create_texture_data_indirect(lod, clut_data, clut.fmt))
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
                width: raw.width,
                height: raw.height,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            mip_level_count: raw.data.lod_count(),
            sample_count: 1,
        });

        let mut current_width = raw.width;
        let mut current_height = raw.height;
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
    pub fn update_raw(&mut self, id: TextureId, raw: Texture) -> bool {
        let processed = match raw.data {
            TextureData::Direct(_) => Processed::Direct(None),
            TextureData::Indirect(_) => Processed::Indirect(Default::default()),
        };

        let old = self.families.insert(
            id,
            Family {
                raw: Some(raw),
                processed,
            },
        );

        old.is_some()
    }

    pub fn update_clut(&mut self, addr: ClutId, clut: ClutData) {
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
        config: TextureRef,
    ) -> &wgpu::TextureView {
        let family = self.families.get_mut(&config.id).unwrap();
        match &mut family.processed {
            Processed::Direct(processed) => processed.get_or_insert_with(|| {
                Self::create_texture(
                    device,
                    queue,
                    &mut self.tmem,
                    family.raw.as_ref().unwrap(),
                    config.clut,
                )
            }),
            Processed::Indirect(processed) => match processed.entry(config.clut) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => {
                    let texture = Self::create_texture(
                        device,
                        queue,
                        &mut self.tmem,
                        family.raw.as_ref().unwrap(),
                        config.clut,
                    );

                    v.insert(texture)
                }
            },
        }
    }

    pub fn insert_direct(&mut self, id: TextureId, tex: wgpu::TextureView) {
        self.families.insert(
            id,
            Family {
                raw: None,
                processed: Processed::Direct(Some(tex)),
            },
        );
    }
}
