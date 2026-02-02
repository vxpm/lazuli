use lazuli::Address;
use lazuli::gekko::disasm::{Extensions, Ins};
use lazuli::system::System;

use crate::cpu::jit::table::Table;

const ICACHE_L0_BITS: usize = 8;
const ICACHE_L0_COUNT: usize = 1 << ICACHE_L0_BITS;
const ICACHE_L0_MASK: usize = ICACHE_L0_COUNT - 1;
const ICACHE_L1_BITS: usize = 11;
const ICACHE_L1_COUNT: usize = 1 << ICACHE_L1_BITS;
const ICACHE_L1_MASK: usize = ICACHE_L1_COUNT - 1;
const ICACHE_L2_BITS: usize = 8;
const ICACHE_L2_COUNT: usize = 1 << ICACHE_L2_BITS;
const ICACHE_L2_MASK: usize = ICACHE_L2_COUNT - 1;

type CacheLine = [u32; 8];

#[inline(always)]
fn addr_to_icache_idx(addr: Address) -> (usize, usize, usize) {
    let base = (addr.value() >> 5) as usize;
    (
        base >> (27 - ICACHE_L0_BITS) & ICACHE_L0_MASK,
        (base >> (27 - ICACHE_L0_BITS - ICACHE_L1_BITS)) & ICACHE_L1_MASK,
        (base >> (27 - ICACHE_L0_BITS - ICACHE_L1_BITS - ICACHE_L2_BITS)) & ICACHE_L2_MASK,
    )
}

#[derive(Default)]
pub struct Cache(Table<Table<Table<CacheLine, ICACHE_L2_COUNT>, ICACHE_L1_COUNT>, ICACHE_L0_COUNT>);

impl Cache {
    pub fn get(&mut self, sys: &mut System, physical: Address) -> Ins {
        let (idx0, idx1, idx2) = addr_to_icache_idx(physical);
        let level1 = self.0.get_or_default(idx0);
        let level2 = level1.get_or_default(idx1);
        let cacheline = match level2.get(idx2) {
            Some(cacheline) => cacheline,
            None => {
                let base = physical.align_down(32);

                let mut cacheline = [0; 8];
                for (index, word) in cacheline.iter_mut().enumerate() {
                    *word = sys.read_phys_slow::<u32>(base + 4 * index as u32);
                }

                level2.insert(idx2, cacheline)
            }
        };

        let offset = (physical.value() % 32) / 4;
        Ins::new(cacheline[offset as usize], Extensions::gekko_broadway())
    }

    pub fn invalidate(&mut self, physical: Address) {
        let (idx0, idx1, idx2) = addr_to_icache_idx(physical);
        let Some(level1) = self.0.get_mut(idx0) else {
            return;
        };
        let Some(level2) = level1.get_mut(idx1) else {
            return;
        };

        level2.remove(idx2);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
