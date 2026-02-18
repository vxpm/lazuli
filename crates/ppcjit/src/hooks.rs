use cranelift::codegen::ir;
use cranelift_codegen::isa::CallConv;
use gekko::{Address, Cpu, QuantReg};
use strum::FromRepr;

use crate::FastmemLut;
use crate::block::{Info, LinkData};

pub type Context = std::ffi::c_void;

pub type GetRegistersHook = extern "C-unwind" fn(*mut Context) -> *mut Cpu;
pub type GetFastmemHook = extern "C-unwind" fn(*mut Context) -> *mut FastmemLut;

pub type FollowLinkHook =
    extern "C-unwind" fn(*const Info, *mut Context, *mut LinkData) -> bool;
pub type TryLinkHook = extern "C-unwind" fn(*mut Context, Address, *mut LinkData);

pub type ReadHook<T> = extern "C-unwind" fn(*mut Context, Address, *mut T) -> bool;
pub type WriteHook<T> = extern "C-unwind" fn(*mut Context, Address, T) -> bool;
pub type ReadQuantizedHook =
    extern "C-unwind" fn(*mut Context, Address, QuantReg, *mut f64) -> u8;
pub type WriteQuantizedHook = extern "C-unwind" fn(*mut Context, Address, QuantReg, f64) -> u8;

pub type InvalidateICache = extern "C-unwind" fn(*mut Context, Address);

pub type GenericHook = extern "C-unwind" fn(*mut Context);

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u32)]
pub enum HookKind {
    GetRegisters,
    GetFastmem,
    FollowLink,
    TryLink,
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
    /// Hook that returns a pointer to the CPU state struct given the context.
    pub get_registers: GetRegistersHook,
    /// Hook that returns a pointer to the fastmem LUT given the context.
    pub get_fastmem: GetFastmemHook,

    /// Hook that checks whether a linked block should be followed or the execution should return.
    pub follow_link: FollowLinkHook,
    /// Tries to link this block to another one given the current context, the destination address
    /// and a pointer to where the linked block function pointer should be stored.
    pub try_link: TryLinkHook,

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
            follow_link: stub!(),
            try_link: stub!(),
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

    /// Returns the function signature for the `follow_link` hook.
    pub(crate) fn follow_link_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type), // info
                ir::AbiParam::new(ptr_type), // ctx
                ir::AbiParam::new(ptr_type), // lnk data
            ],
            returns: vec![ir::AbiParam::new(ir::types::I8)], // follow?
            call_conv,
        }
    }

    /// Returns the function signature for the `try_link` hook.
    pub(crate) fn try_link_sig(ptr_type: ir::Type, call_conv: CallConv) -> ir::Signature {
        ir::Signature {
            params: vec![
                ir::AbiParam::new(ptr_type),       // ctx
                ir::AbiParam::new(ir::types::I32), // address to link to
                ir::AbiParam::new(ptr_type),       // link ptr storage
            ],
            returns: vec![],
            call_conv,
        }
    }

    /// Returns the function signature for a memory read hook.
    pub(crate) fn read_sig(ptr_type: ir::Type, _read_type: ir::Type, call_conv: CallConv) -> ir::Signature {
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
    pub(crate) fn write_sig(ptr_type: ir::Type, write_type: ir::Type, call_conv: CallConv) -> ir::Signature {
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
