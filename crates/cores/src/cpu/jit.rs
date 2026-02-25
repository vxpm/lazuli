mod icache;
mod mapping;
mod table;

use indexmap::IndexSet;
use lazuli::cores::{CpuCore, Executed};
use lazuli::gekko::{self, Cpu, DEQUANTIZATION_LUT, QUANTIZATION_LUT, QuantReg, QuantizedType};
use lazuli::system::{self, System};
use lazuli::{Address, Cycles, Primitive};
use mapping::Mapping;
use ppcjit::block::{BlockFn, Info, LinkData, Pattern};
use ppcjit::hooks::*;
use ppcjit::{Block, FastmemLut};

#[rustfmt::skip]
pub use ppcjit;

/// Identifier for a block in a [`Blocks`] storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockId(usize);

pub struct StoredBlock {
    pub inner: Block,
    pub links: Vec<*mut Option<LinkData>>,
}

// TODO: this is problematic
unsafe impl Send for StoredBlock {}

/// A structure which keeps tracks of compiled [`Block`]s.
pub struct Blocks {
    storage: Vec<StoredBlock>,
    logical_mappings: mapping::Table,
    physical_mappings: mapping::Table,
    logical_deps: mapping::DepsTable,
    physical_deps: mapping::DepsTable,
    temp_deps: IndexSet<Address>,
}

impl Default for Blocks {
    fn default() -> Self {
        Self {
            storage: Default::default(),
            logical_mappings: Default::default(),
            physical_mappings: Default::default(),
            logical_deps: Default::default(),
            physical_deps: Default::default(),
            temp_deps: IndexSet::new(),
        }
    }
}

struct MappingNotFoundError;

impl Blocks {
    fn insert_mapping(&mut self, logical: bool, addr: Address, mapping: Mapping) {
        let (mappings, deps) = if logical {
            (&mut self.logical_mappings, &mut self.logical_deps)
        } else {
            (&mut self.physical_mappings, &mut self.physical_deps)
        };

        mappings.insert(addr, mapping);

        let start = addr;
        let end = addr + mapping.length;
        deps.mark(addr, start..end);
    }

    fn remove_mapping_if_contains(
        &mut self,
        logical: bool,
        addr: Address,
        target: Address,
    ) -> Result<Option<Mapping>, MappingNotFoundError> {
        let (mappings, deps) = if logical {
            (&mut self.logical_mappings, &mut self.logical_deps)
        } else {
            (&mut self.physical_mappings, &mut self.physical_deps)
        };

        let mapping = mappings.get(addr).ok_or(MappingNotFoundError)?;

        let start = addr;
        let end = addr + mapping.length;
        let range = start..end;

        if range.contains(&target) {
            deps.unmark(addr, range);
            Ok(mappings.remove(addr))
        } else {
            Ok(None)
        }
    }

    /// Inserts a block into the storage and maps it to the given address.
    #[inline(always)]
    pub fn insert(&mut self, logical: bool, addr: Address, block: Block) -> BlockId {
        let length = 4 * block.meta().seq.len() as u32;
        let id = BlockId(self.storage.len());

        self.storage.push(StoredBlock {
            inner: block,
            links: Vec::new(),
        });

        self.insert_mapping(logical, addr, Mapping { id, length });

        id
    }

    /// Returns the mapping at `addr`.
    #[inline(always)]
    pub fn get_mapping(&self, logical: bool, addr: Address) -> Option<Mapping> {
        let mappings = if logical {
            &self.logical_mappings
        } else {
            &self.physical_mappings
        };

        mappings.get(addr).copied()
    }

    /// Returns the block mapped to `addr`.
    #[inline(always)]
    pub fn get(&mut self, logical: bool, addr: Address) -> Option<&StoredBlock> {
        self.storage.get(self.get_mapping(logical, addr)?.id.0)
    }

