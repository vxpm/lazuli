//! Framebuffers (EFB and XFB).

use std::collections::hash_map::Entry;

use lazuli::modules::render::oneshot::{self, Sender};
use lazuli::modules::render::{CopyArgs, Texels, TextureId, XfbPart};
use lazuli::system::gx::pix::{ColorCopyFormat, DepthCopyFormat};
use lazuli::system::gx::{EFB_HEIGHT, EFB_WIDTH, pix};
use lazuli::system::vi::Dimensions;
use rustc_hash::FxHashMap;
use zerocopy::FromBytes;

use crate::render::Renderer;

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

impl Renderer {
    pub fn set_xfb_dimensions(&mut self, dims: Dimensions) {
        if dims == self.external_fb.dimensions() {
            return;
        }

        self.external_fb.resize(
            &self.device,
            wgpu::Extent3d {
                width: dims.width as u32,
                height: dims.height as u32,
                depth_or_array_layers: 1,
            },
        );

        let mut output = self.shared.output.lock().unwrap();
        *output = self.external_fb.framebuffer().clone();
    }

    pub fn set_efb_format(&mut self, format: pix::BufferFormat) {
        self.flush(format_args!("framebuffer format changed to {format:?}"));

        match format {
            pix::BufferFormat::RGB8Z24 | pix::BufferFormat::RGB565Z16 => {
                self.pipeline_config.has_alpha = false
            }
            pix::BufferFormat::RGBA6Z24 => self.pipeline_config.has_alpha = true,
            _ => (),
        }
    }

    fn copy_color_to_tex(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        half: bool,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::TextureView {
        let divisor = if half { 2 } else { 1 };
        let size = wgpu::Extent3d {
            width: width as u32 / divisor,
            height: height as u32 / divisor,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            dimension: wgpu::TextureDimension::D2,
            size,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 1,
        });
        let view = texture.create_view(&Default::default());

        let color = self.embedded_fb.color();
        self.color_blitter.blit_to_texture(
            &self.device,
            color,
            wgpu::Origin3d {
                x: x as u32,
                y: y as u32,
                z: 0,
            },
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
            &view,
            encoder,
        );

        view
    }

    fn copy_depth_to_tex(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        half: bool,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::TextureView {
        let divisor = if half { 2 } else { 1 };
        let size = wgpu::Extent3d {
            width: width as u32 / divisor,
            height: height as u32 / divisor,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            dimension: wgpu::TextureDimension::D2,
            size,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 1,
        });
        let view = texture.create_view(&Default::default());

        let depth = self.embedded_fb.depth();
        self.depth_blitter.blit_to_texture(
            &self.device,
            depth,
            wgpu::Origin3d {
                x: x as u32,
                y: y as u32,
                z: 0,
            },
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
            &view,
            encoder,
        );

        view
    }

    fn encode_color(
        &mut self,
        color: &wgpu::TextureView,
        format: ColorCopyFormat,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::TextureView {
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            dimension: wgpu::TextureDimension::D2,
            size: color.texture().size(),
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 1,
        });

        let view = output.create_view(&Default::default());
        self.converter
            .convert_color(&self.device, format, color, &view, encoder);

