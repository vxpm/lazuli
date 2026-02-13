//! Framebuffers (EFB and XFB).

use std::collections::hash_map::Entry;

use lazuli::modules::render::XfbPart;
use lazuli::system::gx::{EFB_HEIGHT, EFB_WIDTH};
use lazuli::system::vi::Dimensions;
use rustc_hash::FxHashMap;

pub struct Embedded {
    /// Color component of the EFB.
    color: wgpu::TextureView,
    /// Multisampled color component of the EFB.
    multisampled_color: wgpu::TextureView,
    /// Depth component of the EFB.
    depth: wgpu::TextureView,
}

impl Embedded {
    pub fn new(device: &wgpu::Device) -> Self {
        let size = wgpu::Extent3d {
            width: EFB_WIDTH as u32,
            height: EFB_HEIGHT as u32,
            depth_or_array_layers: 1,
        };

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("efb color resolved"),
            dimension: wgpu::TextureDimension::D2,
            size,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 1,
        });

        let multisampled_color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("efb color multisampled"),
            dimension: wgpu::TextureDimension::D2,
            size,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 4,
        });

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("efb depth"),
            dimension: wgpu::TextureDimension::D2,
            size,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 4,
        });

        let color = color.create_view(&Default::default());
        let multisampled_color = multisampled_color.create_view(&Default::default());
        let depth = depth.create_view(&Default::default());

        Self {
            color,
            multisampled_color,
            depth,
        }
    }

    pub fn color(&self) -> &wgpu::TextureView {
        &self.color
    }

    pub fn multisampled_color(&self) -> &wgpu::TextureView {
        &self.multisampled_color
    }

    pub fn depth(&self) -> &wgpu::TextureView {
        &self.depth
    }
}

pub struct External {
    framebuffer: wgpu::TextureView,
    texture_pool: FxHashMap<wgpu::Extent3d, wgpu::TextureView>,
    copies: FxHashMap<u32, wgpu::TextureView>,
}

impl External {
    fn create_framebuffer(device: &wgpu::Device, size: wgpu::Extent3d) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("external framebuffer"),
                dimension: wgpu::TextureDimension::D2,
                size,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
                mip_level_count: 1,
                sample_count: 1,
            })
            .create_view(&Default::default())
    }

    pub fn new(device: &wgpu::Device) -> Self {
        let framebuffer = Self::create_framebuffer(
            device,
            wgpu::Extent3d {
                width: 640,
                height: 480,
                depth_or_array_layers: 1,
            },
        );

        Self {
            framebuffer,
            texture_pool: FxHashMap::default(),
            copies: Default::default(),
        }
    }

    pub fn framebuffer(&self) -> &wgpu::TextureView {
        &self.framebuffer
    }

    pub fn dimensions(&self) -> Dimensions {
        Dimensions {
            width: self.framebuffer.texture().width() as u16,
            height: self.framebuffer.texture().height() as u16,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        self.framebuffer = Self::create_framebuffer(device, size);
    }

    pub fn create_copy(
        &mut self,
        device: &wgpu::Device,
        id: u32,
        size: wgpu::Extent3d,
    ) -> wgpu::TextureView {
        let tex = match self.texture_pool.entry(size) {
            Entry::Occupied(o) => o.remove(),
            Entry::Vacant(_) => {
                let tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("xfb copy texture"),
                    dimension: wgpu::TextureDimension::D2,
                    size,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                    mip_level_count: 1,
                    sample_count: 1,
                });

                tex.create_view(&wgpu::TextureViewDescriptor::default())
            }
        };

        self.copies.insert(id, tex.clone());
        tex
    }

    /// Builds the XFB texture from a list of parts describing where to put each copy. Copies must
    /// have been previously added with `insert_copy` and are consumed by this method.
    pub fn build(&mut self, encoder: &mut wgpu::CommandEncoder, parts: Vec<XfbPart>) {
        let framebuffer = self.framebuffer.texture();
        encoder.clear_texture(
            framebuffer,
            &wgpu::ImageSubresourceRange {
                aspect: wgpu::TextureAspect::default(),
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            },
        );

        for part in parts {
            let saved = self.copies.get(&part.id).unwrap();
            let saved_size = saved.texture().size();
            let framebuffer_size = framebuffer.size();

            // HACK: this isnt the right way to deal with this... Animal Crossing needs it,
            // investigate further (XFB dimensions seem incorrect?)
            let width = saved_size.width.min(framebuffer_size.width - part.offset_x);
            let height = saved_size
                .height
                .min(framebuffer_size.height - part.offset_y);

            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: saved.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::TexelCopyTextureInfo {
                    texture: framebuffer,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: part.offset_x,
                        y: part.offset_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.texture_pool
            .extend(self.copies.drain().map(|(_, tex)| {
                let size = tex.texture().size();
                (size, tex)
            }));
    }
}
