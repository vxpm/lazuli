use std::ffi::c_void;
use std::ptr::NonNull;

use jitalloc::{Allocation, ReadExec};

use crate::Sequence;
use crate::hooks::Context;

#[derive(Debug)]
#[repr(C)]
pub struct LinkData {
    /// Linked block
    pub block: BlockFn,
    /// Information regarding the pattern of the linked block
    pub pattern: Pattern,
}

/// Information about block execution.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Info {
    /// How many instructions have been executed already. Updated on block exits only.
    pub instructions: u32,
    /// How many cycles have been executed already. Updated on block exits only.
    pub cycles: u32,
}

/// Information regarding a block's execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Executed {
    /// How many instructions were executed.
    pub instructions: u32,
    /// How many cycles were executed.
    pub cycles: u32,
}

/// A block pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Pattern {
    /// No known pattern.
    None = 0,
    /// A single instruction long block with a call.
    Call,
    /// Branching to self
    IdleBasic,
    /// Idling by reading from a fixed memory location on a loop
    IdleVolatileRead,
    /// Function which the status of the CPU->DSP mailbox and returns it.
    GetMailboxStatusFunc,
}

/// Meta information regarding a block.
#[derive(Debug, Clone)]
pub struct Meta {
    /// The sequence of instructions this block contains.
    pub seq: Sequence,
    /// The Cranelift IR of this block. Only available if `cfg!(debug_assertions)` is true.
    pub clir: Option<String>,
    /// The disassembly of this block. Only available if `cfg!(debug_assertions)` is true.
    pub disasm: Option<String>,
    /// How many cycles this block executes at most.
    pub cycles: u32,
    /// The pattern of this block.
    pub pattern: Pattern,
}

/// A handle representing a compiled block of PowerPC instructions. This struct does not manage the
/// memory behind the block.
///
/// In order to call the block, use [`Jit::call`](super::Jit::call).
pub struct Block {
    code: Allocation<ReadExec>,
    meta: Meta,
}

/// A opaque handle representing the function of a compiled [`Block`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct BlockFn(NonNull<c_void>);

impl Block {
    pub(crate) fn new(code: Allocation<ReadExec>, meta: Meta) -> Self {
        Self { code, meta }
    }

    /// Meta information regarding this block.
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Returns a pointer to the function of this block.
    pub fn as_ptr(&self) -> BlockFn {
        // SAFETY: the pointer isn't accessed by anything other than Jit::call
        BlockFn(unsafe { self.code.as_ptr().cast() })
    }
}

/// A trampoline that allows calling blocks produced by a [`Jit`](super::Jit) compiler.
pub(super) struct Trampoline(pub(super) Allocation<ReadExec>);

type TrampolineFn = extern "C-unwind" fn(*mut Info, *mut Context, BlockFn);

impl Trampoline {
    /// Calls the given block using this trampoline.
    ///
    /// # Safety
    /// The allocator used for this trampoline and the block must not be used while the block is
    /// being called (i.e. this function is being executed).
    pub unsafe fn call(&self, ctx: *mut Context, block: BlockFn) -> Info {
        let mut info = Info {
            instructions: 0,
            cycles: 0,
        };

        let trampoline: TrampolineFn = unsafe { std::mem::transmute(self.0.as_ptr().cast::<u8>()) };
        trampoline(&raw mut info, ctx, block);

        info
    }
}
