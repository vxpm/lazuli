use dspint::Interpreter;
use lazuli::cores::DspCore;
use lazuli::system::System;

use super::{DSP_COEF, DSP_ROM};

pub struct Core {
    interpreter: Interpreter,
}

impl Default for Core {
    fn default() -> Self {
        let mut interpreter = Interpreter::default();
        interpreter.mem.irom.copy_from_slice(&DSP_ROM[..]);
        interpreter.mem.coef.copy_from_slice(&DSP_COEF[..]);

        Self { interpreter }
    }
}

impl DspCore for Core {
    fn exec(&mut self, sys: &mut System, instructions: u32) -> u32 {
        self.interpreter.check_reset(sys);
        self.interpreter.exec(sys, instructions);
        instructions
    }
}
