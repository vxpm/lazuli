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

pub struct VertexParser {
    code: Allocation<ReadExec>,
}

impl VertexParser {
    pub(crate) fn new(code: Allocation<ReadExec>) -> Self {
        Self { code }
    }

    pub(crate) fn as_ptr(&self) -> ParserFn {
        unsafe { std::mem::transmute(self.code.as_ptr().cast::<u8>()) }
    }
}
