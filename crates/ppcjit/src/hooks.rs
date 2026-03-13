use cranelift::codegen::ir;
use cranelift::codegen::isa::CallConv;
use gekko::{Address, Cpu, QuantReg};
use strum::FromRepr;

use crate::FastmemLut;
use crate::block::BlockFn;

/// Caller context.
pub type Context = std::ffi::c_void;
/// Data specific to a block exit.
pub type ExitData = std::ffi::c_void;
/// Count of how many instructions were executed in a block.
pub type InstructionCount = u16;
/// Count of how many cycles were executed in a block.
pub type CycleCount = u16;

/// Hook called before the first block in the chain starts executing. Should return a pointer to
/// the CPU registers struct.
pub type GetRegistersHook = extern "C-unwind" fn(*mut Context) -> *mut Cpu;
/// Hook called before the first block in the chain starts executing. Should return a pointer to
/// the fastmem lookup-table.
pub type GetFastmemHook = extern "C-unwind" fn(*mut Context) -> *mut FastmemLut;

/// Hook called on any block exit.
///
/// Each exit has some data associated with it which can be used by this hook as it wish. The size
/// of the data is configurable in the JIT [`Settings`](super::Settings).
///
/// Should return a pointer to a block to jump to and keep the chain executing or `None` if you
/// wish to exit the chain. In other words, this allows for _block linking_.
pub type ExitHook = extern "C-unwind" fn(
    *const Context,
    *mut ExitData,
    InstructionCount,
    CycleCount,
) -> Option<BlockFn>;

/// Hook called whenever the JIT wants to read a value of type `T` from an address that isn't
/// accessible through fastmem. Should return whether the read failed.
pub type ReadHook<T> = extern "C-unwind" fn(*mut Context, Address, *mut T) -> bool;
/// Hook called whenever the JIT wants to write a value of type `T` to an address that isn't
/// accessible through fastmem. Should return whether the write failed.
pub type WriteHook<T> = extern "C-unwind" fn(*mut Context, Address, T) -> bool;
/// Hook called whenever the JIT wants to read a quantized paired-single from an address that isn't
/// accessible through fastmem. Should return whether the read failed.
pub type ReadQuantizedHook = extern "C-unwind" fn(*mut Context, Address, QuantReg, *mut f64) -> u8;
/// Hook called whenever the JIT wants to write a quantized paired-single to an address that isn't
/// accessible through fastmem. Should return whether the write failed.
pub type WriteQuantizedHook = extern "C-unwind" fn(*mut Context, Address, QuantReg, f64) -> u8;

/// Hook that invalidates the instruction cache line that contains the given address.
pub type InvalidateICache = extern "C-unwind" fn(*mut Context, Address);

/// Generic hook signature for hooks that don't take any arguments or return anything.
pub type GenericHook = extern "C-unwind" fn(*mut Context);

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u32)]
pub enum HookKind {
    GetRegisters,
    GetFastmem,
    Exit,
    ReadI8,
    ReadI16,
    ReadI32,
    ReadI64,
    WriteI8,
    WriteI16,
    WriteI32,
    WriteI64,
    ReadQuant,
    WriteQuant,
    InvICache,
    ClearICache,
    DCacheDma,
    MsrChanged,
    IBatChanged,
    DBatChanged,
    TbRead,
    TbChanged,
    DecRead,
    DecChanged,
}

/// External functions that JITed code calls.
pub struct Hooks {
    pub get_registers: GetRegistersHook,
    pub get_fastmem: GetFastmemHook,
    pub exit: ExitHook,

    // memory
    pub read_i8: ReadHook<i8>,
    pub write_i8: WriteHook<i8>,
    pub read_i16: ReadHook<i16>,
    pub write_i16: WriteHook<i16>,
    pub read_i32: ReadHook<i32>,
    pub write_i32: WriteHook<i32>,
    pub read_i64: ReadHook<i64>,
    pub write_i64: WriteHook<i64>,
    pub read_quantized: ReadQuantizedHook,
    pub write_quantized: WriteQuantizedHook,

    // cache
    pub invalidate_icache: InvalidateICache,
    pub clear_icache: GenericHook,
    pub dcache_dma: GenericHook,

