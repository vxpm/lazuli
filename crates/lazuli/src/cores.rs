use gekko::{Address, Cycles};

use crate::system::System;

#[derive(Default, Clone, Copy)]
pub struct Info {
    /// How many cycles have been executed.
    pub executed_cycles: Cycles,
    /// How many instructions have been executed.
    pub executed_instructions: u32,
    /// Whether a breakpoint was hit.
    pub hit_breakpoint: bool,
}

/// Trait for CPU cores.
pub trait CpuCore: Send {
    /// Drives the CPU core forward by approximatedly the given number of `cycles`, stopping at any
    /// address in `breakpoints` or whenever a scheduler event is pending.
    fn exec(&mut self, sys: &mut System, cycles: Cycles, breakpoints: &[Address]) -> Info;
    /// Steps the CPU, i.e. runs exactly 1 instruction.
    fn step(&mut self, sys: &mut System) -> Info;
}

/// Trait for DSP cores.
pub trait DspCore: Send {
    /// Drives the DSP core forward by _at most_ the specified amount of instructions. The actual
    /// number of instructions executed is returned.
    fn exec(&mut self, sys: &mut System, instructions: u32) -> u32;
}

/// Cores that emulate system components.
pub struct Cores {
    pub cpu: Box<dyn CpuCore>,
    pub dsp: Box<dyn DspCore>,
}
