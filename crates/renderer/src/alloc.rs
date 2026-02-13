//! Buffer allocator.

use flume::{Receiver, Sender};

#[derive(Clone)]
struct BufferPair {
    primary: wgpu::Buffer,
    secondary: Option<wgpu::Buffer>,
}

impl BufferPair {
    fn map(self, f: impl FnOnce(BufferPair) + Send + 'static) {
        let buffer = match &self.secondary {
            Some(secondary) => secondary.clone(),
            None => self.primary.clone(),
        };

        buffer.map_async(wgpu::MapMode::Write, .., move |_| f(self));
    }

    fn write_and_unmap(&self, encoder: &mut wgpu::CommandEncoder, data: &[u8]) {
        let size = data.len() as u64;
        match &self.secondary {
            Some(secondary) => {
                {
                    let mut mapped = secondary.get_mapped_range_mut(..size);
                    mapped.copy_from_slice(data);
                }

                secondary.unmap();
                encoder.copy_buffer_to_buffer(secondary, 0, &self.primary, 0, Some(size));
            }
            None => {
                {
                    let mut mapped = self.primary.get_mapped_range_mut(..size);
                    mapped.copy_from_slice(data);
                }

                self.primary.unmap();
            }
        }
    }
}

pub struct Allocator {
    mappable_primary: bool,
    usages: wgpu::BufferUsages,
    available: Vec<Vec<BufferPair>>,
    allocated: Vec<BufferPair>,
    sender: Sender<BufferPair>,
    receiver: Receiver<BufferPair>,
}

fn bucket_for(size: u64) -> usize {
    size.ilog2() as usize - 4
}

impl Allocator {
    pub fn new(device: &wgpu::Device, usages: wgpu::BufferUsages) -> Self {
        let (sender, receiver) = flume::unbounded();
        Self {
            mappable_primary: device
                .features()
                .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS),
            usages,
            available: Default::default(),
            allocated: Default::default(),
            sender,
            receiver,
        }
    }

    fn recall(&mut self) {
        if self.receiver.is_empty() {
            return;
        }

        while let Ok(pair) = self.receiver.try_recv() {
            let size = pair.primary.size();
            let bucket = bucket_for(size);

            if self.available.len() <= bucket {
                self.available.resize(bucket + 1, Vec::new());
            }

            self.available[bucket].push(pair);
        }
    }

    fn allocate_inner(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        data: &[u8],
        recall: bool,
    ) -> wgpu::Buffer {
        let size = data.len() as u64;
        let buffer_size = size.next_power_of_two().max(16);
        let bucket = bucket_for(buffer_size);

        let pair = self
            .available
            .get_mut(bucket)
            .and_then(|bucket| bucket.pop());

        let pair = match pair {
            Some(pair) => pair,
            None => {
                if recall {
                    self.recall();
                    return self.allocate_inner(device, encoder, data, false);
                }

                let usage = self.usages
                    | if self.mappable_primary {
                        wgpu::BufferUsages::MAP_WRITE
                    } else {
                        wgpu::BufferUsages::COPY_DST
                    };

                let primary = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: buffer_size,
                    usage,
                    mapped_at_creation: self.mappable_primary,
                });

                let secondary = (!self.mappable_primary).then(|| {
                    device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: buffer_size,
                        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: true,
                    })
                });

                BufferPair { primary, secondary }
            }
        };

        pair.write_and_unmap(encoder, data);

        let buffer = pair.primary.clone();
        self.allocated.push(pair);

        buffer
    }

    pub fn allocate(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        data: &[u8],
    ) -> wgpu::Buffer {
        self.allocate_inner(device, encoder, data, true)
    }

    pub fn free(&mut self) {
        for pair in self.allocated.drain(..) {
            let sender = self.sender.clone();
            pair.map(move |pair| sender.send(pair).unwrap());
        }
    }
}
