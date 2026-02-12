//! Framebuffers (EFB and XFB).

use lazuli::modules::render::XfbPart;
use lazuli::system::gx::{EFB_HEIGHT, EFB_WIDTH};
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
    saved_copies: FxHashMap<u32, wgpu::TextureView>,
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
            saved_copies: Default::default(),
        }
    }

    pub fn framebuffer(&self) -> &wgpu::TextureView {
        &self.framebuffer
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        self.framebuffer = Self::create_framebuffer(device, size);
    }

    /// Saves the given texture as the source for the copy with the given ID.
    pub fn insert_copy(&mut self, id: u32, tex: wgpu::TextureView) {
        self.saved_copies.insert(id, tex);
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
            let saved = self.saved_copies.get(&part.id).unwrap();
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
                saved.texture().size(),
            );
        }

        self.saved_copies.clear();
    }
}
