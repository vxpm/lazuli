//! A collection of parsers for GameCube/Wii file formats.

pub mod apploader;
pub mod dol;
pub mod iso;
pub mod cso;
pub mod rvz;

pub use binrw;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    GameCube,
    Wii,
}
