use eframe::egui::{self, Color32};
use egui_extras::{Column, TableBuilder};
use lazuli::modules::debug::Location;
use lazuli::system;
use lazuli::system::eabi::CallStack;
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    #[serde(skip)]
    location: Option<Location<'static>>,
    #[serde(skip)]
    call_stack: CallStack,
}

#[typetag::serde(name = "call_stack")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Call Stack"
    }

    fn prepare(&mut self, state: &mut State) {
        let emulator = &state.lazuli;
        self.call_stack = system::eabi::current_call_stack(&emulator.sys);
        self.location = emulator
            .sys
            .modules
            .debug
            .find_location(emulator.sys.cpu.pc)
            .map(|l| l.into_owned());
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(location) = &self.location {
                    ui.label(format!(
                        "Location of PC: {}:{}:{}",
                        location.file.as_deref().unwrap_or("<unknown>"),
                        location.line.unwrap_or(0),
                        location.column.unwrap_or(0)
                    ));
                } else {
                    ui.label("Location of PC: <unknown>");
                }
            });

            ui.separator();

            let builder = TableBuilder::new(ui)
                .auto_shrink(egui::Vec2b::new(false, true))
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto()) // addr
                .column(Column::auto()) // stack
                .column(Column::remainder().at_least(200.0)); // symbol

            let table = builder.header(20.0, |mut header| {
                header.col(|ui| {
                    ui.label("Address");
                });
                header.col(|ui| {
                    ui.label("Stack");
                });
                header.col(|ui| {
                    ui.label("Symbol");
                });
            });

            table.body(|mut body| {
                for call in self.call_stack.0.iter().rev() {
                    body.row(20.0, |mut row| {
                        row.col(|ui| {
                            let text = egui::RichText::new(call.address.to_string())
                                .family(egui::FontFamily::Monospace)
                                .color(Color32::LIGHT_BLUE);

                            ui.label(text);
                        });

                        row.col(|ui| {
                            let text = egui::RichText::new(call.stack.to_string())
                                .family(egui::FontFamily::Monospace)
                                .color(Color32::LIGHT_GREEN);

                            ui.label(text);
                        });

                        row.col(|ui| {
                            let text = egui::RichText::new(format!(
                                "{} ({})",
                                call.symbol.as_deref().unwrap_or("<unknown>"),
                                call.location.as_deref().unwrap_or("<unknown>"),
                            ))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::GRAY);

                            ui.label(text);
                        });
                    })
                }
            });
        });
    }
}
