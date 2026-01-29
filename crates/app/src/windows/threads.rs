use bytesize::ByteSize;
use eframe::egui::{self, Color32};
use egui_extras::{Column, TableBuilder};
use indexmap::IndexMap;
use lazuli::system::eabi::CallStack;
use lazuli::system::os::Thread;
use lazuli::{Address, system};
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[derive(Debug)]
enum ThreadKind {
    System,
    Normal,
}

struct ThreadInfo {
    kind: ThreadKind,
    thread: Thread,
    orphan: bool,
    call_stack: Option<CallStack>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    #[serde(skip)]
    threads: IndexMap<Address, ThreadInfo>,
    #[serde(skip)]
    current: Option<Address>,
    #[serde(skip)]
    selected: usize,
}

#[typetag::serde(name = "os-threads")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "OS Threads"
    }

    fn prepare(&mut self, state: &mut State) {
        let Some(threads) = system::os::system_threads(&state.lazuli.sys) else {
            return;
        };

        for thread in self.threads.values_mut() {
            thread.orphan = true;
        }

        for thread in threads.active {
            self.threads.insert(
                thread.addr,
                ThreadInfo {
                    kind: ThreadKind::Normal,
                    thread,
                    orphan: false,
                    call_stack: None,
                },
            );
        }

        self.current = threads.current.as_ref().map(|t| t.addr);
        if let Some(current) = threads.current {
            self.threads.insert(
                current.addr,
                ThreadInfo {
                    kind: ThreadKind::Normal,
                    thread: current,
                    orphan: false,
                    call_stack: None,
                },
            );
        }

        self.threads.insert(
            threads.default.addr,
            ThreadInfo {
                kind: ThreadKind::System,
                thread: threads.default,
                orphan: false,
                call_stack: None,
            },
        );

        for thread in self.threads.values_mut() {
            thread.call_stack = if thread.orphan {
                None
            } else if self.current.is_some_and(|c| c == thread.thread.addr) {
                let sp = Address(state.lazuli.sys.cpu.user.gpr[1]);
                let pc = state.lazuli.sys.cpu.pc;
                Some(system::eabi::call_stack(&state.lazuli.sys, sp, pc))
            } else {
                Some(system::eabi::call_stack(
                    &state.lazuli.sys,
                    thread.thread.data.context.sp,
                    thread.thread.data.context.srr0,
                ))
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let selected = self.threads.get_index(self.selected);

            ui.horizontal_wrapped(|ui| {
                let selected_label = selected.map_or("None".into(), |(a, _)| a.to_string());
                egui::ComboBox::from_label("Thread")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for (index, (address, thread)) in self.threads.iter().enumerate() {
                            let label = egui::RichText::new(format!(
                                "{} [{:02}] ({:?}, {:?})",
                                address,
                                thread.thread.data.priority,
                                thread.kind,
                                thread.thread.data.state
                            ))
                            .family(egui::FontFamily::Monospace)
                            .color(if thread.orphan {
                                Color32::RED
                            } else {
                                Color32::GRAY
                            });

                            ui.selectable_value(&mut self.selected, index, label);
                        }
                    });

                if let Some((_, info)) = selected {
                    let t = &info.thread.data;
                    let yes_or_no = |b| if b { "Yes" } else { "No" };
                    ui.label(format!("State: {:?}", t.state));
                    ui.label(format!("Detached: {}", yes_or_no(t.detached)));
                    ui.label(format!("Suspended: {}", yes_or_no(t.suspended)));
                    ui.label(format!("Priority: {} ({})", t.priority, t.base_priority));
                    ui.label(format!("Stack size: {}", ByteSize(t.stack_size() as u64)));
                    ui.label(format!("Error: {}", t.error));
                }
            });

            ui.separator();

            if let Some((_, info)) = selected
                && let Some(call_stack) = &info.call_stack
            {
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
                    for call in call_stack.0.iter().rev() {
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
            }
        });
    }
}
