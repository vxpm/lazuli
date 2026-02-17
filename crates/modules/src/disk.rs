use std::io::{Read, Seek, SeekFrom};

use lazuli::disks::cso::{Cso, CsoReader};
use lazuli::disks::rvz::{Rvz, RvzReader};
use lazuli::modules::disk::DiskModule;

/// An implementation of [`DiskModule`] for raw .iso data from a reader.
#[derive(Debug)]
pub struct IsoModule<R>(pub Option<R>);

impl<R> Read for IsoModule<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(r) = &mut self.0 {
            r.read(buf)
        } else {
            Err(std::io::Error::other("no disk inserted"))
        }
    }
}

impl<R> Seek for IsoModule<R>
where
    R: Seek,
{
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        if let Some(r) = &mut self.0 {
            r.seek(from)
        } else {
            Err(std::io::Error::other("no disk inserted"))
        }
    }
}

impl<R> DiskModule for IsoModule<R>
where
    R: Read + Seek + Send,
{
    fn has_disk(&self) -> bool {
        self.0.is_some()
    }
}

/// An implementation of [`DiskModule`] for .rvz disks.
pub struct RvzModule<R>(RvzReader<R>);

impl<R> RvzModule<R> {
    pub fn new(rvz: Rvz<R>) -> Self {
        Self(RvzReader::new(rvz))
    }
}

impl<R> Read for RvzModule<R>
where
    R: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl<R> Seek for RvzModule<R>
where
    R: Read + Seek,
{
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(from)
    }
}

impl<R> DiskModule for RvzModule<R>
where
    R: Read + Seek + Send,
{
    fn has_disk(&self) -> bool {
        true
    }
}

/// An implementation of [`DiskModule`] for .cso/.ciso disks.
pub struct CsoModule<R>(CsoReader<R>);

impl<R> CsoModule<R> {
    pub fn new(cso: Cso<R>) -> Self {
        Self(CsoReader::new(cso))
    }
}

impl<R> Read for CsoModule<R>
where
    R: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl<R> Seek for CsoModule<R>
where
    R: Read + Seek,
{
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(from)
    }
}

impl<R> DiskModule for CsoModule<R>
where
    R: Read + Seek + Send,
{
    fn has_disk(&self) -> bool {
        true
    }
}
