use std::alloc::Layout;

use jitalloc::{Allocation, Allocator, ReadExec, ReadWrite};

/// A module that contains code and data allocations.
pub struct Module {
    code_allocator: Allocator<ReadExec>,
    data_allocator: Allocator<ReadWrite>,
}

impl Module {
    pub fn new() -> Self {
        Self {
            code_allocator: Allocator::new(),
            data_allocator: Allocator::new(),
        }
    }

    pub fn allocate_code(&mut self, code: &[u8]) -> Allocation<ReadExec> {
        self.code_allocator.allocate(64, code)
    }

    pub fn allocate_data(&mut self, layout: Layout) -> Allocation<ReadWrite> {
        self.data_allocator
            .allocate_uninit(layout.align(), layout.size())
    }
}
