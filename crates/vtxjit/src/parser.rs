#![cfg_attr(not(test), expect(unused, reason = "meta is only used in tests"))]

use jitalloc::{Allocation, ReadExec};
use lazuli::system::gx::cmd::attributes::VertexAttributeTable;
use lazuli::system::gx::cmd::{Arrays, VertexDescriptor};
use lazuli::system::gx::{MatrixSet, Vertex};

use crate::UnpackedDefaultMatrices;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Config {
    pub vcd: VertexDescriptor,
    pub vat: VertexAttributeTable,
}

impl Config {
    #[inline(always)]
    pub fn canonicalize(self) -> Self {
        // TODO: canonicalize
        self
    }
}

// ram, arrays, default matrices, data, vertices, matrix map, count
pub type ParserFn = extern "sysv64" fn(
    *const u8,
    *const Arrays,
    *const UnpackedDefaultMatrices,
    *const u8,
    *mut Vertex,
    *mut MatrixSet,
    u32,
);

/// Meta information regarding a parser.
#[derive(Debug, Clone)]
pub struct Meta {
    /// The Cranelift IR of this block. Only available if `cfg!(debug_assertions)` is true.
    pub clir: Option<String>,
    /// The disassembly of this block. Only available if `cfg!(debug_assertions)` is true.
    pub disasm: Option<String>,
}

/// A vertex stream parser.
pub struct VertexParser {
    code: Allocation<ReadExec>,
    meta: Meta,
}

impl VertexParser {
    pub fn new(code: Allocation<ReadExec>, meta: Meta) -> Self {
        Self { code, meta }
    }

    pub fn as_ptr(&self) -> ParserFn {
        unsafe { std::mem::transmute(self.code.as_ptr().cast::<u8>()) }
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }
}
