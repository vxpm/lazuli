use cranelift_codegen::ir;

extern "C" fn floor_f32(x: f32) -> f32 {
    x.floor()
}

extern "C" fn floor_f64(x: f64) -> f64 {
    x.floor()
}

extern "C" fn ceil_f32(x: f32) -> f32 {
    x.ceil()
}

extern "C" fn ceil_f64(x: f64) -> f64 {
    x.ceil()
}

extern "C" fn trunc_f32(x: f32) -> f32 {
    x.trunc()
}

extern "C" fn trunc_f64(x: f64) -> f64 {
    x.trunc()
}

extern "C" fn round_f32(x: f32) -> f32 {
    x.round()
}

extern "C" fn round_f64(x: f64) -> f64 {
    x.round()
}

extern "C" fn fma_f32(a: f32, b: f32, c: f32) -> f32 {
    a.mul_add(b, c)
}

extern "C" fn fma_f64(a: f64, b: f64, c: f64) -> f64 {
    a.mul_add(b, c)
}

#[cfg(target_arch = "x86_64")]
#[repr(transparent)]
struct Vector128(core::arch::x86_64::__m128i);

// TODO: ensure this works, needed for MacOS support
#[cfg(target_arch = "aarch64")]
#[repr(transparent)]
struct Vector128(core::arch::aarch64::int64x2_t);

#[expect(
    improper_ctypes_definitions,
    reason = "vector intrinsics aren't FFI safe... but it works and there's no easy alternative"
)]
extern "C" fn x86_pshufb(a: Vector128, b: Vector128) -> Vector128 {
    let a: [u8; 16] = unsafe { std::mem::transmute(a.0) };
    let b: [u8; 16] = unsafe { std::mem::transmute(b.0) };

    let mut result: [u8; 16] = [0; 16];
    for i in 0..16 {
        let index = b[i];
        result[i] = std::hint::select_unpredictable(index & 0x80 == 0, a[index as usize & 0xF], 0);
    }

    unsafe { std::mem::transmute(result) }
}

pub fn get(libcall: ir::LibCall) -> usize {
    macro_rules! fn_addr {
        ($fn:expr) => {
            ($fn as *const () as usize)
        };
    }

    match libcall {
        ir::LibCall::CeilF32 => fn_addr!(ceil_f32),
        ir::LibCall::CeilF64 => fn_addr!(ceil_f64),
        ir::LibCall::FloorF32 => fn_addr!(floor_f32),
        ir::LibCall::FloorF64 => fn_addr!(floor_f64),
        ir::LibCall::TruncF32 => fn_addr!(trunc_f32),
        ir::LibCall::TruncF64 => fn_addr!(trunc_f64),
        ir::LibCall::NearestF32 => fn_addr!(round_f32),
        ir::LibCall::NearestF64 => fn_addr!(round_f64),
        ir::LibCall::FmaF32 => fn_addr!(fma_f32),
        ir::LibCall::FmaF64 => fn_addr!(fma_f64),
        ir::LibCall::Memcpy => fn_addr!(libc::memcpy),
        ir::LibCall::Memset => fn_addr!(libc::memset),
        ir::LibCall::Memmove => fn_addr!(libc::memmove),
        ir::LibCall::Memcmp => fn_addr!(libc::memcmp),
        ir::LibCall::X86Pshufb => fn_addr!(x86_pshufb),
        _ => unimplemented!("libcall: {libcall:?}"),
    }
}
