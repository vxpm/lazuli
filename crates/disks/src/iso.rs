//! A GameCube/Wii `.iso` file contains the entire image of a disk.

pub mod filesystem;

use std::io::{Read, Seek, SeekFrom};

use binrw::{BinRead, BinWrite, NullString};
use filesystem::FileSystem;

use crate::{Console, apploader, dol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead, BinWrite)]
#[brw(big, magic = 0xC233_9F3D_u32)]
pub struct MagicWord;

#[derive(Debug, Clone, PartialEq, Eq, BinRead, BinWrite)]
#[brw(big)]
pub struct Meta {
    pub console_id: u8,
    pub game_id: u16,
    pub country_code: u8,
    pub maker_code: u16,
    pub disk_id: u8,
    pub version: u8,
    pub audio_streaming: u8,
    pub stream_buffer_size: u8,
    #[brw(pad_before = 0x12)]
    pub magic: MagicWord,
    #[brw(assert(game_name.len() <= 0x3E0))]
    pub game_name: NullString,
}

impl Meta {
    pub fn game_code(&self) -> u32 {
        let game = self.game_id.to_be_bytes();
        u32::from_be_bytes([self.console_id, game[0], game[1], self.country_code])
    }

    pub fn game_code_str(&self) -> Option<String> {
        String::from_utf8(self.game_code().to_be_bytes().into()).ok()
    }

    pub fn console(&self) -> Option<Console> {
        Some(match self.console_id {
            b'G' => Console::GameCube,
            b'R' => Console::Wii,
            _ => return None,
        })
    }

    pub fn region(&self) -> Option<Region> {
        Some(match self.country_code {
            b'J' => Region::Japan,
            b'P' => Region::Pal,
            b'E' => Region::Usa,
            _ => return None,
        })
    }

    pub fn audio_streaming(&self) -> Option<bool> {
        Some(match self.audio_streaming {
            0 => false,
            1 => true,
            _ => return None,
        })
    }
}

/// The header of a GameCube .iso file.
#[derive(Debug, Clone, BinRead, BinWrite)]
#[brw(big)]
pub struct Header {
    #[brw(pad_size_to = 0x400)]
    pub meta: Meta,
    pub debug_monitor_offset: u32,
    pub debug_monitor_target: u32,
    #[brw(pad_before = 0x18)]
    pub bootfile_offset: u32,
    pub filesystem_offset: u32,
    pub filesystem_size: u32,
    pub max_filesystem_size: u32,
    pub user_position: u32,
    pub user_length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Japan,
    Pal,
    Usa,
}

/// A GameCube .iso file.
#[derive(Debug)]
pub struct Iso<R> {
    /// Header of the ISO.
    header: Header,
    /// Reader of the contents.
    reader: R,
}

impl<R> Iso<R>
where
    R: Read + Seek,
{
    pub fn new(mut reader: R) -> Result<Self, binrw::Error> {
        let header = Header::read(&mut reader)?;
        Ok(Self { header, reader })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn reader(&mut self) -> &mut R {
        &mut self.reader
    }

    pub fn bootfile(&mut self) -> Result<dol::Dol, binrw::Error> {
        self.reader
            .seek(SeekFrom::Start(self.header.bootfile_offset as u64))?;
        dol::Dol::read(&mut self.reader)
    }

    pub fn bootfile_header(&mut self) -> Result<dol::Header, binrw::Error> {
        self.reader
            .seek(SeekFrom::Start(self.header.bootfile_offset as u64))?;
        dol::Header::read(&mut self.reader)
    }

    pub fn apploader(&mut self) -> Result<apploader::Apploader, binrw::Error> {
        self.reader.seek(SeekFrom::Start(0x2440))?;
        apploader::Apploader::read(&mut self.reader)
    }

    pub fn apploader_header(&mut self) -> Result<apploader::Header, binrw::Error> {
        self.reader.seek(SeekFrom::Start(0x2440))?;
        apploader::Header::read(&mut self.reader)
    }

    pub fn filesystem(&mut self) -> Result<FileSystem, binrw::Error> {
        self.reader
            .seek(SeekFrom::Start(self.header.filesystem_offset as u64))?;

        FileSystem::read(&mut self.reader)
    }
}
