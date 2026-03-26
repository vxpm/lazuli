use eframe::egui;
use lazuli::Address;
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
}

#[typetag::serde(name = "subsystem-pi")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Processor Interface"
    }

    fn prepare(&mut self, state: &mut State) {
        let core = &state.lazuli;
        let pi = &core.sys.processor;
        self.fifo_start = pi.fifo_start;
        self.fifo_end = pi.fifo_end_minus_4;
        self.fifo_current = pi.fifo_current.address();
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            mmio_dbg(ui, "FIFO start", &self.fifo_start);
            mmio_dbg(ui, "FIFO end", &self.fifo_end);
            mmio_dbg(ui, "FIFO current", &self.fifo_current);
        });
    }
}
