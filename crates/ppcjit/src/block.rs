use std::ffi::c_void;
use std::ptr::NonNull;

use bitos::bitos;
use jitalloc::{Allocation, ReadExec};

use crate::Sequence;
use crate::hooks::Context;

/// Metadata regarding a branch exit.
#[bitos(35)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchMeta {
    /// Whether the target address is relative to the branch address.
    #[bits(0)]
    pub relative: bool,
    /// Whether the branch is conditional.
    #[bits(1)]
    pub conditional: bool,
    /// Whether the branch is indirect (i.e. not encoded directly in the branch instruction).
    #[bits(2)]
    pub indirect: bool,
    /// Address of the branch.
    #[bits(3..35)]
    pub address: u32,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    Sync   = 0b0,
    Branch = 0b1,
}

#[bitos(64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitReason {
    #[bits(0)]
    pub kind: ExitKind,
    #[bits(1..36)]
    pub branch: BranchMeta,
}

impl ExitReason {
    pub const SYNC: Self = Self(0);
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

type TrampolineFn = extern "C-unwind" fn(*mut Context, BlockFn);

impl Trampoline {
    /// Calls the given block using this trampoline.
    ///
    /// # Safety
    /// The allocator used for this trampoline and the block must not be used while the block is
    /// being called (i.e. this function is being executed).
    pub unsafe fn call(&self, ctx: *mut Context, block: BlockFn) {
        let trampoline: TrampolineFn = unsafe { std::mem::transmute(self.0.as_ptr().cast::<u8>()) };
        trampoline(ctx, block);
    }
}
