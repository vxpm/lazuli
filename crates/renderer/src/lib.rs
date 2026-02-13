#![feature(iter_array_chunks)]

mod alloc;
mod blit;
mod clear;
mod render;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use flume::{Receiver, Sender};
use lazuli::modules::render::{Action, RenderModule};

use crate::blit::XfbBlitter;
use crate::render::Renderer as RendererInner;

#[expect(clippy::needless_pass_by_value, reason = "makes it clearer")]
fn worker(mut renderer: RendererInner, receiver: Receiver<Action>) {
    while let Ok(action) = receiver.recv() {
        renderer.exec(action);
    }
}

pub struct Stats {
    pub counters: wgpu::InternalCounters,
    pub alloc: Option<wgpu::AllocatorReport>,
}

struct Inner {
    device: wgpu::Device,
    shared: Arc<render::Shared>,
    blitter: XfbBlitter,
}

/// A WGPU based renderer implementation.
///
/// This type is reference counted and therefore cheaply clonable.
#[derive(Clone)]
pub struct Renderer {
    inner: Arc<Inner>,
    sender: Sender<Action>,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let blitter = XfbBlitter::new(&device, format);
        let (renderer, shared) = RendererInner::new(device.clone(), queue);

        const CAPACITY: usize = 1024 * 1024 / size_of::<Action>();
        let (sender, receiver) = flume::bounded(CAPACITY);

        std::thread::Builder::new()
            .name("lazuli wgpu renderer".into())
            .spawn(move || worker(renderer, receiver))
            .unwrap();

        Self {
            inner: Arc::new(Inner {
                device,
                shared,
                blitter,
            }),
            sender,
        }
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let output = self.inner.shared.output.lock().unwrap();
        let size = output.texture().size();

        // TODO: change this
        self.inner.blitter.blit_to_target(
            &self.inner.device,
            &output,
            wgpu::Origin3d::ZERO,
            size,
            pass,
        );
    }

    pub fn rendered_anything(&self) -> bool {
        self.inner
            .shared
            .rendered_anything
            .swap(false, Ordering::Relaxed)
    }

    pub fn stats(&self) -> Box<Stats> {
        let counters = self.inner.device.get_internal_counters();
        let alloc = self.inner.device.generate_allocator_report();
        Box::new(Stats { counters, alloc })
    }
}

impl RenderModule for Renderer {
    fn exec(&mut self, action: Action) {
        self.sender.send(action).expect("rendering thread is alive");
    }
}