    /// Invalidate mappings that contain `addr`.
    pub fn invalidate(&mut self, logical: bool, target: Address) {
        let deps = if logical {
            &mut self.logical_deps
        } else {
            &mut self.physical_deps
        };

        let Some(deps) = deps.get(target) else { return };
        if deps.is_empty() {
            return;
        }

        let mut temp_deps = std::mem::replace(&mut self.temp_deps, IndexSet::new());
        deps.clone_into(&mut temp_deps);

        for dep in temp_deps.iter() {
            let Ok(mapping) = self.remove_mapping_if_contains(logical, *dep, target) else {
                panic!("mapping {dep} is listed as dependent on a page but it does not exist");
            };

            let Some(mapping) = mapping else {
                continue;
            };

            let block = &mut self.storage[mapping.id.0];
            for link in block.links.drain(..) {
                let link = unsafe { link.as_mut().unwrap() };
                *link = None;
            }
        }

        temp_deps.clear();
        self.temp_deps = temp_deps;
    }

    /// Clears all mappings.
    pub fn clear(&mut self) {
        self.logical_mappings.clear();
        self.physical_mappings.clear();
        self.logical_deps.clear();
        self.physical_deps.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitReason {
    None,
    IdleLooping,
}

/// Context to be passed in for execution of JIT blocks.
struct Context<'a> {
    /// The system state, so that the JIT block can operate on it.
    sys: &'a mut System,
    /// The block mapping, so that write operations can invalidate blocks.
    blocks: &'a mut Blocks,
    /// ICache
    icache: &'a mut icache::Cache,
    /// Amount of cycles we are trying to execute.
    target_cycles: u32,
    /// Maximum instructions we should execute.
    max_instructions: u32,
    /// Whether to forcely disable following links.
    force_no_link: bool,
    /// Last followed link.
    last_followed_link: Option<BlockFn>,
    /// Reason for exit.
    exit_reason: ExitReason,
}

const CTX_HOOKS: Hooks = {
    extern "C-unwind" fn get_registers<'a>(ctx: &'a mut Context) -> &'a mut Cpu {
        &mut ctx.sys.cpu
    }

    extern "C-unwind" fn get_fastmem<'a>(ctx: &'a mut Context) -> &'a FastmemLut {
        if ctx.sys.cpu.supervisor.config.msr.data_addr_translation() {
            ctx.sys.mem.data_fastmem_lut_logical()
        } else {
            ctx.sys.mem.data_fastmem_lut_physical()
        }
    }

    extern "C-unwind" fn follow_link(
        info: &Info,
        ctx: &mut Context,
        link_data: &mut Option<LinkData>,
    ) -> bool {
        // if we have reached cycle or instruction limit, don't follow links, just exit.
        if ctx.force_no_link
            || info.cycles >= ctx.target_cycles
            || info.instructions >= ctx.max_instructions
        {
            ctx.last_followed_link = None;
            return false;
        }

        let Some(link_data) = link_data else {
            return true;
        };

        // otherwise, detect whether we are idle looping and exit too
        let follow = match link_data.pattern {
            Pattern::IdleBasic | Pattern::IdleVolatileRead => {
                if ctx.last_followed_link == Some(link_data.block) {
                    ctx.exit_reason = ExitReason::IdleLooping;
                    false
                } else {
                    true
                }
            }
            _ => true,
        };

        // if not idle looping, then sure, follow link
        ctx.last_followed_link = Some(link_data.block);
        follow
    }

    extern "C-unwind" fn try_link(
        ctx: &mut Context,
        addr: Address,
        link_data: &mut Option<LinkData>,
    ) {
        debug_assert!(link_data.is_none());
        let logical = ctx.sys.cpu.supervisor.config.msr.instr_addr_translation();
        if let Some(mapping) = ctx.blocks.get_mapping(logical, addr) {
            let stored = ctx.blocks.storage.get_mut(mapping.id.0).unwrap();
            *link_data = Some(LinkData {
                block: stored.inner.as_ptr(),
                pattern: stored.inner.meta().pattern,
            });

            stored.links.push(&raw mut *link_data);
        }
    }

    extern "C-unwind" fn read<P: Primitive>(
        ctx: &mut Context,
        addr: Address,
        value: &mut P,
    ) -> bool {
        if let Some(read) = ctx.sys.read_slow(addr) {
            *value = read;
            true
        } else {
            std::hint::cold_path();
            tracing::error!(pc = ?ctx.sys.cpu.pc, "failed to translate address {addr}");
            false
        }
    }

    extern "C-unwind" fn write<P: Primitive>(ctx: &mut Context, addr: Address, value: P) -> bool {
        if ctx.sys.write_slow(addr, value) {
            true
        } else {
            std::hint::cold_path();
            tracing::error!(pc = ?ctx.sys.cpu.pc, "failed to translate address {addr}");
            false
        }
    }

    extern "C-unwind" fn read_quantized(
        ctx: &mut Context,
        addr: Address,
        gqr: QuantReg,
        value: &mut f64,
    ) -> u8 {
        let ty = gqr.load_type();
        let scale = if ty != QuantizedType::Float {
            gqr.load_scale().value()
        } else {
            0
        };

        let read = match ty {
            QuantizedType::U8 => ctx.sys.read::<u8>(addr).map(|x| x as f64),
            QuantizedType::U16 => ctx.sys.read::<u16>(addr).map(|x| x as f64),
            QuantizedType::I8 => ctx.sys.read::<i8>(addr).map(|x| x as f64),
            QuantizedType::I16 => ctx.sys.read::<i16>(addr).map(|x| x as f64),
            _ => ctx.sys.read::<u32>(addr).map(|x| f32::from_bits(x) as f64),
        };

        let Some(read) = read else {
            std::hint::cold_path();
            tracing::error!("failed to translate address {addr}");
            return 0;
        };

        let scaled = read * DEQUANTIZATION_LUT[(scale as usize) & 0b0011_1111];
        *value = scaled;

        ty.size()
    }

    extern "C-unwind" fn write_quantized(
        ctx: &mut Context,
        addr: Address,
        gqr: QuantReg,
        value: f64,
    ) -> u8 {
        let ty = gqr.store_type();
        let scale = if ty != QuantizedType::Float {
            gqr.store_scale().value()
        } else {
            0
        };

        let scaled = value * QUANTIZATION_LUT[(scale as usize) & 0b0011_1111];
        let success = match ty {
            QuantizedType::U8 => ctx.sys.write(addr, scaled as u8),
            QuantizedType::U16 => ctx.sys.write(addr, scaled as u16),
            QuantizedType::I8 => ctx.sys.write(addr, scaled as i8),
            QuantizedType::I16 => ctx.sys.write(addr, scaled as i16),
            _ => ctx.sys.write(addr, (scaled as f32).to_bits()),
        };

        if !success {
            std::hint::cold_path();
            tracing::error!("failed to translate address {addr}");
            return 0;
        }

        ty.size()
    }

    extern "C-unwind" fn invalidate_icache(ctx: &mut Context, addr: Address) {
        let cacheline_base = addr.align_down(32);
        let is_logical = ctx.sys.cpu.supervisor.config.msr.instr_addr_translation();

        if is_logical {
            for offset in 0..32 {
                let logical = cacheline_base + offset;
                let physical = ctx.sys.translate_inst_addr(logical);

                ctx.blocks.invalidate(true, logical);
                if let Some(physical) = physical {
                    ctx.blocks.invalidate(false, physical);
                }
            }

            if let Some(physical) = ctx.sys.translate_inst_addr(cacheline_base) {
                ctx.icache.invalidate(physical);
            }
        } else {
            for offset in 0..32 {
                let physical = cacheline_base + offset;
                ctx.blocks.invalidate(false, physical);
            }

            ctx.icache.invalidate(cacheline_base);
        }
    }

    extern "C-unwind" fn clear_icache(ctx: &mut Context) {
        ctx.icache.clear();
    }

    extern "C-unwind" fn dcache_dma(ctx: &mut Context) {
        let dma = ctx.sys.cpu.supervisor.config.dma.clone();

        if dma.lower.trigger() {
            let regions = ctx.sys.mem.regions();
            let ram =
                &mut regions.ram[dma.mem_address().value() as usize..][..dma.length() as usize];
            let l2c = &mut regions.l2c[dma.cache_address().value() as usize - 0xE000_0000..]
                [..dma.length() as usize];

            debug_assert!(dma.length() <= 4096);

            match dma.lower.direction() {
                gekko::DmaDirection::FromCacheToRam => {
                    ram.copy_from_slice(l2c);
                }
                gekko::DmaDirection::FromRamToCache => {
                    l2c.copy_from_slice(ram);
                }
            }
        }

        ctx.sys.cpu.supervisor.config.dma.lower.set_trigger(false);
        ctx.sys.cpu.supervisor.config.dma.lower.set_flush(false);
    }

    extern "C-unwind" fn msr_changed(ctx: &mut Context) {
        ctx.sys.scheduler.schedule_now(system::pi::check_interrupts);
    }

    extern "C-unwind" fn ibat_changed(ctx: &mut Context) {
        tracing::info!("ibats changed - clearing blocks mapping and rebuilding ibat lut");
        ctx.blocks.clear();
        ctx.sys
            .mem
            .build_inst_bat_lut(&ctx.sys.cpu.supervisor.memory.ibat);
    }

    extern "C-unwind" fn dbat_changed(ctx: &mut Context) {
        tracing::info!("dbats changed - rebuilding dbat lut");
        ctx.sys
            .mem
            .build_data_bat_lut(&ctx.sys.cpu.supervisor.memory.dbat);
    }

    extern "C-unwind" fn dec_read(ctx: &mut Context) {
        ctx.sys.update_decrementer();
    }

    extern "C-unwind" fn dec_changed(ctx: &mut Context) {
        ctx.sys.lazy.last_updated_dec = ctx.sys.scheduler.elapsed_time_base();
        ctx.sys.scheduler.cancel(System::decrementer_overflow);

        let dec = ctx.sys.cpu.supervisor.misc.dec;
        tracing::trace!("decrementer changed to {dec}");

        ctx.sys
            .scheduler
            .schedule(dec as u64 * 12, System::decrementer_overflow);
    }

    extern "C-unwind" fn tb_read(ctx: &mut Context) {
        ctx.sys.update_time_base();
    }

    extern "C-unwind" fn tb_changed(ctx: &mut Context) {
        ctx.sys.lazy.last_updated_tb = ctx.sys.scheduler.elapsed_time_base();
        tracing::info!("time base changed to {}", ctx.sys.cpu.supervisor.misc.tb);
    }

    #[expect(
        clippy::missing_transmute_annotations,
        reason = "unnecessary - the definitions are above"
    )]
    unsafe {
        use std::mem::transmute;

        let get_registers =
            transmute::<_, GetRegistersHook>(get_registers as extern "C-unwind" fn(_) -> _);
        let get_fastmem =
            transmute::<_, GetFastmemHook>(get_fastmem as extern "C-unwind" fn(_) -> _);

        let follow_link =
            transmute::<_, FollowLinkHook>(follow_link as extern "C-unwind" fn(_, _, _) -> _);
        let try_link = transmute::<_, TryLinkHook>(try_link as extern "C-unwind" fn(_, _, _));

        let read_i8 =
            transmute::<_, ReadHook<i8>>(read::<i8> as extern "C-unwind" fn(_, _, _) -> _);
        let write_i8 =
            transmute::<_, WriteHook<i8>>(write::<i8> as extern "C-unwind" fn(_, _, _) -> _);
        let read_i16 =
            transmute::<_, ReadHook<i16>>(read::<i16> as extern "C-unwind" fn(_, _, _) -> _);
        let write_i16 =
            transmute::<_, WriteHook<i16>>(write::<i16> as extern "C-unwind" fn(_, _, _) -> _);
        let read_i32 =
            transmute::<_, ReadHook<i32>>(read::<i32> as extern "C-unwind" fn(_, _, _) -> _);
        let write_i32 =
            transmute::<_, WriteHook<i32>>(write::<i32> as extern "C-unwind" fn(_, _, _) -> _);
        let read_i64 =
            transmute::<_, ReadHook<i64>>(read::<i64> as extern "C-unwind" fn(_, _, _) -> _);
        let write_i64 =
            transmute::<_, WriteHook<i64>>(write::<i64> as extern "C-unwind" fn(_, _, _) -> _);
        let read_quantized = transmute::<_, ReadQuantizedHook>(
            read_quantized as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        let write_quantized = transmute::<_, WriteQuantizedHook>(
            write_quantized as extern "C-unwind" fn(_, _, _, _) -> _,
        );

        let invalidate_icache =
            transmute::<_, InvalidateICache>(invalidate_icache as extern "C-unwind" fn(_, _));
        let clear_icache = transmute::<_, GenericHook>(clear_icache as extern "C-unwind" fn(_));
        let dcache_dma = transmute::<_, GenericHook>(dcache_dma as extern "C-unwind" fn(_));

        let msr_changed = transmute::<_, GenericHook>(msr_changed as extern "C-unwind" fn(_));

        let ibat_changed = transmute::<_, GenericHook>(ibat_changed as extern "C-unwind" fn(_));
        let dbat_changed = transmute::<_, GenericHook>(dbat_changed as extern "C-unwind" fn(_));

        let tb_read = transmute::<_, GenericHook>(tb_read as extern "C-unwind" fn(_));
        let tb_changed = transmute::<_, GenericHook>(tb_changed as extern "C-unwind" fn(_));

        let dec_read = transmute::<_, GenericHook>(dec_read as extern "C-unwind" fn(_));
        let dec_changed = transmute::<_, GenericHook>(dec_changed as extern "C-unwind" fn(_));

        Hooks {
            get_registers,
            get_fastmem,

            follow_link,
            try_link,

            read_i8,
            write_i8,
            read_i16,
            write_i16,
            read_i32,
            write_i32,
            read_i64,
            write_i64,
            read_quantized,
            write_quantized,

            invalidate_icache,
            clear_icache,
            dcache_dma,

            msr_changed,

            ibat_changed,
            dbat_changed,

            tb_read,
            tb_changed,

            dec_read,
            dec_changed,
        }
    }
};

