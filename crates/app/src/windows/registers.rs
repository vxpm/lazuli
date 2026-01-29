use eframe::egui::{self, Color32};
use egui_extras::{Column, TableBuilder};
use lazuli::gekko::Cpu;
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum Group {
    #[default]
    Gpr,
    Fpr,
    Cr,
    Others,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    group: Group,
    #[serde(skip)]
    cpu: Cpu,
}

impl Window {
    fn gpr(&self, ui: &mut egui::Ui) {
        let builder = TableBuilder::new(ui)
            .auto_shrink(egui::Vec2b::new(false, true))
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::remainder());

        let table = builder.header(20.0, |mut header| {
            header.col(|ui| {
                ui.label("GPR");
            });
            header.col(|ui| {
                ui.label("Hex");
            });
        });

        table.body(|mut body| {
            for (gpr, value) in self.cpu.user.gpr.iter().copied().enumerate() {
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        let text = egui::RichText::new(format!("R{gpr:02}"))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_BLUE);

                        ui.label(text);
                    });

                    row.col(|ui| {
                        let text = egui::RichText::new(format!("{value:08X}"))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_GREEN);

                        ui.label(text);
                    });
                })
            }
        });
    }

    fn fpr(&self, ui: &mut egui::Ui) {
        let builder = TableBuilder::new(ui)
            .auto_shrink(egui::Vec2b::new(false, true))
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::remainder().at_least(100.0))
            .column(Column::remainder().at_least(100.0));

        let table = builder.header(20.0, |mut header| {
            header.col(|ui| {
                ui.label("FPR");
            });
            header.col(|ui| {
                ui.label("PS0");
            });
            header.col(|ui| {
                ui.label("PS1");
            });
        });

        table.body(|mut body| {
            for (fpr, value) in self.cpu.user.fpr.iter().copied().enumerate() {
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        let text = egui::RichText::new(format!("F{fpr:02}"))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_BLUE);

                        ui.label(text);
                    });

                    row.col(|ui| {
                        let text = egui::RichText::new(format!("{}", value.0[0]))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_GREEN);

                        ui.label(text);
                    });

                    row.col(|ui| {
                        let text = egui::RichText::new(format!("{}", value.0[1]))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_GREEN);

                        ui.label(text);
                    });
                })
            }
        });
    }

    fn cr(&self, ui: &mut egui::Ui) {
        let builder = TableBuilder::new(ui)
            .auto_shrink(egui::Vec2b::new(false, true))
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::remainder());

        let table = builder.header(20.0, |mut header| {
            header.col(|ui| {
                ui.label("Reg");
            });
            header.col(|ui| {
                ui.label("Value");
            });
        });

        table.body(|mut body| {
            body.row(20.0, |mut row| {
                let xer = self.cpu.user.xer.clone();
                row.col(|ui| {
                    let text = egui::RichText::new("XER".to_string())
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_BLUE);

                    ui.label(text);
                });

                row.col(|ui| {
                    let text = egui::RichText::new(format!("{xer:?}"))
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_GREEN);

                    ui.label(text);
                });
            });

            body.row(20.0, |mut row| {
                let cr = self.cpu.user.cr.to_bits();
                row.col(|ui| {
                    let text = egui::RichText::new("CR".to_string())
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_BLUE);

                    ui.label(text);
                });

                row.col(|ui| {
                    let text = egui::RichText::new(format!("0x{cr:08X}"))
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_GREEN);

                    ui.label(text);
                });
            });

            for index in 0..8 {
                let cr = self.cpu.user.cr.fields_at(7 - index).unwrap();
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        let text = egui::RichText::new(format!("CR{index:02}"))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_BLUE);

                        ui.label(text);
                    });

                    row.col(|ui| {
                        let text = egui::RichText::new(format!("{cr:?}"))
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::LIGHT_GREEN);

                        ui.label(text);
                    });
                })
            }
        });
    }

    fn others(&self, ui: &mut egui::Ui) {
        let builder = TableBuilder::new(ui)
            .auto_shrink(egui::Vec2b::new(false, true))
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::remainder());

        let table = builder.header(20.0, |mut header| {
            header.col(|ui| {
                ui.label("Reg");
            });
            header.col(|ui| {
                ui.label("Value");
            });
        });

        table.body(|mut body| {
            body.row(20.0, |mut row| {
                let ctr = self.cpu.user.ctr;
                row.col(|ui| {
                    let text = egui::RichText::new("CTR".to_string())
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_BLUE);

                    ui.label(text);
                });

                row.col(|ui| {
                    let text = egui::RichText::new(format!("{ctr}"))
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_GREEN);

                    ui.label(text);
                });
            });

            body.row(20.0, |mut row| {
                let dec = self.cpu.supervisor.misc.dec;
                row.col(|ui| {
                    let text = egui::RichText::new("DEC".to_string())
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_BLUE);

                    ui.label(text);
                });

                row.col(|ui| {
                    let text = egui::RichText::new(format!("{dec}"))
                        .family(egui::FontFamily::Monospace)
                        .color(Color32::LIGHT_GREEN);

                    ui.label(text);
                });
            });
        });
    }
}

#[typetag::serde(name = "registers")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Registers"
    }

    fn prepare(&mut self, state: &mut State) {
        self.cpu = state.lazuli.sys.cpu.clone();
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::ComboBox::from_label("Group")
                .selected_text(format!("{:?}", self.group))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.group, Group::Gpr, "Gpr");
                    ui.selectable_value(&mut self.group, Group::Fpr, "Fpr");
                    ui.selectable_value(&mut self.group, Group::Cr, "Cr");
                    ui.selectable_value(&mut self.group, Group::Others, "Others");
                });

            ui.separator();

            match self.group {
                Group::Gpr => self.gpr(ui),
                Group::Fpr => self.fpr(ui),
                Group::Cr => self.cr(ui),
                Group::Others => self.others(ui),
            }
        });
    }
}
