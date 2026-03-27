use eframe::egui;
use lazuli::Address;
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum VarKind {
    #[default]
    U32,
    U16,
    U8,
}

#[derive(Serialize, Deserialize)]
struct Variable {
    address: u32,
    label: String,
    kind: VarKind,
    #[serde(skip_serializing, default)]
    value: u32,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    variables: Vec<Variable>,

    #[serde(skip)]
    variable_address: String,
    #[serde(skip)]
    variable_label: String,
    #[serde(skip)]
    variable_kind: VarKind,
}

#[typetag::serde(name = "variables")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Variables"
    }

    fn prepare(&mut self, state: &mut State) {
        let emulator = &state.lazuli;
        for variable in self.variables.iter_mut() {
            if let Some(physical) = emulator.sys.mem.translate_data_addr(variable.address) {
                variable.value = emulator.sys.read_phys_pure(Address(physical)).unwrap_or(0);
            } else {
                variable.value = 0xDEADBEEF;
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        ui.set_max_width(250.0);

        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            ui.scope(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Address: ");
                    ui.text_edit_singleline(&mut self.variable_address);
                });
                ui.horizontal(|ui| {
                    ui.label("Label: ");
                    ui.text_edit_singleline(&mut self.variable_label);
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("")
                        .selected_text(format!("{:?}", self.variable_kind))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.variable_kind, VarKind::U32, "U32");
                            ui.selectable_value(&mut self.variable_kind, VarKind::U16, "U16");
                            ui.selectable_value(&mut self.variable_kind, VarKind::U8, "U8");
                        });

                    if ui.button("Add").clicked() {
                        let address = self
                            .variable_address
                            .trim()
                            .trim_prefix("0x")
                            .replace("_", "");
                        let label = self.variable_label.clone();
                        let kind = self.variable_kind;

                        if let Ok(address) = u32::from_str_radix(&address, 16) {
                            dbg!(Address(address));
                            self.variables.push(Variable {
                                address,
                                label,
                                kind,
                                value: 0,
                            });
                        }
                    }
                });
            });

            let mut remove = None;
            for (i, variable) in self.variables.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("🗑").clicked() {
                        remove = Some(i);
                    }

                    let value = match variable.kind {
                        VarKind::U32 => format!("0x{:08X}", variable.value),
                        VarKind::U16 => format!("0x{:04X}", variable.value >> 16),
                        VarKind::U8 => format!("0x{:02X}", variable.value >> 24),
                    };

                    ui.label(format!("{}: {}", variable.label, value))
                        .on_hover_text(format!("0x{:08X}", variable.address));
                });
            }

            if let Some(i) = remove {
                self.variables.remove(i);
            }
        });
    }
}
