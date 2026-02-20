//! Arena allocator for JITs.
mod region;

use std::marker::PhantomData;
use std::ptr::NonNull;

#[cfg(target_family = "windows")]
use windows::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache, Threading::GetCurrentProcess,
};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    unsafe fn sys_icache_invalidate(start: *mut std::ffi::c_void, len: usize);
}

use crate::region::Region;

#[rustfmt::skip]
pub use crate::region::Protection;

/// # Safety considerations
/// The allocator this allocation comes from must not be modified while the allocation
/// is accessed. This is specially important for multi-threaded contexts.
pub struct Allocation<K>(NonNull<[u8]>, PhantomData<K>);

impl<K> Allocation<K> {
    /// Returns a pointer to the allocation.
    ///
    /// # Safety
    /// In order to access the data behind the pointer, accesses to the underlying allocator must
    /// be synchronized, as stated in the type docs. For more information, see
    /// [`Allocator::allocate`].
    #[inline(always)]
    pub unsafe fn as_ptr(&self) -> NonNull<[u8]> {
        self.0
    }
}

// SAFETY: safe to send to another thread as long as accesses to the allocation are synchronized
// with accesses to the allocator, which is the user's responsibility
unsafe impl<K> Send for Allocation<K> {}

/// Trait for kinds of allocations.
pub trait AllocKind {
    /// The protection of this kind of allocation.
    const PROTECTION: Protection;
}

/// Readable and executable allocation kind.
pub struct ReadExec;
impl AllocKind for ReadExec {
    const PROTECTION: Protection = Protection::ReadExec;
}

/// Readable and writable allocation kind.
pub struct ReadWrite;
impl AllocKind for ReadWrite {
    const PROTECTION: Protection = Protection::ReadWrite;
}

/// An arena allocator for data with the given protection kind `K`.
///
/// Allocations performed by this allocator are _never_ freed.
pub struct Allocator<K> {
    /// The currently active region
    current: Option<Region>,
    /// Offset into the current region
    offset: usize,
    /// Phantom
    _phantom: PhantomData<K>,
}

impl<K> Allocator<K>
where
    K: AllocKind,
{
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            current: None,
            offset: 0,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn current(&mut self, len: usize) -> Region {
        if let Some(region) = self.current {
            region
        } else {
            let region = Region::new(None, len);
            self.current = Some(region);
            region
        }
    }

    fn allocate_inner(&mut self, alignment: usize, length: usize) -> (Region, Allocation<K>) {
        assert!(length > 0);

        let alignment = alignment.max(1).next_power_of_two();
        let effective_offset = self.offset.next_multiple_of(alignment);

        let region = self.current(length);
        let remaining = region.len().checked_sub(effective_offset);

        if remaining.is_none_or(|r| r < length) {
            let end = unsafe { region.as_ptr().add(region.len()) };
            let region = Region::new(Some(end.addr()), length);
            self.current = Some(region);
            self.offset = 0;
            return self.allocate_inner(alignment, length);
        }

        let start = unsafe { region.as_ptr().add(effective_offset) };
        self.offset = effective_offset + length;

        (
            region,
            Allocation(
                NonNull::slice_from_raw_parts(NonNull::new(start.cast()).unwrap(), length),
                PhantomData,
            ),
        )
    }

    /// Same as [`Self::allocate`], but does not initialize the value. See it's docs for more info.
    pub fn allocate_uninit(&mut self, alignment: usize, length: usize) -> Allocation<K> {
        let (region, alloc) = self.allocate_inner(alignment, length);
        region.protect(self.offset, K::PROTECTION);

        alloc
    }

    /// Allocates a region of memory with the given `alignment` and initializes it with `data`.
    ///
    /// # Safety considerations
    /// While creating an allocation is safe, _accessing_ the allocations must _not_ be done while
    /// allocation takes place.
    ///
    /// This is because the memory protection of existing allocations might be temporarily modified
    /// during the process of allocating.
    ///
    /// This is enforced in [`Allocation`]'s `as_ptr` method as a safety requirement.
    pub fn allocate(&mut self, alignment: usize, data: &[u8]) -> Allocation<K> {
        let (region, alloc) = self.allocate_inner(alignment, data.len());
        region.protect(self.offset, Protection::ReadWrite);

        // SAFETY: the allocation is guaranteed to be `data.len()` bytes long and writable, since
        // we've protected it as `ReadWrite`. the pointers also do not overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), alloc.0.as_ptr().cast(), data.len())
        };

        if K::PROTECTION != Protection::ReadWrite {
            region.protect(self.offset, K::PROTECTION);
        }

        #[cfg(any(target_family = "windows", target_os = "macos"))]
        if K::PROTECTION == Protection::ReadExec {
            #[cfg(target_family = "windows")]
            unsafe {
                let process = GetCurrentProcess();
                FlushInstructionCache(process, Some(alloc.0.as_ptr().cast()), data.len()).unwrap();
            }

            #[cfg(target_os = "macos")]
            unsafe {
                sys_icache_invalidate(alloc.0.as_ptr().cast(), data.len());
            }
        }

        alloc
    }
}
