//! Audio interface (AI).
use bitos::integer::u15;
use bitos::{BitUtils, bitos};
use gekko::Address;
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::system::scheduler::HandlerCtx;
use crate::system::{System, pi};

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    KHz48 = 0,
    KHz32 = 1,
}

impl SampleRate {
    pub fn value(self) -> u16 {
        match self {
            Self::KHz48 => 48_000,
            Self::KHz32 => 32_000,
        }
    }

    pub fn cycles_per_frame(self) -> u64 {
        gekko::FREQUENCY / self.value() as u64
    }

    pub fn cycles_per_block(self) -> u64 {
        8 * self.cycles_per_frame()
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Control {
    #[bits(0)]
    pub playing: bool,
    #[bits(1)]
    pub aux_sample_rate: SampleRate,
    #[bits(2)]
    pub interrupt_enabled: bool,
    #[bits(3)]
    pub interrupt: bool,
    #[bits(4)]
    pub interrupt_valid: bool,
    #[bits(5)]
    pub sample_counter_reset: bool,
    #[bits(6)]
    pub dsp_sample_rate: SampleRate,
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DmaControl {
    #[bits(0..15)]
    pub length_by_32: u15,
    #[bits(15)]
    pub playing: bool,
}

#[derive(Default)]
pub struct Interface {
    pub control: Control,
    pub dma_base: Address,
    pub dma_control: DmaControl,
    pub current_dma_block: u16,
    pub sample_counter: u32,
    pub interrupt_sample: u32,
}

impl Interface {
    pub fn write_control(&mut self, value: Control) {
        self.control.set_playing(value.playing());
        self.control.set_aux_sample_rate(value.aux_sample_rate());
        self.control
            .set_interrupt_enabled(value.interrupt_enabled());
        self.control
            .set_interrupt(self.control.interrupt() & !value.interrupt());
        self.control.set_interrupt_valid(value.interrupt_valid());

        if value.sample_counter_reset() {
            self.sample_counter = 0;
        }

        self.control.set_dsp_sample_rate(value.dsp_sample_rate());
    }

    pub fn any_interrupt(&self) -> bool {
        self.control.interrupt_enabled() && self.control.interrupt()
    }

    /// How many DMA bytes are remaining to be transferred.
    pub fn dma_remaining(&self) -> u16 {
        32 * (self.dma_control.length_by_32().value() - self.current_dma_block)
    }
}

fn push_streaming_frame(sys: &mut System, ctx: HandlerCtx) {
    sys.audio.sample_counter += 1;
    if sys.audio.control.interrupt_valid() && sys.audio.sample_counter == sys.audio.interrupt_sample
    {
        println!("raising sample counter int");
        sys.audio.control.set_interrupt(true);
        pi::check_interrupts(sys);
    }

    sys.scheduler.schedule_full(
        sys.audio.control.aux_sample_rate().cycles_per_frame() - ctx.cycles_late.value(),
        self::push_streaming_frame,
    );
}

pub fn start_streaming(sys: &mut System) {
    if !sys.scheduler.contains_full(self::push_streaming_frame) {
        sys.scheduler.schedule_full(
            sys.audio.control.aux_sample_rate().cycles_per_frame(),
            self::push_streaming_frame,
        );
    }
}

pub fn stop_streaming(sys: &mut System) {
    sys.scheduler.cancel_full(self::push_streaming_frame);
}

#[derive(Debug, Clone, Copy, Default, IntoBytes, FromBytes, Immutable)]
#[repr(C)]
pub struct Frame {
    pub left: i16,
    pub right: i16,
}

fn push_data_dma_block(sys: &mut System, ctx: HandlerCtx) {
    let addr =
        Address(sys.audio.dma_base.0.with_bit(31, false)) + 32 * sys.audio.current_dma_block as u32;
    let frames: [Frame; 8] = std::array::from_fn(|i| Frame {
        left: sys.read_phys_slow::<i16>(addr + 4 * i as u32 + 2),
        right: sys.read_phys_slow::<i16>(addr + 4 * i as u32),
    });

    for frame in frames {
        sys.modules.audio.play(frame);
    }

    sys.audio.current_dma_block += 1;

    let total_blocks = sys.audio.dma_control.length_by_32().value();
    if sys.audio.current_dma_block >= total_blocks {
        sys.dsp.control.set_ai_dma_interrupt(true);
        sys.audio.current_dma_block = 0;
        pi::check_interrupts(sys);

        // NOTE: it's important to only check this at the end of transfers - if a transfer is
        // started, it must execute until completion! (breaks Mario Sunshine otherwise)
        if !sys.audio.dma_control.playing() {
            return;
        }
    }

    sys.scheduler.schedule_full(
        sys.audio.control.dsp_sample_rate().cycles_per_block() - ctx.cycles_late.value(),
        self::push_data_dma_block,
    );
}

pub fn start_data_dma(sys: &mut System) {
    sys.modules
        .audio
        .set_sample_rate(sys.audio.control.dsp_sample_rate());

    if !sys.scheduler.contains_full(self::push_data_dma_block) {
        sys.scheduler.schedule_full(
            sys.audio.control.dsp_sample_rate().cycles_per_block(),
            self::push_data_dma_block,
        );
    }
}

pub fn stop_data_dma(sys: &mut System) {
    sys.scheduler.cancel_full(self::push_data_dma_block);
}
