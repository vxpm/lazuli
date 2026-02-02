use std::ops::Range;

use indexmap::IndexSet;
use lazuli::Address;

use crate::cpu::jit::BlockId;
use crate::cpu::jit::table::Table as BaseTable;

const MAP_TBL_L0_BITS: usize = 12;
const MAP_TBL_L0_COUNT: usize = 1 << MAP_TBL_L0_BITS;
const MAP_TBL_L0_MASK: usize = MAP_TBL_L0_COUNT - 1;
const MAP_TBL_L1_BITS: usize = 8;
const MAP_TBL_L1_COUNT: usize = 1 << MAP_TBL_L1_BITS;
const MAP_TBL_L1_MASK: usize = MAP_TBL_L1_COUNT - 1;
const MAP_TBL_L2_BITS: usize = 10;
const MAP_TBL_L2_COUNT: usize = 1 << MAP_TBL_L2_BITS;
const MAP_TBL_L2_MASK: usize = MAP_TBL_L2_COUNT - 1;

const DEPS_PAGE_LEN: usize = 1 << 12;
const DEPS_TBL_L0_BITS: usize = 12;
const DEPS_TBL_L0_COUNT: usize = 1 << DEPS_TBL_L0_BITS;
const DEPS_TBL_L0_MASK: usize = DEPS_TBL_L0_COUNT - 1;
const DEPS_TBL_L1_BITS: usize = 8;
const DEPS_TBL_L1_COUNT: usize = 1 << DEPS_TBL_L1_BITS;
const DEPS_TBL_L1_MASK: usize = DEPS_TBL_L1_COUNT - 1;

#[inline(always)]
fn addr_to_mapping_idx(addr: Address) -> (usize, usize, usize) {
    let base = (addr.value() >> 2) as usize;
    (
        base >> (30 - MAP_TBL_L0_BITS) & MAP_TBL_L0_MASK,
        (base >> (30 - MAP_TBL_L0_BITS - MAP_TBL_L1_BITS)) & MAP_TBL_L1_MASK,
        (base >> (30 - MAP_TBL_L0_BITS - MAP_TBL_L1_BITS - MAP_TBL_L2_BITS)) & MAP_TBL_L2_MASK,
    )
}

#[inline(always)]
fn page_to_deps_idx(page: usize) -> (usize, usize) {
    (
        page >> (20 - DEPS_TBL_L0_BITS) & DEPS_TBL_L0_MASK,
        (page >> (20 - DEPS_TBL_L0_BITS - DEPS_TBL_L1_BITS)) & DEPS_TBL_L1_MASK,
    )
}

#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    pub id: BlockId,
    pub length: u32,
}

#[derive(Default)]
pub struct Table(
    BaseTable<BaseTable<BaseTable<Mapping, MAP_TBL_L2_COUNT>, MAP_TBL_L1_COUNT>, MAP_TBL_L0_COUNT>,
);

impl Table {
    pub fn insert(&mut self, addr: Address, mapping: Mapping) {
        let (idx0, idx1, idx2) = addr_to_mapping_idx(addr);
        let level1 = self.0.get_or_default(idx0);
        let level2 = level1.get_or_default(idx1);
        level2.insert(idx2, mapping);
    }

    pub fn remove(&mut self, addr: Address) -> Option<Mapping> {
        let (idx0, idx1, idx2) = addr_to_mapping_idx(addr);
        let level1 = self.0.get_mut(idx0)?;
        let level2 = level1.get_mut(idx1)?;
        level2.remove(idx2)
    }

    pub fn get(&self, addr: Address) -> Option<&Mapping> {
        let (idx0, idx1, idx2) = addr_to_mapping_idx(addr);
        let level1 = self.0.get(idx0)?;
        let level2 = level1.get(idx1)?;
        level2.get(idx2)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Default)]
pub struct DepsTable(BaseTable<BaseTable<IndexSet<Address>, DEPS_TBL_L1_COUNT>, DEPS_TBL_L0_COUNT>);

impl DepsTable {
    /// Marks address `addr` as dependent on the pages that cover the given range.
    pub fn mark(&mut self, addr: Address, range: Range<Address>) {
        let start_page = range.start.value() as usize / DEPS_PAGE_LEN;
        let end_page = range.end.value() as usize / DEPS_PAGE_LEN;

        for page in start_page..=end_page {
            let (idx0, idx1) = page_to_deps_idx(page);
            let level1 = self.0.get_or_default(idx0);
            let deps = level1.get_or_default(idx1);
            deps.insert(addr);
        }
    }

    /// Unmarks address `addr` as dependent on the pages that cover the given range.
    pub fn unmark(&mut self, addr: Address, range: Range<Address>) {
        let start_page = range.start.value() as usize / DEPS_PAGE_LEN;
        let end_page = range.end.value() as usize / DEPS_PAGE_LEN;

        for page in start_page..=end_page {
            let (idx0, idx1) = page_to_deps_idx(page);
            let level1 = self.0.get_or_default(idx0);
            let deps = level1.get_or_default(idx1);
            deps.swap_remove(&addr);
        }
    }

    /// Returns the set of dependencies of the page that contains the given address.
    pub fn get(&self, addr: Address) -> Option<&IndexSet<Address>> {
        let page = addr.value() as usize / DEPS_PAGE_LEN;
        let (idx0, idx1) = page_to_deps_idx(page);
        let level1 = self.0.get(idx0)?;
        level1.get(idx1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
