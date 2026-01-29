//! External interface (EXI).
use std::io::Write;

use bitos::bitos;
use bitos::integer::{u2, u3};
use gekko::Address;
use util::boxed_array;

use crate::Primitive;
use crate::system::System;

pub const SRAM_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device0 {
    MemoryCardA,
    IplRtcSram,
    SerialPort1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device1 {
    MemoryCardB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device2 {
    AD16,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Parameter {
    #[bits(0)]
    pub device_interrupt_mask: bool,
    #[bits(1)]
    pub device_interrupt: bool,
    #[bits(2)]
    pub transfer_interrupt_mask: bool,
    #[bits(3)]
    pub transfer_interrupt: bool,
    #[bits(4..7)]
    pub clock_multiplier: u3,
    #[bits(7..10)]
    pub device_select: u3,
    #[bits(10)]
    pub attach_interrupt_mask: bool,
    #[bits(11)]
    pub attach_interrupt: bool,
    #[bits(12)]
    pub device_connected: bool,
}

impl Parameter {
    pub fn write(&mut self, value: Parameter) {
        self.set_device_interrupt_mask(value.device_interrupt_mask());
        self.set_device_interrupt(self.device_interrupt() & !value.device_interrupt());
        self.set_transfer_interrupt_mask(value.transfer_interrupt_mask());
        self.set_transfer_interrupt(self.transfer_interrupt() & !value.transfer_interrupt());

        self.set_clock_multiplier(value.clock_multiplier());
        self.set_device_select(value.device_select());

        self.set_attach_interrupt_mask(value.attach_interrupt_mask());
        self.set_attach_interrupt(self.attach_interrupt() & !value.attach_interrupt());
    }

    pub fn device0(&self) -> Option<Device0> {
        Some(match self.device_select().value() {
            0b001 => Device0::MemoryCardA,
            0b010 => Device0::IplRtcSram,
            0b100 => Device0::SerialPort1,
            _ => return None,
        })
    }

    pub fn device1(&self) -> Option<Device1> {
        Some(match self.device_select().value() {
            0b001 => Device1::MemoryCardB,
            _ => return None,
        })
    }

    pub fn device2(&self) -> Option<Device2> {
        Some(match self.device_select().value() {
            0b001 => Device2::AD16,
            _ => return None,
        })
    }
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Read      = 0b00,
    Write     = 0b01,
    ReadWrite = 0b10,
    Reserved  = 0b11,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Control {
    #[bits(0)]
    pub transfer_ongoing: bool,
    #[bits(1)]
    pub dma: bool,
    #[bits(2..4)]
    pub transfer_mode: TransferMode,
    #[bits(4..6)]
    pub imm_length_minus_one: u2,
}

impl Control {
    pub fn imm_length(&self) -> u32 {
        self.imm_length_minus_one().value() as u32 + 1
    }
}

#[derive(Debug, Clone, Default)]
pub enum IplChipState {
    #[default]
    Idle,
    SramWrite(u8),
    UartWrite,
}

#[derive(Debug, Clone, Default)]
pub struct Channel0 {
    pub rtc: u32,
    pub ipl_base: u32,
    pub ipl_state: IplChipState,

    pub parameter: Parameter,
    pub control: Control,
    pub dma_base: Address,
    pub dma_length: u32,
    pub immediate: u32,
}

#[derive(Default, Debug, Clone)]
pub struct Channel1 {
    pub parameter: Parameter,
    pub control: Control,
    pub dma_base: Address,
    pub dma_length: u32,
    pub immediate: u32,
}

#[derive(Default, Debug, Clone)]
pub struct Channel2 {
    pub parameter: Parameter,
    pub control: Control,
    pub dma_base: Address,
    pub dma_length: u32,
    pub immediate: u32,
}

pub struct Interface {
    pub sram: Box<[u8; SRAM_LEN]>,
    pub channel0: Channel0,
    pub channel1: Channel0,
    pub channel2: Channel0,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            sram: boxed_array(0),
            channel0: Default::default(),
            channel1: Default::default(),
            channel2: Default::default(),
        }
    }
}

fn ipl_transfer(sys: &mut System) {
    if !sys.external.channel0.control.dma() {
        sys.external.channel0.ipl_base = sys.external.channel0.immediate >> 6;
        tracing::debug!("set IPL base to 0x{:08X}", sys.external.channel0.ipl_base);
        return;
    }

    let ram_base = sys.external.channel0.dma_base.value() as usize;
    let ipl_base = sys.external.channel0.ipl_base as usize;
    let length = sys.external.channel0.dma_length as usize;
    tracing::debug!(
        "IPL ROM DMA: 0x{:08X} bytes from IPL 0x{:08X} to RAM 0x{:08X}",
        length,
        ipl_base,
        ram_base
    );

    let regions = sys.mem.regions();
    regions.ram[ram_base..][..length].copy_from_slice(&regions.ipl[ipl_base..][..length]);
}

fn update_sram_checksum(sys: &mut System) {
    let mut c1 = 0u16;
    let mut c2 = 0u16;
    sys.external.sram[0x13] = 0b0110_1100;

    for i in 0..4 {
        let word = u16::read_be_bytes(&sys.external.sram[0xC + 2 * i..]);
        c1 = c1.wrapping_add(word);
        c2 = c2.wrapping_add(word ^ 0xFFFF);
    }

    c1.write_be_bytes(&mut sys.external.sram[0..2]);
    c2.write_be_bytes(&mut sys.external.sram[2..4]);
}

fn sram_transfer_read(sys: &mut System) {
    self::update_sram_checksum(sys);

    let sram_base =
        (((sys.external.channel0.immediate & !0xA000_0000) - 0x0000_0100) >> 6) as usize;
    tracing::debug!("SRAM TRANSFER {:?}", sys.external.channel0.control);

    if !sys.external.channel0.control.dma() {
        return;
    }

    let ram_base = sys.external.channel0.dma_base.value() as usize;
    let length = sys.external.channel0.dma_length as usize;
    tracing::debug!(
        "SRAM DMA: 0x{:08X} bytes from SRAM 0x{:08X} to RAM 0x{:08X}",
        length,
        sram_base,
        ram_base
    );

    sys.mem.ram_mut()[ram_base..][..length]
        .copy_from_slice(&sys.external.sram[sram_base..][..length]);
}

fn sram_transfer_write(sys: &mut System, current: u8) {
    assert!(!sys.external.channel0.control.dma());

    sys.external
        .channel0
        .immediate
        .write_be_bytes(&mut sys.external.sram[current as usize..]);

    let next = current + 4;
    if next == 64 {
        sys.external.channel0.ipl_state = IplChipState::Idle;
    } else {
        sys.external.channel0.ipl_state = IplChipState::SramWrite(next);
    }
}

fn uart_transfer_write(sys: &mut System) {
    assert!(!sys.external.channel0.control.dma());
    let value = sys.external.channel0.immediate;

    for byte in value.to_be_bytes() {
        if byte == 0x1B {
            continue;
        }

        print!("{}", char::from_u32(byte as u32).unwrap());
        if byte == b'\r' {
            println!();
        }
    }

    std::io::stdout().flush().unwrap();
}

fn uart_transfer_read(sys: &mut System) {
    if !sys.external.channel0.control.dma() {
        sys.external.channel0.immediate = 0;
        return;
    }

    let ram_base = sys.external.channel0.dma_base.value() as usize;
    let length = sys.external.channel0.dma_length as usize;
    tracing::debug!(
        "UART DMA: 0x{:08X} bytes from UART to RAM 0x{:08X}",
        length,
        ram_base
    );

    sys.mem.ram_mut()[ram_base..][..length].fill(0);
}

fn ipl_rtc_sram_transfer(sys: &mut System) {
    match sys.external.channel0.clone().ipl_state {
        IplChipState::SramWrite(current) => self::sram_transfer_write(sys, current),
        IplChipState::UartWrite => self::uart_transfer_write(sys),
        IplChipState::Idle => {
            // new transfer
            match sys.external.channel0.clone().immediate {
                0x0000_0000..0x2000_0000 => self::ipl_transfer(sys),
                0x2000_0000 => {
                    tracing::debug!("RTC read: 0x{:08X}", sys.external.channel0.rtc);
                    assert!(!sys.external.channel0.control.dma());
                    sys.external.channel0.immediate = sys.external.channel0.rtc;
                }
                0x2000_0100..0x2000_1100 => self::sram_transfer_read(sys),
                0x2001_0000 => self::uart_transfer_read(sys),
                0xA000_0000 => {
                    tracing::debug!("RTC write: 0x{:08X}", sys.external.channel0.immediate);
                    assert!(!sys.external.channel0.control.dma());
                    sys.external.channel0.rtc = sys.external.channel0.immediate;
                }
                0xA000_0100..0xA000_1100 => {
                    let sram_base = (((sys.external.channel0.immediate & !0xA000_0000)
                        - 0x0000_0100)
                        >> 6) as u8;
                    tracing::debug!("starting SRAM write: 0x{:08X}", sram_base);
                    assert!(!sys.external.channel0.control.dma());

                    sys.external.channel0.ipl_state = IplChipState::SramWrite(sram_base);
                }
                0xA001_0000 => {
                    tracing::debug!("starting EXI UART write");
                    sys.external.channel0.ipl_state = IplChipState::UartWrite;
                }
                _ => todo!("{:08X}", sys.external.channel0.immediate),
            }
        }
    }

    sys.external.channel0.control.set_transfer_ongoing(false);
}

pub fn channel0_transfer(sys: &mut System) {
    match sys.external.channel0.parameter.device0().unwrap() {
        Device0::IplRtcSram => {
            self::ipl_rtc_sram_transfer(sys);
        }
        Device0::SerialPort1 => {
            // no ethernet adapter
            tracing::debug!("SP1 read - ignoring");
            sys.external.channel0.immediate = 0;
            sys.external.channel0.control.set_transfer_ongoing(false);
        }
        _ => todo!(),
    }
}

pub fn channel2_transfer(sys: &mut System) {
    assert_eq!(
        sys.external.channel2.parameter.device2(),
        Some(Device2::AD16)
    );

    let op = sys.external.channel2.immediate;
    if op == 0 {
        tracing::warn!("checking AD16 ID");
        sys.external.channel2.immediate = 0x0412_0000;
    } else {
        tracing::warn!("unknown AD16 op");
    }

    sys.external.channel2.control.set_transfer_ongoing(false);
}

pub fn update(sys: &mut System) {
    if sys.external.channel0.control.transfer_ongoing() {
        self::channel0_transfer(sys);
    }

    if sys.external.channel2.control.transfer_ongoing() {
        self::channel2_transfer(sys);
    }
}
