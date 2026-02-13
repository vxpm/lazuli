//! State of the system (i.e. GameCube and emulator).

pub mod bus;
pub mod eabi;
pub mod executable;
pub mod ipl;
pub mod lazy;
pub mod os;
pub mod scheduler;

pub mod ai;
pub mod di;
pub mod dspi;
pub mod exi;
pub mod gx;
pub mod mem;
pub mod pi;
pub mod si;
pub mod vi;

use std::io::{Cursor, SeekFrom};

use disks::binrw::BinRead;
use disks::{apploader, dol, iso};
use easyerr::{Error, ResultExt};
use gekko::{Address, Cpu, Cycles};

use crate::modules::audio::AudioModule;
use crate::modules::debug::DebugModule;
use crate::modules::disk::DiskModule;
use crate::modules::input::InputModule;
use crate::modules::render::RenderModule;
use crate::modules::vertex::VertexModule;
use crate::system::dspi::Dsp;
use crate::system::executable::Executable;
use crate::system::gx::Gpu;
use crate::system::ipl::Ipl;
use crate::system::lazy::Lazy;
use crate::system::mem::Memory;
use crate::system::scheduler::{HandlerCtx, Scheduler};

/// System configuration.
pub struct Config {
    pub ipl_lle: bool,
    pub ipl: Option<Vec<u8>>,
    pub sideload: Option<Executable>,
    pub perform_efb_copies: bool,
}

/// System modules.
pub struct Modules {
    pub audio: Box<dyn AudioModule>,
    pub debug: Box<dyn DebugModule>,
    pub disk: Box<dyn DiskModule>,
    pub input: Box<dyn InputModule>,
    pub render: Box<dyn RenderModule>,
    pub vertex: Box<dyn VertexModule>,
}

/// System state.
pub struct System {
    /// System configuration.
    pub config: Config,
    /// System modules.
    pub modules: Modules,
    /// Scheduler for events.
    pub scheduler: Scheduler,
    /// The CPU state.
    pub cpu: Cpu,
    /// The GPU state.
    pub gpu: Gpu,
    /// The DSP state.
    pub dsp: Dsp,
    /// System memory.
    pub mem: Memory,
    /// State of mechanisms that update lazily (e.g. time related registers).
    pub lazy: Lazy,
    /// The video interface.
    pub video: vi::Interface,
    /// The processor interface.
    pub processor: pi::Interface,
    /// The external interface.
    pub external: exi::Interface,
    /// The audio interface.
    pub audio: ai::Interface,
    /// The disk interface.
    pub disk: di::Interface,
    /// The serial interface.
    pub serial: si::Interface,
}

#[derive(Debug, Error)]
pub enum LoadApploaderError {
    #[error(transparent)]
    Io { source: std::io::Error },
    #[error(transparent)]
    Apploader { source: disks::binrw::Error },
}

impl System {
    fn load_apploader(&mut self) -> Result<Address, LoadApploaderError> {
        self.modules
            .disk
            .seek(SeekFrom::Start(0x2440))
            .context(LoadApploaderCtx::Io)?;

        let apploader = apploader::Apploader::read(&mut self.modules.disk)
            .context(LoadApploaderCtx::Apploader)?;

        let size = apploader.header.size;
        self.mem.ram_mut()[0x0120_0000..][..size as usize].copy_from_slice(&apploader.body);

        Ok(Address(apploader.header.entrypoint))
    }

    fn load_executable(&mut self) {
        let Some(exec) = self.config.sideload.take() else {
            return;
        };

        match &exec {
            Executable::Dol(dol) => {
                self.cpu.pc = Address(dol.entrypoint());
                self.cpu.supervisor.memory.setup_default_bats();
                self.mem.build_bat_lut(&self.cpu.supervisor.memory);

                self.cpu
                    .supervisor
                    .config
                    .msr
                    .set_instr_addr_translation(true);
                self.cpu
                    .supervisor
                    .config
                    .msr
                    .set_data_addr_translation(true);

                // zero bss first, let other sections overwrite it if it occurs
                for offset in 0..dol.header.bss_size {
                    self.write(Address(dol.header.bss_target + offset), 0u8);
                }

                for section in dol.text_sections() {
                    for (offset, byte) in section.content.iter().copied().enumerate() {
                        self.write(Address(section.target) + offset as u32, byte);
                    }
                }

                for section in dol.data_sections() {
                    for (offset, byte) in section.content.iter().copied().enumerate() {
                        self.write(Address(section.target) + offset as u32, byte);
                    }
                }
            }
        }

        self.config.sideload = Some(exec);
        tracing::debug!("finished loading executable");
    }

