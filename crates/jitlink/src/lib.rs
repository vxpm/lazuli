mod libcalls;

use cranelift_codegen::FinalizedMachReloc;
use cranelift_codegen::binemit::Reloc;
pub use libcalls::get as libcall;

/// Writes a relocation in the given buffer.
pub fn write_relocation(code: &mut [u8], reloc: &FinalizedMachReloc, addr: usize) {
    match reloc.kind {
        Reloc::Abs8 => {
            let base = reloc.offset;
            code[base as usize..][..size_of::<usize>()].copy_from_slice(&addr.to_ne_bytes());
        }
        _ => todo!("write relocation kind {:?}", reloc.kind),
    }
}
