//! A `.cso` or '.ciso' file is a disc format designed
//! to save space without the CPU overhead of actual compression

use std::io::{Read, Seek, SeekFrom};
use binrw::{BinRead, BinWrite};
use crate::{apploader, dol, iso};

const CSO_HEADER_SIZE: usize = 0x8000; // 32KB
const CSO_MAP_SIZE: usize = CSO_HEADER_SIZE - size_of::<u32>() - 4; // 0x8000 (32768) - 4 (magic) - 4 (block_size)

/// The header of a CSO file.
#[derive(Debug, Clone, BinRead, BinWrite)]
#[br(big, magic = b"CISO")]
pub struct CsoHeader {
    #[br(little)]
    /// Size of the blocks
    pub block_size: u32,
    /// Used (1) or Unused (0)
    pub map: [u8; CSO_MAP_SIZE]
}

/// A Gamecube .cso file.
pub struct Cso<R> {
    /// Header of the file
    header: CsoHeader,
    /// LUT
    map: Vec<Option<u64>>,
    /// Reader of the contents
    reader: R
}

impl<R> Cso<R>
where
    R: Read + Seek
{
    /// Creates a new [`Cso`] from the given reader.
    pub fn new(mut reader: R) -> Result<Self, binrw::Error> {
        let header = CsoHeader::read(&mut reader)?;

        let mut map= Vec::with_capacity(CSO_MAP_SIZE);
        let mut current_offset = CSO_HEADER_SIZE as u64;

        for is_present in header.map {
            if is_present == 1
            {
                map.push(Some(current_offset));
                current_offset += header.block_size as u64;
            }
            else
            {
                map.push(None);
            }
        }

        Ok(Self { header, map, reader })
    }

    pub fn header(&self) -> &CsoHeader {
        &self.header
    }

    pub fn map(&self) -> &Vec<Option<u64>> {
        &self.map
    }

    pub fn reader(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Read from disk at the given offset and writes it into the output buffer.
    /// Returns how many bytes were actually read.
    pub fn read(&mut self, disk_offset: u64, out: &mut [u8]) -> std::io::Result<u64> {
        let block_size = self.header().block_size as u64;
        let mut current_disk_offset = disk_offset;
        let mut remaining = out.len() as u64;

        while remaining > 0 {

            let block = (current_disk_offset / block_size) as usize;
            let data_offset = current_disk_offset % block_size;
            let to_read = remaining.min(block_size - data_offset);

            let out_start = (current_disk_offset - disk_offset) as usize;
            let out = &mut out[out_start as usize..][..to_read as usize];

            match self.map[block] {
                Some(cso_block) => {
                    self.reader.seek(SeekFrom::Start(cso_block + data_offset))?;
                    self.reader.read_exact(out)?;
                }
                None => {
                    out.fill(0);
                }
            }

            current_disk_offset += to_read;
            remaining -= to_read;
        }

        Ok(out.len() as u64 - remaining)
    }
}

/// A wrapper around [`Cso`] providing an implementation of [`Read`] and [`Seek`].
pub struct CsoReader<R> {
    cso: Cso<R>,
    position: u64,
}

impl<R> CsoReader<R> {
    pub fn new(cso: Cso<R>) -> Self {
        Self { cso, position: 0 }
    }

    pub fn inner(&self) -> &Cso<R> {
        &self.cso
    }

    pub fn inner_mut(&mut self) -> &mut Cso<R> {
        &mut self.cso
    }

    pub fn into_inner(self) -> Cso<R> {
        self.cso
    }
}

impl<R> Read for CsoReader<R>
where
    R: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = match self.cso.read(self.position, buf) {
            Ok(read) => read,
            Err(e) => {
                return Err(std::io::Error::other(
                    format!("cso disk module failed: {e}"
                )));
            },
        };

        self.position += read;
        Ok(read as usize)
    }
}

impl<R> Seek for CsoReader<R>
where
    R: Read + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Start(x) => self.position = x,
            SeekFrom::End(x) => {
                self.position = self
                    .cso
                    .header()
                    .block_size
                    .saturating_sub_signed(x as i32) as u64;
            },
            SeekFrom::Current(x) => self.position = self.position.saturating_add_signed(x),
        }

        Ok(self.position)
    }
}

impl<R> CsoReader<R>
where
    R: Read + Seek,
{
    pub fn iso_header(&mut self) -> Result<iso::Header, binrw::Error> {
        self.seek(SeekFrom::Start(0))?;
        iso::Header::read_be(self)
    }

    pub fn apploader(&mut self) -> Result<apploader::Apploader, binrw::Error> {
        self.seek(SeekFrom::Start(0x2440))?;
        apploader::Apploader::read(self)
    }

    pub fn apploader_header(&mut self) -> Result<apploader::Header, binrw::Error> {
        self.seek(SeekFrom::Start(0x2440))?;
        apploader::Header::read(self)
    }

    pub fn bootfile(&mut self) -> Result<dol::Dol, binrw::Error> {
        let header = self.iso_header()?;
        self.seek(SeekFrom::Start(header.bootfile_offset as u64))?;
        dol::Dol::read(self)
    }

    pub fn bootfile_header(&mut self) -> Result<dol::Header, binrw::Error> {
        let header = self.iso_header()?;
        self.seek(SeekFrom::Start(header.bootfile_offset as u64))?;
        dol::Header::read(self)
    }

    pub fn filesystem(&mut self) -> Result<iso::filesystem::FileSystem, binrw::Error> {
        let header = self.iso_header()?;
        self.seek(SeekFrom::Start(header.filesystem_offset as u64))?;
        iso::filesystem::FileSystem::read(self)
    }
}
