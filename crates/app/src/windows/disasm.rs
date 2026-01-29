use eframe::egui;
use egui_extras::{Column, TableBuilder};
use lazuli::Address;
use lazuli::gekko::disasm::{Extensions, Ins, ParsedIns};
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[derive(Serialize, Deserialize)]
pub struct Window {
    target: u32,
    #[serde(skip)]
    target_text: String,
    follow_pc: bool,
    simplified: bool,

    #[serde(skip)]
    pc: u32,
    #[serde(skip)]
    rows: u32,
    #[serde(skip)]
    instructions: Vec<Ins>,
    #[serde(skip)]
    breakpoints: Vec<u32>,
    #[serde(skip)]
    breakpoint_to_toggle: Option<u32>,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            target: Default::default(),
            target_text: String::new(),
            follow_pc: true,
            simplified: true,
            instructions: Vec::new(),

            pc: 0,
            rows: 0,
            breakpoints: Vec::new(),
            breakpoint_to_toggle: None,
        }
    }
}

impl Window {}

#[typetag::serde(name = "disasm")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "📼 Disassembly"
    }

    fn prepare(&mut self, state: &mut State) {
        self.breakpoints.clear();
        self.breakpoints
            .extend(state.breakpoints.iter().map(|b| b.value()));

        if let Some(breakpoint) = self.breakpoint_to_toggle.take() {
            let breakpoint = Address(breakpoint);
            if state.breakpoints.contains(&breakpoint) {
                state.remove_breakpoint(breakpoint);
            } else {
                state.add_breakpoint(breakpoint);
            }
        }

        let emulator = &state.lazuli;
        self.pc = emulator.sys.cpu.pc.value();

        if self.follow_pc {
            self.target = self.pc;
        }

        let mut current = Address(self.target.wrapping_sub(4 * (self.rows / 2)));
        for _ in 0..self.rows {
            let translated = emulator
                .sys
                .translate_inst_addr(current)
                .unwrap_or_default();

            let code = emulator.sys.read_phys_pure(translated).unwrap_or(0);
            let ins = Ins::new(code, Extensions::gekko_broadway());
            self.instructions.push(ins);

            current += 4;
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.follow_pc, "Follow PC");
            ui.checkbox(&mut self.simplified, "Simplified");
        });

        if !self.follow_pc {
            ui.horizontal(|ui| {
                ui.label("Target: ");
                if ui.text_edit_singleline(&mut self.target_text).lost_focus() {
                    let clean = self.target_text.trim_prefix("0x").replace("_", "");
                    if let Ok(addr) = u32::from_str_radix(&clean, 16) {
                        self.target = addr;
                        self.target_text = format!("{:08X}", self.target);
                    }
                }
            });
        }

        let response = ui.scope(|ui| {
            let builder = TableBuilder::new(ui)
                .auto_shrink(true)
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto())
                .column(Column::exact(250.0));

            let table = builder.header(20.0, |mut header| {
                header.col(|ui| {
                    ui.label("Address");
                });
                header.col(|ui| {
                    ui.label("Instruction");
                });
            });

            table.body(|mut body| {
                let ui = body.ui_mut();
                ui.spacing_mut().item_spacing = egui::Vec2::new(5.0, 0.0);
                ui.set_max_width(ui.available_width());

                let mut current = self.target.wrapping_sub(4 * (self.rows / 2));
                self.rows = (body.ui_mut().available_height() / 20.0) as u32;

                for ins in self.instructions.drain(..) {
                    body.row(20.0, |mut row| {
                        row.col(|ui| {
                            let color = if current == self.pc {
                                egui::Color32::LIGHT_RED
                            } else if current == self.target {
                                egui::Color32::LIGHT_GREEN
                            } else {
                                egui::Color32::LIGHT_BLUE
                            };

                            let is_breakpoint = self.breakpoints.contains(&current);
                            let breakpoint_symbol = egui::RichText::new("⏺");
                            let breakpoint_toggle = if is_breakpoint {
                                egui::Label::new(breakpoint_symbol.color(egui::Color32::LIGHT_RED))
                                    .selectable(false)
                                    .sense(egui::Sense::click())
                            } else {
                                egui::Label::new(breakpoint_symbol.color(egui::Color32::GRAY))
                                    .selectable(false)
                                    .sense(egui::Sense::click())
                            };

                            let text = egui::RichText::new(Address(current).to_string())
                                .family(egui::FontFamily::Monospace)
                                .color(color);

                            ui.horizontal(|ui| {
                                if ui.add(breakpoint_toggle).clicked() {
                                    self.breakpoint_to_toggle = Some(current);
                                }

                                ui.label(text);
                            });
                        });

                        row.col(|ui| {
                            let mut parsed = ParsedIns::new();
                            if self.simplified {
                                ins.parse_simplified(&mut parsed);
                            } else {
                                ins.parse_basic(&mut parsed);
                            }

                            let text = egui::RichText::new(parsed.to_string())
                                .color(egui::Color32::LIGHT_GRAY)
                                .family(egui::FontFamily::Monospace);

                            ui.add_space(2.5);
                            ui.label(text);
                        });
                    });

                    current = current.wrapping_add(4);
                }
            });
        });

        let rect = response.response.rect;
        let response = ui.interact(rect, egui::Id::new("disasm_scroll"), egui::Sense::hover());

        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            self.target = self
                .target
                .wrapping_add_signed(-4 * ((delta.y / 10.0) as i32));
        }
    }
}