    fn load_ipl_hle(&mut self) {
        self.cpu.supervisor.memory.setup_default_bats();
        self.mem.build_bat_lut(&self.cpu.supervisor.memory);

        self.modules
            .disk
            .seek(SeekFrom::Start(0))
            .context(LoadApploaderCtx::Io)
            .unwrap();

        let header = iso::Header::read(&mut self.modules.disk)
            .context(LoadApploaderCtx::Apploader)
            .unwrap();

        tracing::info!(
            game_code = header.meta.game_code(),
            maker_code = header.meta.maker_code,
            disk_id = header.meta.disk_id,
            version = header.meta.version,
            audio_streaming = header.meta.audio_streaming,
            stream_buffer_size = header.meta.stream_buffer_size,
            "loading '{}' ({}) using IPL HLE",
            header.meta.game_name,
            header
                .meta
                .game_code_str()
                .as_deref()
                .unwrap_or("<unknown>")
        );

        // load apploader
        let entry = self.load_apploader().unwrap();

        // load ipl-hle
        let mut cursor = Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../local/ipl-hle.dol"
        )));
        let ipl = dol::Dol::read(&mut cursor).unwrap();
        self.config.sideload = Some(Executable::Dol(ipl));
        self.load_executable();

        // setup apploader entrypoint for ipl-hle
        self.cpu.user.gpr[3] = entry.value();

        // load dolphin-os globals
        self.write_phys_slow::<u32>(Address(0x00), header.meta.game_code());
        self.write_phys_slow::<u16>(Address(0x04), header.meta.maker_code);
        self.write_phys_slow::<u8>(Address(0x06), header.meta.disk_id);
        self.write_phys_slow::<u8>(Address(0x07), header.meta.version);
        self.write_phys_slow::<u8>(Address(0x08), header.meta.audio_streaming);
        self.write_phys_slow::<u8>(Address(0x09), header.meta.stream_buffer_size);

        self.write_phys_slow::<u32>(Address(0x1C), 0xC233_9F3D); // DVD Magic Word
        self.write_phys_slow::<u32>(Address(0x20), 0x0D15_EA5E); // Boot kind
        self.write_phys_slow::<u32>(Address(0x24), 0x0000_0001); // Version
        self.write_phys_slow::<u32>(Address(0x28), 0x0180_0000); // Physical Memory Size
        self.write_phys_slow::<u32>(Address(0x2C), 0x1000_0005); // Console Type
        self.write_phys_slow::<u32>(Address(0x30), 0x8042_E260); // Arena Low
        self.write_phys_slow::<u32>(Address(0x34), 0x817F_E8C0); // Arena High
        self.write_phys_slow::<u32>(Address(0x38), 0x817F_E8C0); // FST address
        self.write_phys_slow::<u32>(Address(0x3C), 0x0000_0024); // FST max length
        // TODO: deal with TV mode, games hang if it is wrong...
        self.write_phys_slow::<u32>(Address(0xCC), 0x0000_0000); // TV Mode
        self.write_phys_slow::<u32>(Address(0xD0), 0x0100_0000); // ARAM size
        self.write_phys_slow::<u32>(Address(0xF8), 0x09A7_EC80); // Bus clock
        self.write_phys_slow::<u32>(Address(0xFC), 0x1CF7_C580); // CPU clock

        self.video
            .display_config
            .set_video_format(vi::VideoFormat::Pal50);

        // setup MSR
        self.cpu.supervisor.config.msr.set_exception_prefix(false);

        // done :)
    }

    fn load_ipl(&mut self) {
        self.cpu.supervisor.config.msr.set_exception_prefix(true);
        self.cpu.pc = Address(0xFFF0_0100);
    }

    pub fn new(modules: Modules, mut config: Config) -> Self {
        let mut scheduler = Scheduler::default();
        scheduler.schedule(1 << 16, gx::cmd::process);

        let ipl = Ipl::new(config.ipl.take().unwrap_or_else(|| vec![0; mem::IPL_LEN]));

        let mut system = System {
            scheduler,
            cpu: Cpu::default(),
            gpu: Gpu::default(),
            dsp: Dsp::new(),
            mem: Memory::new(&ipl),
            lazy: Lazy::default(),
            video: vi::Interface::default(),
            processor: pi::Interface::default(),
            external: exi::Interface::new(),
            audio: ai::Interface::default(),
            disk: di::Interface::default(),
            serial: si::Interface::default(),

            config,
            modules,
        };

        if system.config.ipl_lle {
            system.load_ipl();
        } else if system.config.sideload.is_some() {
            system.load_executable();
        } else if system.modules.disk.has_disk() {
            system.load_ipl_hle();
        } else {
            system.load_ipl();
        }

        system
    }

    /// Processes scheduled events.
    #[inline(always)]
    pub fn process_events(&mut self) {
        while let Some(event) = self.scheduler.pop() {
            let cycles_late = self.scheduler.elapsed() - event.cycle;
            let ctx = HandlerCtx {
                cycles_late: Cycles(cycles_late),
            };

            event.handler.call(self, ctx);
        }
    }
}
