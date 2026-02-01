//! Disk interface (DI).
use std::io::SeekFrom;

use bitos::{BitUtils, bitos};
use gekko::Address;
use strum::FromRepr;

use crate::system::{System, pi};

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Status {
    #[bits(0)]
    pub break_request: bool,
    #[bits(1)]
    pub device_err_interrupt_mask: bool,
    #[bits(2)]
    pub device_err_interrupt: bool,
    #[bits(3)]
    pub transfer_interrupt_mask: bool,
    #[bits(4)]
    pub transfer_interrupt: bool,
    #[bits(5)]
    pub break_interrupt_mask: bool,
    #[bits(6)]
    pub break_interrupt: bool,
}

impl Status {
    pub fn any_interrupt(&self) -> bool {
        let device_err = self.device_err_interrupt() && self.device_err_interrupt();
        let transfer = self.transfer_interrupt() && self.transfer_interrupt_mask();
        let break_ = self.break_interrupt() && self.break_interrupt_mask();
        device_err || transfer || break_
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Read  = 0,
    Write = 1,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Control {
    #[bits(0)]
    pub transfer_ongoing: bool,
    #[bits(1)]
    pub dma: bool,
    #[bits(2)]
    pub mode: TransferMode,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Cover {
    #[bits(0)]
    pub open: bool,
    #[bits(1)]
    pub interrupt_mask: bool,
    #[bits(2)]
    pub interrupt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub enum Opcode {
    Identify    = 0x12,
    Read        = 0xA8,
    Seek        = 0xAB,
    Status      = 0xE0,
    AudioStream = 0xE1,
    AudioStatus = 0xE2,
    StopMotor   = 0xE3,
    AudioConfig = 0xE4,
    Debug       = 0xFE,
    DebugEnable = 0xFF,
}

impl Opcode {
    pub fn new(value: u8) -> Self {
        Opcode::from_repr(value).expect("unknown disk command opcode")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Identify,
    Read { offset: u32, length: u32 },
    Seek { offset: u32 },
    Status,
    StartAudioStream { offset: u32, length: u32 },
    StopAudioStream,
    AudioStreamStatus,
    StopMotor,
    DisableAudioStream,
    EnableAudioStream,
    Debug,
    DebugEnable,
}

#[derive(Default)]
pub struct Interface {
    pub status: Status,
    pub control: Control,
    pub command_buffer: [u32; 3],
    pub dma_base: Address,
    pub dma_length: u32,
    pub cover: Cover,
    pub config: u32,
    pub immediate: u32,
}

impl Interface {
    pub fn write_status(&mut self, value: Status) {
        self.status
            .set_device_err_interrupt_mask(value.device_err_interrupt_mask());
        self.status.set_device_err_interrupt(
            self.status.device_err_interrupt() & !value.device_err_interrupt(),
        );

        self.status
            .set_transfer_interrupt_mask(value.transfer_interrupt_mask());
        self.status
            .set_transfer_interrupt(self.status.transfer_interrupt() & !value.transfer_interrupt());

        self.status
            .set_break_interrupt_mask(value.break_interrupt_mask());
        self.status
            .set_break_interrupt(self.status.break_interrupt() & !value.break_interrupt());
    }

    pub fn write_cover(&mut self, value: Cover) {
        self.cover.set_interrupt_mask(value.interrupt_mask());
        self.cover
            .set_interrupt(self.cover.interrupt() & !value.interrupt());
    }

    pub fn command(&self) -> Command {
        let buf = self.command_buffer[0].to_be_bytes();
        let opcode = Opcode::new(buf[0]);

        match opcode {
            Opcode::Identify => Command::Identify,
            Opcode::Read => {
                if buf[3] == 0x40 {
                    assert_eq!(self.command_buffer[1], 0);
                    assert_eq!(self.command_buffer[2], 0x20);
                    assert_eq!(self.dma_length, 0x20);
                }

                assert!(self.dma_length.is_multiple_of(32));

                Command::Read {
                    offset: self.command_buffer[1] << 2,
                    length: self.command_buffer[2],
                }
            }
            Opcode::Seek => Command::Seek {
                offset: self.command_buffer[1] << 2,
            },
            Opcode::Status => Command::Status,
            Opcode::AudioStream => match buf[1] {
                0x00 => Command::StartAudioStream {
                    offset: self.command_buffer[1] << 2,
                    length: self.command_buffer[2],
                },
                0x01 => Command::StopAudioStream,
                _ => panic!("unknown audio stream command: {:02X}", buf[1]),
            },
            Opcode::AudioStatus => match buf[1] {
                0x00 => Command::AudioStreamStatus,
                _ => panic!("unknown audio stream status command: {:02X}", buf[1]),
            },
            Opcode::StopMotor => Command::StopMotor,
            Opcode::AudioConfig => match (buf[1], buf[3]) {
                (0x00, 0x00) => Command::DisableAudioStream,
                (0x01, 0x0A) => Command::EnableAudioStream,
                _ => panic!(
                    "unknown audio config command: {:02X}00{:02X}",
                    buf[1], buf[3]
                ),
            },
            Opcode::Debug => Command::Debug,
            Opcode::DebugEnable => Command::DebugEnable,
        }
    }
}

pub fn complete_transfer(sys: &mut System) {
    tracing::debug!("completed DI transfer");
    sys.disk.status.set_transfer_interrupt(true);
    sys.disk.control.set_transfer_ongoing(false);
    sys.disk.dma_length = 0;
    pi::check_interrupts(sys);
}

pub fn complete_seek(sys: &mut System) {
    tracing::debug!("completed DI seek");
    sys.disk.status.set_transfer_interrupt(true);
    sys.disk.control.set_transfer_ongoing(false);
    pi::check_interrupts(sys);
}

pub fn write_control(sys: &mut System, value: Control) {
    sys.disk.control.set_dma(value.dma());
    sys.disk.control.set_mode(value.mode());

    if value.transfer_ongoing() && !sys.disk.control.transfer_ongoing() {
        tracing::debug!("starting DI transfer");
        sys.disk.control.set_transfer_ongoing(true);

        let command = sys.disk.command();
        match command {
            Command::Identify => {
                // TODO: is this right?
                let target = sys.mem.translate_data_addr(sys.disk.dma_base).unwrap();
                let length = sys.disk.dma_length;
                assert_eq!(length, 32);

                sys.mem.ram_mut()[target.value() as usize..][..12].copy_from_slice(&[
                    0x00, 0x00, 0x00, 0x00, // zeros
                    0x20, 0x02, 0x04, 0x02, // date
                    0x61, 0x00, 0x00, 0x00, // version
                ]);

                sys.mem.ram_mut()[target.value() as usize + 12..][..32 - 12].fill(0);
                sys.scheduler.schedule(10000, complete_transfer);
            }
            Command::Read { offset, length } => {
                assert!(sys.disk.control.dma());
                assert_eq!(sys.disk.control.mode(), TransferMode::Read);
                assert_eq!(sys.disk.dma_length, length);

                // load from disk!
                let target = sys.disk.dma_base;
                if length == 0 {
                    tracing::warn!(
                        "ignoring zero sized disk read from 0x{offset:08X} into {target}"
                    );
                    sys.disk.control.set_transfer_ongoing(false);
                    return;
                }

                tracing::debug!(
                    "reading 0x{length:08X} bytes from disk at 0x{offset:08X} into {target}"
                );

                let target = target.value().with_bits(26, 32, 0);
                let slice = &mut sys.mem.ram_mut()[target as usize..][..length as usize];

                if !sys.modules.disk.has_disk() {
                    tracing::error!("tried to read from disk but no disk is inserted");
                    slice.fill(0);
                } else {
                    let new = sys
                        .modules
                        .disk
                        .seek(SeekFrom::Start(offset as u64))
                        .unwrap();

                    assert_eq!(new, offset as u64);

                    sys.modules.disk.read_exact(slice).unwrap();
                }

                sys.scheduler.schedule(10000, complete_transfer);
            }
            Command::Seek { .. } => {
                tracing::warn!("stubbed DVD command - disk seek");
                sys.scheduler.schedule(5000, complete_seek);
            }
            Command::StopMotor => {
                tracing::warn!("stubbed DVD command - stop motor");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            Command::StartAudioStream { .. } => {
                tracing::warn!("stubbed DVD command - start audio stream");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            Command::StopAudioStream => {
                tracing::warn!("stubbed DVD command - stop audio stream");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            Command::AudioStreamStatus => {
                tracing::warn!("stubbed DVD command - audio stream status");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            Command::EnableAudioStream => {
                tracing::warn!("stubbed DVD command - enable audio stream");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            Command::DisableAudioStream => {
                tracing::warn!("stubbed DVD command - disable audio stream");
                sys.disk.status.set_transfer_interrupt(true);
                sys.disk.control.set_transfer_ongoing(false);
                sys.disk.immediate = 0;
            }
            _ => panic!("unimplemented disk command: {:?}", command),
        }
    }
}

// TODO: figure this out
pub fn reset(sys: &mut System, value: u32) {
    if !value.bit(2) {
        return;
    }

    tracing::warn!("dvd drive reset through processor interface");
    sys.disk = Default::default();
}
