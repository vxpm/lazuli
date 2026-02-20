use cranelift_codegen::isa;
use cranelift_codegen::settings::Configurable;

pub fn x86_64_v1() -> isa::Builder {
    isa::lookup_by_name("x86_64").unwrap()
}

pub fn x86_64_v3() -> isa::Builder {
    let mut isa = isa::lookup_by_name("x86_64").unwrap();

    isa.enable("has_sse3").unwrap();
    isa.enable("has_ssse3").unwrap();
    isa.enable("has_sse41").unwrap();
    isa.enable("has_sse42").unwrap();
    isa.enable("has_fma").unwrap();
    isa.enable("has_lzcnt").unwrap();
    isa.enable("has_popcnt").unwrap();
    isa.enable("has_bmi1").unwrap();
    isa.enable("has_bmi2").unwrap();
    isa.enable("has_avx").unwrap();
    isa.enable("has_avx2").unwrap();

    isa
}

pub fn aarch64() -> isa::Builder {
    isa::lookup_by_name("aarch64").unwrap()
}
