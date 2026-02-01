//! Processor interface (PI).
use bitos::bitos;
use bitos::integer::u26;
use gekko::{Address, Exception};

use crate::Primitive;
use crate::system::{System, gx};

#[bitos(14)]
#[derive(Default, Clone, Copy)]
pub struct InterruptSources {
    #[bits(0)]
    pub gp_error: bool,
    #[bits(1)]
    pub reset: bool,
    #[bits(2)]
    pub dvd_interface: bool,
    #[bits(3)]
    pub serial_interface: bool,
    #[bits(4)]
    pub external_interface: bool,
    #[bits(5)]
    pub audio_interface: bool,
    #[bits(6)]
    pub dsp_interface: bool,
    #[bits(7)]
    pub memory_interface: bool,
    #[bits(8)]
    pub video_interface: bool,
    #[bits(9)]
    pub pe_token: bool,
    #[bits(10)]
    pub pe_finish: bool,
    #[bits(11)]
    pub command_processor: bool,
    #[bits(12)]
    pub debug: bool,
    #[bits(13)]
    pub high_speed_port: bool,
}

impl std::fmt::Debug for InterruptSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut set = f.debug_set();
        macro_rules! debug {
            ($($ident:ident),*) => {
                $(
                    if self.$ident() {
                        set.entry(&stringify!($ident));
                    }
                )*
            };
        }

        debug! {
            gp_error,
            reset,
            dvd_interface,
            serial_interface,
            external_interface,
            audio_interface,
            dsp_interface,
            memory_interface,
            video_interface,
            pe_token,
            pe_finish,
            command_processor,
            debug,
            high_speed_port
        }

        set.finish_non_exhaustive()
    }
}

#[bitos(32)]
#[derive(Default, Debug, Clone, Copy)]
pub struct InterruptMask {
    #[bits(0..14)]
    pub sources: InterruptSources,
}

#[bitos(32)]
#[derive(Default, Debug, Clone, Copy)]
pub struct FifoCurrent {
    #[bits(0..26)]
    pub base: u26,
    #[bits(29)]
    pub wrapped: bool,
}

impl FifoCurrent {
    pub fn address(&self) -> Address {
        Address(self.base().value())
    }

    pub fn set_address(&mut self, value: Address) {
        self.set_base(u26::new(value.value()));
    }
}

pub struct Interface {
    // interrupts
    pub mask: InterruptMask,

    // fifo
    pub fifo_start: Address,
    pub fifo_end: Address,
    pub fifo_current: FifoCurrent,

    fifo_queue: [u8; 36],
    fifo_queue_index: usize,
}

impl Default for Interface {
    fn default() -> Self {
        Self {
            mask: Default::default(),
            fifo_start: Default::default(),
            fifo_end: Default::default(),
            fifo_current: Default::default(),

            fifo_queue: [0; 36],
            fifo_queue_index: 0,
        }
    }
}

/// Returns which interrupt sources are active (i.e. triggered but maybe masked).
pub fn get_active_interrupts(sys: &System) -> InterruptSources {
    let mut sources = InterruptSources::default();

    // VI
    let mut video = false;
    for i in &sys.video.interrupts {
        video |= i.enable() && i.status();
    }
    sources.set_video_interface(video);

    // PE
    sources.set_pe_token(sys.gpu.pix.interrupt.token() && sys.gpu.pix.interrupt.token_enabled());
    sources.set_pe_finish(sys.gpu.pix.interrupt.finish() && sys.gpu.pix.interrupt.finish_enabled());

    // AI
    sources.set_audio_interface(
        sys.audio.control.interrupt() && sys.audio.control.interrupt_enabled(),
    );

    // DSP
    sources.set_dsp_interface(sys.dsp.control.any_interrupt());

    // DI
    sources.set_dvd_interface(sys.disk.status.any_interrupt());

    // SI
    sources.set_serial_interface(sys.serial.any_interrupt());

    sources
}

/// Returns which interrupt sources are raised (i.e. triggered and unmasked).
pub fn get_raised_interrupts(sys: &System) -> InterruptSources {
    InterruptSources::from_bits(
        self::get_active_interrupts(sys).to_bits() & sys.processor.mask.sources().to_bits(),
    )
}

/// Checks whether any of the currently raised interrutps can be taken and, if any, raises the
/// interrupt exception.
pub fn check_interrupts(sys: &mut System) {
    if !sys.cpu.supervisor.config.msr.interrupts() {
        return;
    }

    let raised = self::get_raised_interrupts(sys);
    if raised.to_bits().value() != 0 {
        tracing::debug!("raising interrupt exception for {raised:?}");
        sys.cpu.raise_exception(Exception::Interrupt);
    }
}

/// Pushes a value into the PI FIFO. Values are queued up until 32 bytes are available, then
/// written all at once.
pub fn fifo_push<P: Primitive>(sys: &mut System, value: P) {
    value.write_be_bytes(
        &mut sys.processor.fifo_queue[sys.processor.fifo_queue_index..][..size_of::<P>()],
    );
    sys.processor.fifo_queue_index += size_of::<P>();

    if sys.processor.fifo_queue_index < 32 {
        return;
    }

    let mut data = [0; 32];
    data.copy_from_slice(&sys.processor.fifo_queue[..32]);

    for byte in data {
        let current = sys.processor.fifo_current.address();
        sys.write_phys_slow(current, byte);
        sys.processor.fifo_current.set_address(current + 1);
        if sys.processor.fifo_current.address() > sys.processor.fifo_end {
            std::hint::cold_path();
            sys.processor.fifo_current.set_wrapped(true);
            sys.processor
                .fifo_current
                .set_address(sys.processor.fifo_start);
        }
    }

    sys.processor
        .fifo_queue
        .copy_within(32..sys.processor.fifo_queue_index, 0);
    sys.processor.fifo_queue_index -= 32;

    if sys.gpu.cmd.control.linked_mode() {
        gx::cmd::sync_to_pi(sys);
        gx::cmd::consume(sys);
    }
}