/// JIT configuration.
pub struct Config {
    /// Maximum number of instructions per JIT block.
    pub instr_per_block: u32,
    /// Code generation settings.
    pub jit_settings: ppcjit::Settings,
}

pub struct Core {
    pub config: Config,
    pub compiler: ppcjit::Jit,
    pub blocks: Blocks,
    pub icache: icache::Cache,
}

fn closest_breakpoint(pc: Address, breakpoints: &[Address]) -> Address {
    let mut closest_breakpoint = Address(pc.value().saturating_add(u32::MAX));
    let mut closest_distance = closest_breakpoint.value() - pc.value();
    for breakpoint in breakpoints.iter().copied() {
        let distance = breakpoint.value().checked_sub(pc.value());
        if let Some(distance) = distance
            && distance <= closest_distance
            && distance != 0
        {
            closest_breakpoint = breakpoint;
            closest_distance = distance;
        }
    }

    closest_breakpoint
}

impl Core {
    pub fn new(config: Config) -> Self {
        let compiler = ppcjit::Jit::new(config.jit_settings.clone(), CTX_HOOKS);

        Self {
            config,
            compiler,
            blocks: Blocks::default(),
            icache: Default::default(),
        }
    }

    /// Compiles a sequence of at most `limit` instructions starting at `addr` into a JIT block.
    fn compile(&mut self, sys: &mut System, addr: Address, limit: u32) -> ppcjit::Block {
        let _span = tracing::trace_span!("compiling new block", addr = ?sys.cpu.pc).entered();

        let mut count = 0;
        let instructions = std::iter::from_fn(|| {
            if count >= limit {
                return None;
            }

            let current = addr + 4 * count;
            let Some(physical) = sys.translate_inst_addr(current) else {
                tracing::error!("failed to translate {current} at {}", addr);
                return None;
            };

            let ins = self.icache.get(sys, physical);
            count += 1;

            Some(ins)
        });

        let block = match self.compiler.build(instructions) {
            Ok(b) => b,
            Err(e) => match e {
                ppcjit::BuildError::EmptyBlock => panic!("built empty block at pc {}", sys.cpu.pc),
                ppcjit::BuildError::Builder { source } => panic!("block builder error: {}", source),
                ppcjit::BuildError::Codegen {
                    source,
                    sequence,
                    clir,
                } => {
                    panic!(
                        "block codegen error:\n{}\n{}\n{:?}",
                        sequence,
                        clir.as_deref().unwrap_or("<missing clir>"),
                        source,
                    )
                }
            },
        };

        tracing::trace!(
            instructions = block.meta().seq.len(),
            "block sequence built"
        );

        block
    }

