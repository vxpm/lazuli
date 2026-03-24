#![feature(deque_extend_front)]

pub mod primitive;
pub mod stream;

pub mod cores;
pub mod modules;

pub mod panic;
pub mod system;

pub use disks;
pub use gekko::{self, Address, Cycles};
pub use primitive::Primitive;

use crate::cores::Cores;
use crate::system::{Modules, System};

/// The Lazuli emulator.
pub struct Lazuli {
    /// System state.
    pub sys: System,
    /// Cores of the emulator.
    cores: Cores,
    /// How many DSP cycles are pending.
    dsp_pending: f64,
}

impl Lazuli {
    pub fn new(cores: Cores, modules: Modules, config: system::Config) -> Self {
        Self {
            sys: System::new(modules, config),
            cores,
            dsp_pending: 0.0,
        }
    }

    /// Advances emulation by the specified number of CPU cycles.
    pub fn exec(&mut self, cycles: Cycles, breakpoints: &[Address]) -> cores::Info {
        let mut info = cores::Info::default();
        while info.executed_cycles < cycles {
            // how many CPU cycles can we execute?
            let remaining = cycles - info.executed_cycles;

            // execute CPU
            let step_info = self.cores.cpu.exec(&mut self.sys, remaining, breakpoints);
            info.executed_instructions += step_info.executed_instructions;
            info.executed_cycles += step_info.executed_cycles;

            // process events
            self.sys.process_events();

            // execute DSP
            self.dsp_pending += step_info.executed_cycles.to_dsp_cycles();
            if self.dsp_pending >= 64.0 {
                let dsp_cycles = self.dsp_pending.floor();
                self.dsp_pending -= dsp_cycles;
                self.cores.dsp.exec(&mut self.sys, dsp_cycles as u32);
            }

            // process events
            self.sys.process_events();

            if step_info.hit_breakpoint || breakpoints.contains(&self.sys.cpu.pc) {
                std::hint::cold_path();
                info.hit_breakpoint = true;
                break;
            }
        }

        info
    }

    pub fn step(&mut self) -> cores::Info {
        // execute CPU
        let executed = self.cores.cpu.step(&mut self.sys);

        // execute DSP
        self.dsp_pending += executed.executed_cycles.to_dsp_cycles();
        if self.dsp_pending >= 64.0 {
            let dsp_cycles = self.dsp_pending.floor();
            self.dsp_pending -= dsp_cycles;
            self.cores.dsp.exec(&mut self.sys, dsp_cycles as u32);
        }

        executed
    }
}
