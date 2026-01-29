use eframe::egui;
use lazuli::Address;
use lazuli::system::pi::{self, InterruptSources};
use serde::{Deserialize, Serialize};

use crate::windows::Ctx;
use crate::windows::subsystem::mmio_dbg;
use crate::{AppWindow, State};

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    #[serde(skip)]
    fifo_start: Address,
    #[serde(skip)]
    fifo_end: Address,
    #[serde(skip)]
    fifo_current: Address,
    #[serde(skip)]
    active: InterruptSources,
    #[serde(skip)]
    raised: InterruptSources,
    #[serde(skip)]
    debug: bool,
}

#[typetag::serde(name = "subsystem-pi")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Processor Interface"
    }

    fn prepare(&mut self, state: &mut State) {
        let core = &mut state.lazuli;
        let pi = &mut core.sys.processor;
        self.fifo_start = pi.fifo_start;
        self.fifo_end = pi.fifo_end;
        self.fifo_current = pi.fifo_current.address();
        self.active = pi::get_active_interrupts(&core.sys);
        self.raised = pi::get_raised_interrupts(&core.sys);

        if self.debug {
            self.debug = false;
            core.sys.gpu.pix.interrupt.set_finish(true);
            core.sys.scheduler.schedule_now(pi::check_interrupts);
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            if ui.button("debug").clicked() {
                self.debug = true;
            }

            mmio_dbg(ui, "FIFO start", &self.fifo_start);
            mmio_dbg(ui, "FIFO end", &self.fifo_end);
            mmio_dbg(ui, "FIFO current", &self.fifo_current);
            mmio_dbg(ui, "Active Interrupts", &self.active);
            mmio_dbg(ui, "Raised Interrupts", &self.raised);
        });
    }
}