    // msr
    pub msr_changed: GenericHook,

    // bats
    pub ibat_changed: GenericHook,
    pub dbat_changed: GenericHook,

    // time base
    pub tb_read: GenericHook,
    pub tb_changed: GenericHook,

    // decrementer
    pub dec_read: GenericHook,
    pub dec_changed: GenericHook,
}

impl Hooks {
    #[allow(unused_assignments)]
    #[cfg(test)]
    pub(crate) unsafe fn stub() -> Self {
        let mut count = usize::MAX;
        macro_rules! stub {
            () => {{
                let ptr = unsafe { std::mem::transmute(count) };
                count -= 1;
                ptr
            }};
        }

        Self {
            get_registers: stub!(),
            get_fastmem: stub!(),
            exit: stub!(),
            read_i8: stub!(),
            write_i8: stub!(),
            read_i16: stub!(),
            write_i16: stub!(),
            read_i32: stub!(),
            write_i32: stub!(),
            read_i64: stub!(),
            write_i64: stub!(),
            read_quantized: stub!(),
            write_quantized: stub!(),
            invalidate_icache: stub!(),
            clear_icache: stub!(),
            dcache_dma: stub!(),
            msr_changed: stub!(),
            ibat_changed: stub!(),
            dbat_changed: stub!(),
            tb_read: stub!(),
            tb_changed: stub!(),
            dec_read: stub!(),
            dec_changed: stub!(),
        }
    }

    /// Returns the function signature for the `get_registers` hook.
    pub(crate) fn get_registers_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type), // ctx
            ],
            returns: vec![ir::AbiParam::new(ptr_type)], // registers
            call_conv,
        }
    }

    /// Returns the function signature for the `get_fastmem` hook.
    pub(crate) fn get_fastmem_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type), // ctx
            ],
            returns: vec![ir::AbiParam::new(ptr_type)], // fastmem lut
            call_conv,
        }
    }

    /// Returns the function signature for the `on_exit` hook.
    pub(crate) fn on_exit_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ptr_type),       // exit data
                ir::AbiParam::new(ir::types::I16), // inst count
                ir::AbiParam::new(ir::types::I16), // cycle count
            ],
            returns: vec![ir::AbiParam::new(ptr_type)], // pointer to linked block
            call_conv,
        }
    }

    /// Returns the function signature for a memory read hook.
    pub(crate) fn read_sig(
        ptr_type: ir::Type,
        _read_type: ir::Type,
        call_conv: CallConv,
    ) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address
                ir::AbiParam::new(ptr_type),       // value ptr
            ],
            returns: vec![ir::AbiParam::new(ir::types::I8)], // success
            call_conv,
        }
    }

    /// Returns the function signature for a memory write hook.
    pub(crate) fn write_sig(
        ptr_type: ir::Type,
        write_type: ir::Type,
        call_conv: CallConv,
    ) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address
                ir::AbiParam::new(write_type),     // value
            ],
            returns: vec![ir::AbiParam::new(ir::types::I8)], // success
            call_conv,
        }
    }

    /// Returns the function signature for a quantized memory read hook.
    pub(crate) fn read_quantized_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address
                ir::AbiParam::new(ir::types::I32), // gqr
                ir::AbiParam::new(ptr_type),       // value ptr
            ],
            returns: vec![ir::AbiParam::new(ir::types::I8)], // size
            call_conv,
        }
    }

    /// Returns the function signature for a quantized memory read hook.
    pub(crate) fn write_quantized_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address
                ir::AbiParam::new(ir::types::I32), // gqr
                ir::AbiParam::new(ir::types::F64), // value
            ],
            returns: vec![ir::AbiParam::new(ir::types::I8)], // size
            call_conv,
        }
    }

    /// Returns the function signature for a invalidade icache hook.
    pub(crate) fn invalidate_icache_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address
            ],
            returns: vec![],
            call_conv,
        }
    }

    /// Returns the function signature for a generic hook.
    pub(crate) fn generic_hook_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type), // ctx
            ],
            returns: vec![],
            call_conv,
        }
    }
}
