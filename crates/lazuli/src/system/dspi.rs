//! DSP interface (DSPI).
use bitos::integer::{u15, u31};
use bitos::{BitUtils, bitos};
use gekko::Address;
use util::boxed_array;

use crate::system::System;

pub const ARAM_LEN: usize = 16 * bytesize::MIB as usize;

#[bitos(32)]
#[derive(Debug, Default)]
pub struct Mailbox {
    #[bits(0..16)]
    pub low: u16,
    #[bits(16..31)]
    pub high: u15,
    #[bits(16..32)]
    pub high_and_status: u16,

    #[bits(0..31)]
    pub data: u31,
    #[bits(31)]
    pub status: bool,
}

#[bitos(16)]
#[derive(Debug, Clone, Copy)]
pub struct Control {
    #[bits(0)]
    pub reset: bool,
    #[bits(1)]
    pub interrupt: bool,
    #[bits(2)]
    pub halt: bool,
    #[bits(3)]
    pub ai_interrupt: bool,
    #[bits(4)]
    pub ai_interrupt_mask: bool,
    #[bits(5)]
    pub aram_interrupt: bool,
    #[bits(6)]
    pub aram_interrupt_mask: bool,
    #[bits(7)]
    pub dsp_interrupt: bool,
    #[bits(8)]
    pub dsp_interrupt_mask: bool,
    #[bits(9)]
    pub aram_dma_ongoing: bool,
    #[bits(10)]
    pub unknown: bool,
    #[bits(11)]
    pub reset_high: bool,
}

impl Default for Control {
    fn default() -> Self {
        Self::from_bits(0).with_reset_high(true)
    }
}

impl Control {
    pub fn any_interrupt(&self) -> bool {
        let ai = self.ai_interrupt() && self.ai_interrupt_mask();
        let aram = self.aram_interrupt() && self.aram_interrupt_mask();
        let dsp = self.dsp_interrupt() && self.dsp_interrupt_mask();
        ai || aram || dsp
    }
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AramDmaDirection {
    FromRamToAram = 0,
    FromAramToRam = 1,
}

#[bitos(32)]
#[derive(Debug, Clone, Default)]
pub struct AramDmaControl {
    #[bits(0..31)]
    pub length: u31,
    #[bits(31)]
    pub direction: AramDmaDirection,
}

#[derive(Default)]
pub struct AramDma {
    pub ram_base: Address,
    pub aram_base: u32,
    pub control: AramDmaControl,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspDmaDirection {
    FromRamToDsp = 0,
    FromDspToRam = 1,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspDmaTarget {
    Dmem = 0,
    Imem = 1,
}

#[bitos(16)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DspDmaControl {
    #[bits(0)]
    pub direction: DspDmaDirection,
    #[bits(1)]
    pub dsp_target: DspDmaTarget,
    #[bits(2)]
    pub transfer_ongoing: bool,
}

#[derive(Default)]
pub struct DspDma {
    pub ram_base: u32,
    pub dsp_base: u16,
    pub length: u16,
    pub control: DspDmaControl,
}

pub struct Dsp {
    pub control: Control,
    /// Data from DSP to CPU
    pub dsp_mailbox: Mailbox,
    /// Data from CPU to DSP
    pub cpu_mailbox: Mailbox,
    pub dsp_dma: DspDma,
    pub aram_dma: AramDma,
    pub aram_len: u32,
    pub aram: Box<[u8; ARAM_LEN]>,
}

impl Dsp {
    pub fn new() -> Self {
        Self {
            control: Default::default(),
            dsp_mailbox: Default::default(),
            cpu_mailbox: Default::default(),
            dsp_dma: Default::default(),
            aram_dma: Default::default(),
            aram_len: 0,
            aram: boxed_array(0),
        }
    }
}

pub fn write_control(sys: &mut System, value: Control) {
    sys.dsp.control.set_reset(value.reset());
    sys.dsp.control.set_halt(value.halt());

    // DSP external interrupt
    sys.dsp.control.set_interrupt(value.interrupt());

    // PI DMA interrupts
    sys.dsp
        .control
        .set_ai_interrupt(sys.dsp.control.ai_interrupt() & !value.ai_interrupt());
    sys.dsp
        .control
        .set_ai_interrupt_mask(value.ai_interrupt_mask());

    sys.dsp
        .control
        .set_aram_interrupt(sys.dsp.control.aram_interrupt() & !value.aram_interrupt());
    sys.dsp
        .control
        .set_aram_interrupt_mask(value.aram_interrupt_mask());

    sys.dsp
        .control
        .set_dsp_interrupt(sys.dsp.control.dsp_interrupt() & !value.dsp_interrupt());
    sys.dsp
        .control
        .set_dsp_interrupt_mask(value.dsp_interrupt_mask());

    sys.dsp.control.set_unknown(value.unknown());
    sys.dsp.control.set_reset_high(value.reset_high());
}

/// Performs the ARAM DMA if length is not zero.
pub fn aram_dma(sys: &mut System) {
    let ram_base = sys.dsp.aram_dma.ram_base.value().with_bits(26, 32, 0);
    let aram_base = sys.dsp.aram_dma.aram_base as usize;

    if aram_base >= ARAM_LEN {
        // software will try to DMA from out-of-bounds ARAM regions to test for ARAM expansion. in
        // this case, just ignore it
        sys.dsp.aram_dma.control.set_length(u31::new(0));
        sys.dsp.control.set_aram_interrupt(true);
        sys.dsp.control.set_aram_dma_ongoing(false);
        return;
    }

    let max_length = ARAM_LEN - aram_base;
    let length = sys.dsp.aram_dma.control.length().value() as usize;
    let effective_length = length.min(max_length);

    match sys.dsp.aram_dma.control.direction() {
        AramDmaDirection::FromRamToAram => {
            tracing::debug!(
                "ARAM DMA {effective_length} bytes from RAM {} to ARAM {aram_base:08X}",
                Address(ram_base)
            );

            let aram = &mut sys.dsp.aram[aram_base as usize..][..effective_length];
            aram.copy_from_slice(&sys.mem.ram()[ram_base as usize..][..effective_length]);
        }
        AramDmaDirection::FromAramToRam => {
            tracing::debug!(
                "ARAM DMA {effective_length} bytes from ARAM {aram_base:08X} to RAM {}",
                Address(ram_base)
            );

            sys.mem.ram_mut()[ram_base as usize..][..effective_length]
                .copy_from_slice(&sys.dsp.aram[aram_base as usize..][..effective_length]);
        }
    }

    sys.dsp.aram_dma.control.set_length(u31::new(0));
    sys.dsp.control.set_aram_interrupt(true);
    sys.dsp.control.set_aram_dma_ongoing(false);
}
