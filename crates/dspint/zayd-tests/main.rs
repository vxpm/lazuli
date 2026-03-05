#![feature(trim_prefix_suffix)]

mod file;

use std::fmt::Write;

use dspint::{Interpreter, Registers};
use lazuli::modules::audio::NopAudioModule;
use lazuli::modules::debug::NopDebugModule;
use lazuli::modules::disk::NopDiskModule;
use lazuli::modules::input::NopInputModule;
use lazuli::modules::render::NopRenderModule;
use lazuli::modules::vertex::NopVertexModule;
use lazuli::system::{self, Modules, System};
use libtest_mimic::{Arguments, Failed, Trial};

fn parse_code(mut words: &[u16]) -> Vec<dspint::Ins> {
    let mut ins = vec![];
    while !words.is_empty() {
        let decoded = dspint::ins::Ins::new(words[0]).decoded();
        if decoded.opcode.needs_extra() {
            ins.push(dspint::Ins::with_extra(words[0], words[1]));
            words = &words[2..];
        } else {
            ins.push(dspint::Ins::new(words[0]));
            words = &words[1..];
        }
    }

    ins
}

struct FailedCase {
    code: Vec<dspint::Ins>,
    initial: Registers,
    expected: Registers,
    divergences: Vec<(dspint::Reg, u16, u16)>,
}

fn run_case(sys: &mut System, case: file::TestCase) -> Result<(), FailedCase> {
    let mut dsp = Interpreter::default();

    // setup
    sys.dsp.control.set_halt(false);
    dsp.pc = 62;
    dsp.regs = case.initial_regs();
    dsp.mem.iram[62..][..case.instructions.len()].copy_from_slice(&case.instructions);
    dsp.mem.iram[62 + case.instructions.len()] = 0x21; // HALT

    // run until halt
    let code = parse_code(&case.instructions);
    while !sys.dsp.control.halt() {
        dsp.step(sys);
    }

    // check
    let allow_status = std::env::var("IGNORE_STATUS").is_ok();
    let expected = case.expected_regs();
    let mut divergences = vec![];
    for i in 0..32 {
        let reg = dspint::Reg::new(i);
        let value = dsp.regs.get(reg);
        let expected = expected.get(reg);

        if value != expected {
            if allow_status && reg == dspint::Reg::Status {
                continue;
            }

            if reg == dspint::Reg::Config {
                continue;
            }

            divergences.push((reg, value, expected));
        }
    }

    if !divergences.is_empty() {
        return Err(FailedCase {
            code,
            initial: case.initial_regs(),
            expected: case.expected_regs(),
            divergences,
        });
    }

    Ok(())
}

fn run_test(file: file::TestFile, quiet: bool) -> Result<(), Failed> {
    let early_exit = std::env::var("EARLY_EXIT").is_ok();
    let total = file.cases.len();
    let mut failures = vec![];

    let modules = Modules {
        audio: Box::new(NopAudioModule),
        debug: Box::new(NopDebugModule),
        disk: Box::new(NopDiskModule),
        input: Box::new(NopInputModule),
        render: Box::new(NopRenderModule),
        vertex: Box::new(NopVertexModule),
    };

    let mut system = System::new(
        modules,
        system::Config {
            ipl: None,
            sideload: None,
            ipl_lle: false,
            perform_efb_copies: false,
        },
    );

    for (i, case) in file.cases.into_iter().enumerate() {
        let Err(failure) = run_case(&mut system, case) else {
            continue;
        };

        let mut pc = 62;
        let mut disasm = String::new();
        for ins in failure.code.iter() {
            writeln!(&mut disasm, "{pc:04X} {ins:?}").unwrap();
            pc += if ins.decoded().needs_extra { 2 } else { 1 };
        }

        let divergences = failure
            .divergences
            .iter()
            .map(|(r, v, e)| format!("{r:?}(v={v:04X}, e={e:04X}), "))
            .collect::<String>();

        if early_exit {
            failures.push(format!(
                "Case {i} failed:\r\nINITIAL: {:04X?}\r\nEXPECTED: {:04X?}\r\nDIVERGENCES: {}\r\nCODE:\r\n{disasm}",
                failure.initial,
                failure.expected,
                divergences.trim_suffix(", "),
            ));
            break;
        } else {
            failures.push(format!(
                "Case {i} failed: {}\r\n{}",
                divergences.trim_suffix(", "),
                disasm
            ));
        }
    }

    if !failures.is_empty() {
        if quiet {
            return Err(Failed::from(format!(
                "Failed a total of {} cases (out of {})",
                failures.len(),
                total
            )));
        }

        let mut msg = format!(
            "Failed a total of {} cases (out of {})\r\n\r\n",
            failures.len(),
            total
        );
        let tests_to_show = 8;

        let show = failures.iter().take(tests_to_show);
        for failure in show {
            writeln!(&mut msg, "{}", failure).unwrap();
        }

        if failures.len() > tests_to_show {
            writeln!(
                &mut msg,
                "... and {} others",
                failures.len() - tests_to_show
            )
            .unwrap();
        }

        return Err(Failed::from(msg));
    }

    Ok(())
}

fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let tests_dir = std::fs::read_dir(format!("{manifest}/zayd-tests/tests")).unwrap();
    let args = Arguments::from_args();
    let env_quiet = std::env::var("QUIET").is_ok();

    let mut tests = vec![];
    for test in tests_dir {
        let test = test.unwrap();
        if test.file_type().unwrap().is_file() {
            let file = file::TestFile::open(test.path());
            tests.push(Trial::test(
                test.file_name().to_string_lossy().into_owned(),
                move || {
                    let result =
                        std::panic::catch_unwind(move || run_test(file, args.quiet || env_quiet));

                    match result {
                        Ok(r) => r,
                        Err(e) => {
                            let mut msg = "<unknown panic>".to_owned();
                            if let Some(s) = e.downcast_ref::<String>() {
                                msg = s.clone();
                            } else if let Some(s) = e.downcast_ref::<&'static str>() {
                                msg = (*s).to_owned();
                            }

                            Err(Failed::from(msg))
                        }
                    }
                },
            ));
        }
    }

    std::panic::set_hook(Box::new(move |_| ()));
    libtest_mimic::run(&args, tests).exit();
}
