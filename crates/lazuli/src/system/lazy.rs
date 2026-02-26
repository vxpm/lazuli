use gekko::Exception;

use crate::system::System;

#[derive(Debug, Default)]
pub struct Lazy {
    pub last_updated_tb: u64,
    pub last_updated_dec: u64,
}

impl System {
    pub fn update_time_base(&mut self) {
        let last_updated = self.lazy.last_updated_tb;
        let now = self.scheduler.elapsed_time_base();
        let delta = now - last_updated;

        let prev = self.cpu.supervisor.misc.tb;
        let new = prev.wrapping_add(delta);

        tracing::trace!(
            "updating time base - now {now}, last updated {last_updated}, since then {delta}. prev: {prev}, new: {new}"
        );

        self.lazy.last_updated_tb = now;
        self.cpu.supervisor.misc.tb = new;
    }

    pub fn update_decrementer(&mut self) {
        let last_updated = self.lazy.last_updated_dec;
        let now = self.scheduler.elapsed();
        let delta = now - last_updated;

        let prev = self.cpu.supervisor.misc.dec;
        let new = prev.wrapping_sub(delta as u32);

        tracing::trace!(
            "updating dec - now {now}, last updated {last_updated}, since then {delta}. prev: {prev}, new: {new}"
        );

        self.lazy.last_updated_dec = now;
        self.cpu.supervisor.misc.dec = new;
    }

    pub fn decrementer_overflow(&mut self) {
        self.update_decrementer();
        if self.cpu.supervisor.config.msr.interrupts() {
            self.cpu.raise_exception(Exception::Decrementer);
            self.scheduler
                .schedule(u32::MAX as u64, System::decrementer_overflow);
        } else {
            self.scheduler.schedule(32, System::decrementer_overflow);
        }
    }
}