    #[inline(always)]
    fn uncached_exec(
        &mut self,
        sys: &mut System,
        target_cycles: u32,
        max_instructions: u32,
        force_no_link: bool,
    ) -> Executed {
        let logical = sys.cpu.supervisor.config.msr.instr_addr_translation();
        let stored = self
            .blocks
            .get(logical, sys.cpu.pc)
            .filter(|b| b.inner.meta().seq.len() <= max_instructions as usize);

        let compiled: ppcjit::Block;
        let block = match stored {
            Some(stored) => stored.inner.as_ptr(),
            None => {
                std::hint::cold_path();

                compiled = self.compile(sys, sys.cpu.pc, max_instructions);
                compiled.as_ptr()
            }
        };

        let mut ctx = Context {
            sys,
            blocks: &mut self.blocks,
            icache: &mut self.icache,
            target_cycles,
            max_instructions,
            force_no_link,

            last_followed_link: None,
            exit_reason: ExitReason::None,
        };

        let info = unsafe {
            self.compiler
                .call(&raw mut ctx as *mut ppcjit::hooks::Context, block)
        };

        let cycles = if ctx.exit_reason == ExitReason::IdleLooping {
            std::hint::cold_path();
            Cycles(target_cycles as u64)
        } else {
            Cycles(info.cycles as u64)
        };

        Executed {
            instructions: info.instructions,
            cycles,
            hit_breakpoint: false,
        }
    }

