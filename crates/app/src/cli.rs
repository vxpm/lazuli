use std::path::PathBuf;

use clap::{Args, Parser};

#[derive(Args, Debug)]
pub struct PpcjitConfig {
    /// Maximum number of instructions per block
    #[arg(visible_alias("ipb"), long, default_value_t = 128)]
    pub instr_per_block: u32,
    /// Whether to treat syscalls as no-ops
    #[arg(long, default_value_t = false)]
    pub nop_syscalls: bool,
    /// Whether to ignore the FPU enabled bit in MSR
    #[arg(long, default_value_t = false)]
    pub force_fpu: bool,
    /// Whether to ignore unimplemented instructions
    #[arg(long, default_value_t = false)]
    pub ignore_unimplemented_inst: bool,
    /// Whether to clear the JIT block cache
    #[arg(long, default_value_t = false)]
    pub clear_cache: bool,
    /// Whether to perform round-to-single operations
    #[arg(long, default_value_t = false)]
    pub round_to_single: bool,
}

/// Lazuli: GameCube emulator
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Config {
    #[command(flatten)]
    pub ppcjit: PpcjitConfig,
    /// Path to the IPL ROM
    #[arg(long)]
    pub ipl: Option<PathBuf>,
    /// Path to the ROM to load and execute
    ///
    /// Supported formats are .iso and .rvz. To sideload executables, use the `exec` argument.
    #[arg(short('i'), long)]
    pub rom: Option<PathBuf>,
    /// Path to the executable to sideload and execute
    ///
    /// Supported format is .dol.
    #[arg(long)]
    pub exec: Option<PathBuf>,
    /// Path to a file to use as a debug info provider
    ///
    /// Supported formats are .elf and .map.
    #[arg(long)]
    pub debug: Option<PathBuf>,
    /// Whether to actually perform EFB->RAM copies.
    #[arg(long, default_value_t = false)]
    pub efb_ram_copies: bool,
    /// Whether to LLE the IPL instead of HLEing it for loading games
    #[arg(long, default_value_t = false)]
    pub ipl_lle: bool,
    /// Whether to start running the emulator right away
    #[arg(short, long, default_value_t = false)]
    pub run: bool,
}
