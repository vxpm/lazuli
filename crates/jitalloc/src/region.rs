#[cfg(target_family = "unix")]
use rustix::mm::{self as mman, MapFlags, ProtFlags};
#[cfg(target_family = "windows")]
use windows::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache, Memory, Threading::GetCurrentProcess,
};

// TODO: don't assume 4 KiB pages
const PAGE_SIZE: usize = 4 * bytesize::KIB as usize;
const REGION_MIN_LEN: usize = 128 * bytesize::KIB as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    ReadExec,
    ReadWrite,
}

/// A memory mapped region.
#[derive(Clone, Copy)]
pub struct Region {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: changing the protection can be done from any thread
unsafe impl Send for Region {}

impl Region {
    /// Creates a new memory mapped region.
    pub fn new(addr_hint: Option<usize>, len: usize) -> Self {
        let addr_hint = addr_hint.map(|a| a.next_multiple_of(PAGE_SIZE));
        let len = len.max(REGION_MIN_LEN);

        // SAFETY: the pointer is aligned to page size (as checked previously) and it has no
        // provenance
        #[cfg(target_family = "unix")]
        let region = unsafe {
            mman::mmap_anonymous(
                addr_hint
                    .map(std::ptr::without_provenance_mut)
                    .unwrap_or_default(),
                len,
                ProtFlags::empty(),
                MapFlags::PRIVATE,
            )
        }
        .unwrap();

        #[cfg(target_family = "windows")]
        let region = unsafe {
            let addr_hint_ptr = addr_hint.map(|addr| std::ptr::without_provenance(addr));
            let result = Memory::VirtualAlloc(
                addr_hint_ptr,
                len,
                Memory::MEM_RESERVE | Memory::MEM_COMMIT,
                Memory::PAGE_NOACCESS,
            );

            if !result.is_null() {
                result
            } else {
                Memory::VirtualAlloc(
                    None,
                    len,
                    Memory::MEM_RESERVE | Memory::MEM_COMMIT,
                    Memory::PAGE_NOACCESS,
                )
            }
        };

        Self {
            ptr: region.cast(),
            len,
        }
    }

    /// Changes the protection of the first `length` bytes of this region to `protection`.
    pub fn protect(&self, length: usize, protection: Protection) {
        assert!(length <= self.len);

        #[cfg(target_family = "unix")]
        {
            use rustix::mm::MprotectFlags;

            let flags = match protection {
                Protection::ReadExec => MprotectFlags::READ | MprotectFlags::EXEC,
                Protection::ReadWrite => MprotectFlags::READ | MprotectFlags::WRITE,
            };

            // SAFETY: this region has been previously mapped by `new`, which makes it safe
            // to call `mprotect` on
            unsafe { mman::mprotect(self.ptr.cast(), length, flags).unwrap() }
        }

        #[cfg(target_family = "windows")]
        {
            let mut prev = Memory::PAGE_PROTECTION_FLAGS(0);
            let flags = match protection {
                Protection::ReadExec => Memory::PAGE_EXECUTE_READ,
                Protection::ReadWrite => Memory::PAGE_READWRITE,
            };

            unsafe {
                Memory::VirtualProtect(self.ptr.cast(), length, flags, &raw mut prev).unwrap()
            }
        }
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