        view
    }

    fn encode_depth(
        &mut self,
        depth: &wgpu::TextureView,
        format: DepthCopyFormat,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::TextureView {
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            dimension: wgpu::TextureDimension::D2,
            size: depth.texture().size(),
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
            mip_level_count: 1,
            sample_count: 1,
        });

        let view = output.create_view(&Default::default());
        self.converter
            .convert_depth(&self.device, format, depth, &view, encoder);

        view
    }

    fn get_texture_data(
        &mut self,
        view: &wgpu::TextureView,
        mut encoder: wgpu::CommandEncoder,
    ) -> Vec<u32> {
        let size = view.texture().size();
        let row_size = size.width * 4;
        let row_stride = row_size.next_multiple_of(256);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: view.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.data_read_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_stride),
                    rows_per_image: None,
                },
            },
            size,
        );

        let (sender, receiver) = oneshot::channel();
        encoder.map_buffer_on_submit(&self.data_read_buffer, wgpu::MapMode::Read, .., |r| {
            sender.send(r).unwrap()
        });

        let cmd = encoder.finish();
        let submission = self.queue.submit([cmd]);
        self.device
            .poll(wgpu::wgt::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();

        let result = receiver.recv().unwrap();
        result.unwrap();

        let mapped = self.data_read_buffer.get_mapped_range(..);
        let data = &*mapped;

        let mut texels = Vec::with_capacity(size.width as usize * size.height as usize);
        for row in 0..size.height as usize {
            let row_data = &data[row * row_stride as usize..][..row_size as usize];
            texels.extend(
                row_data
                    .chunks_exact(4)
                    .map(u32::read_from_bytes)
                    .map(Result::unwrap),
            );
        }

        std::mem::drop(mapped);
        self.data_read_buffer.unmap();

        texels
    }

    fn clear(&mut self, x: u32, y: u32, width: u32, height: u32) {
        let color = self
            .pipeline_config
            .blend
            .color_write
            .then_some(self.clear_color);
        let depth = self.pipeline_config.depth.write.then_some(self.clear_depth);

        self.current_pass.set_scissor_rect(x, y, width, height);
        self.current_pass
            .set_viewport(0.0, 0.0, 640.0, 528.0, 0.0, 1.0);
        self.cleaner
            .clear_target(color, depth, &mut self.current_pass);
    }

    pub fn copy_color(
        &mut self,
        args: CopyArgs,
        format: ColorCopyFormat,
        response: Option<Sender<Texels>>,
        id: TextureId,
    ) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        self.debug(format!(
            "color copy requested: ({}, {}) [{}x{}] (mip: {})",
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half
        ));

        self.submit();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let raw_texture = self.copy_color_to_tex(
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half,
            &mut encoder,
        );

        let encoded_texture = self.encode_color(&raw_texture, format, &mut encoder);
        if let Some(response) = response {
            let data = self.get_texture_data(&raw_texture, encoder);
            response.send(data).unwrap();
        } else {
            let cmd = encoder.finish();
            self.queue.submit([cmd]);
        }

        self.texture_cache.insert_direct(id, encoded_texture);
        if clear {
            self.clear(
                src.x().value() as u32,
                src.y().value() as u32,
                dims.width() as u32,
                dims.height() as u32,
            );
        }
    }

    pub fn copy_depth(
        &mut self,
        args: CopyArgs,
        format: DepthCopyFormat,
        response: Option<Sender<Texels>>,
        id: TextureId,
    ) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        self.debug(format!(
            "depth copy requested: ({}, {}) [{}x{}] (mip: {})",
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half
        ));

        self.submit();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let raw_texture = self.copy_depth_to_tex(
            src.x().value(),
            src.y().value(),
            dims.width(),
            dims.height(),
            half,
            &mut encoder,
        );

        let encoded_texture = self.encode_depth(&raw_texture, format, &mut encoder);
        if let Some(response) = response {
            let data = self.get_texture_data(&raw_texture, encoder);
            response.send(data).unwrap();
        } else {
            let cmd = encoder.finish();
            self.queue.submit([cmd]);
        }

        self.texture_cache.insert_direct(id, encoded_texture);
        if clear {
            self.clear(
                src.x().value() as u32,
                src.y().value() as u32,
                dims.width() as u32,
                dims.height() as u32,
            );
        }
    }

    pub fn copy_xfb(&mut self, args: CopyArgs, id: u32) {
        let CopyArgs {
            src,
            dims,
            half,
            clear,
        } = args;

        assert!(!half);

        self.debug("XFB copy requested");
        self.submit();

        let x = src.x().value() as u32;
        let y = src.y().value() as u32;
        let width = dims.width() as u32;
        let height = dims.height() as u32;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let color = self.embedded_fb.color();
        let target = self.external_fb.create_copy(&self.device, id, size);

        self.current_transfer_encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: color.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::default(),
            },
            size,
        );

        if clear {
            self.clear(x, y, width, height);
        }
    }

    pub fn present_xfb(&mut self, parts: Vec<XfbPart>) {
        self.external_fb
            .build(&mut self.current_transfer_encoder, parts);

        self.submit();
    }
}