    fn cached_exec(
        &mut self,
        sys: &mut System,
        target_cycles: u32,
        max_instructions: u32,
        force_no_link: bool,
    ) -> Executed {
        let logical = sys.cpu.supervisor.config.msr.instr_addr_translation();
        let block = self
            .blocks
            .get(logical, sys.cpu.pc)
            .filter(|b| b.inner.meta().seq.len() <= max_instructions as usize);

        if block.is_none() {
            // avoid trying to compile unimplemented instructions in debug mode
            let instructions = if cfg!(debug_assertions) {
                self.config.instr_per_block.min(max_instructions)
            } else {
                self.config.instr_per_block
            };

            let block = self.compile(sys, sys.cpu.pc, instructions);
            self.blocks.insert(logical, sys.cpu.pc, block);
        }

        self.uncached_exec(sys, target_cycles, max_instructions, force_no_link)
    }

    fn exec_inner<const BREAKPOINTS: bool>(
        &mut self,
        sys: &mut System,
        cycles: Cycles,
        breakpoints: &[Address],
    ) -> Executed {
        let mut executed = Executed::default();
        while executed.cycles < cycles {
            // detect mailbox idle loop
            let logical = sys.cpu.supervisor.config.msr.instr_addr_translation();
            if let Some(stored) = self.blocks.get(logical, sys.cpu.pc)
                && stored.inner.meta().pattern == Pattern::Call
                && let Some(dest) = stored.inner.meta().seq.is_call(sys.cpu.pc)
            {
                std::hint::cold_path();

                if let Some(func_block) = self.blocks.get(logical, dest)
                    && func_block.inner.meta().pattern == Pattern::GetMailboxStatusFunc
                    && sys.dsp.cpu_mailbox.status()
                {
                    std::hint::cold_path();
                    executed.cycles = cycles;
                    executed.instructions = 1;
                    break;
                }
            }

            let max_instructions = if BREAKPOINTS {
                let closest_breakpoint = closest_breakpoint(sys.cpu.pc, breakpoints);
                (closest_breakpoint.value() - sys.cpu.pc.value()) / 4
            } else {
                u32::MAX
            };

            // execute
            let target_cycles = cycles - executed.cycles;
            let e = self.cached_exec(sys, target_cycles.0 as u32, max_instructions, BREAKPOINTS);
            executed.instructions += e.instructions;
            executed.cycles += e.cycles;

            if BREAKPOINTS && breakpoints.contains(&sys.cpu.pc) {
                executed.hit_breakpoint = true;
                break;
            }
        }

        executed
    }
}

impl CpuCore for Core {
    fn exec(&mut self, sys: &mut System, cycles: Cycles, breakpoints: &[Address]) -> Executed {
        if breakpoints.is_empty() {
            self.exec_inner::<false>(sys, cycles, &[])
        } else {
            self.exec_inner::<true>(sys, cycles, breakpoints)
        }
    }

    fn step(&mut self, sys: &mut System) -> Executed {
        self.uncached_exec(sys, u32::MAX, 1, true)
    }
}
