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

/// How many DSP instructions to execute per cycle.
const DSP_INST_PER_CYCLE: f64 = 1.0;
/// How many DSP cycles to execute per step.
const DSP_STEP: u32 = 64;
/// How many DSP instructions to execute per step.
const DSP_INST_PER_STEP: u32 = (DSP_STEP as f64 * DSP_INST_PER_CYCLE) as u32;

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
        let mut total_executed = cores::Info::default();
        while total_executed.executed_cycles < cycles {
            // how many CPU cycles can we execute?
            let remaining = cycles - total_executed.executed_cycles;
            let until_next_dsp_step =
                Cycles((6.0 * ((DSP_STEP as f64) - self.dsp_pending)).ceil() as u64);
            let until_next_event = Cycles(self.sys.scheduler.until_next().unwrap_or(u64::MAX));
            let can_execute = until_next_dsp_step.min(until_next_event).min(remaining);

            // execute CPU
            let executed = self.cores.cpu.exec(&mut self.sys, can_execute, breakpoints);
            total_executed.executed_instructions += executed.executed_instructions;
            total_executed.executed_cycles += executed.executed_cycles;

            // execute DSP
            self.dsp_pending += executed.executed_cycles.to_dsp_cycles();
            while self.dsp_pending >= DSP_STEP as f64 {
                self.cores.dsp.exec(&mut self.sys, DSP_INST_PER_STEP);
                self.dsp_pending -= DSP_STEP as f64;
            }

            self.sys.scheduler.advance(executed.executed_cycles.0);
            self.sys.process_events();

            if executed.hit_breakpoint || breakpoints.contains(&self.sys.cpu.pc) {
                std::hint::cold_path();
                total_executed.hit_breakpoint = true;
                break;
            }
        }

        total_executed
    }

    pub fn step(&mut self) -> cores::Info {
        // execute CPU
        let executed = self.cores.cpu.step(&mut self.sys);
        self.dsp_pending += executed.executed_cycles.to_dsp_cycles();

        // execute DSP
        while self.dsp_pending >= DSP_STEP as f64 {
            self.cores.dsp.exec(&mut self.sys, DSP_INST_PER_STEP);
            self.dsp_pending -= DSP_STEP as f64;
        }

        // process events
        self.sys.scheduler.advance(executed.executed_cycles.0);
        self.sys.process_events();

        executed
    }
}
