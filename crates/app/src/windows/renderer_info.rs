use bytesize::ByteSize;
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[cfg(not(target_os = "macos"))]
type RenderDoc = renderdoc::RenderDoc<renderdoc::V140>;

#[derive(Serialize, Deserialize)]
pub struct Window {

    #[cfg(not(target_os = "macos"))]
    #[serde(skip)]
    renderdoc: Option<RenderDoc>,
    #[serde(skip)]
    capture: bool,
    #[serde(skip)]
    is_capturing: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            #[cfg(not(target_os = "macos"))]
            renderdoc: RenderDoc::new().ok(),
            capture: false,
            is_capturing: false,
        }
    }
}

#[typetag::serde(name = "renderer_info")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "Renderer"
    }

    fn prepare(&mut self, _: &mut State) {}

    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut Ctx) {
        let stats = ctx.renderer.stats();

        ui.vertical(|ui| {
            ui.heading("Allocator Report");
            if let Some(alloc) = &stats.alloc {
                ui.label(format!(
                    "Allocated: {}",
                    ByteSize(alloc.total_allocated_bytes)
                ));

                ui.label(format!(
                    "Reserved: {}",
                    ByteSize(alloc.total_reserved_bytes)
                ));
            } else {
                ui.label("Report unavailable");
            }

            let counters = &stats.counters.hal;
            ui.heading("Counters");
            ui.label(format!(
                "Buffers: {} ({})",
                counters.buffers.read(),
                ByteSize(counters.buffer_memory.read() as u64),
            ));
            ui.label(format!(
                "Textures: {} ({})",
                counters.textures.read(),
                ByteSize(counters.texture_memory.read() as u64),
            ));
            ui.label(format!("Samplers: {}", counters.samplers.read(),));
            ui.label(format!("Shaders: {}", counters.shader_modules.read(),));
            ui.label(format!("Pipelines: {}", counters.render_pipelines.read(),));
            ui.label(format!("Bind Groups: {}", counters.bind_groups.read(),));
            ui.label(format!(
                "Command Encoders: {}",
                counters.command_encoders.read(),
            ));
            ui.label(format!(
                "Memory Allocations: {}",
                counters.memory_allocations.read(),
            ));

            ui.heading("Renderdoc");

            #[cfg(not(target_os = "macos"))]
            if let Some(renderdoc) = &mut self.renderdoc {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.capture, "Capture (Renderdoc)");

                    let null = std::ptr::null();
                    if self.is_capturing {
                        if ctx.renderer.rendered_anything() {
                            renderdoc.end_frame_capture(null, null);
                            self.is_capturing = false;
                        } else {
                            renderdoc.discard_frame_capture(null, null);
                            renderdoc.start_frame_capture(null, null);
                        }
                    }

                    if self.capture && !self.is_capturing {
                        ctx.renderer.rendered_anything();
                        renderdoc.start_frame_capture(null, null);
                        self.is_capturing = true;
                    }
                });
            } else {
                self.renderdoc = RenderDoc::new().ok();
                ui.label("Renderdoc not detected");
            }
        });
    }
}
