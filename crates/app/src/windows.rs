mod call_stack;
mod control;
mod disasm;
mod display;
mod registers;
mod renderer_info;
mod subsystem;
mod threads;
mod variables;
mod xfb;

use eframe::egui::{self, Vec2};
use renderer::Renderer;
use serde::{Deserialize, Serialize};

use crate::runner::State;

pub struct Ctx<'a> {
    pub step: bool,
    pub running: bool,
    pub renderer: &'a mut Renderer,
}

#[typetag::serde]
pub(crate) trait AppWindow: 'static {
    fn title(&self) -> &str;
    fn default_size(&self) -> Option<Vec2> {
        None
    }
    fn prepare(&mut self, state: &mut State);
    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut Ctx);
}

#[derive(Serialize, Deserialize)]
pub struct AppWindowState {
    pub id: egui::Id,
    pub open: bool,
    pub window: Box<dyn AppWindow>,
}

pub fn control() -> control::Window {
    Default::default()
}

pub fn disasm() -> disasm::Window {
    Default::default()
}

pub fn registers() -> registers::Window {
    Default::default()
}

pub fn call_stack() -> call_stack::Window {
    Default::default()
}

pub fn os_threads() -> threads::Window {
    Default::default()
}

pub fn variables() -> variables::Window {
    Default::default()
}

// pub fn xfb() -> xfb::Window {
//     Default::default()
// }

pub fn display() -> display::Window {
    Default::default()
}

pub fn renderer() -> renderer_info::Window {
    Default::default()
}

pub fn subsystem_cp() -> subsystem::cp::Window {
    Default::default()
}

pub fn subsystem_pi() -> subsystem::pi::Window {
    Default::default()
}
