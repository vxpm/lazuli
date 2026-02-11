use eframe::egui::{self};
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

// #[inline]
// fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
//     let (y, cb, cr) = (y as f32, cb as f32 - 128.0, cr as f32 - 128.0);
//
//     let r = y + 1.371 * cr;
//     let g = y - 0.698 * cr - 0.336 * cb;
//     let b = y + 1.732 * cb;
//
//     [r, g, b].map(|x| x.clamp(0.0, 255.0) as u8)
// }

#[derive(Default, Serialize, Deserialize)]
pub struct Window {}

#[typetag::serde(name = "xfb")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "XFB"
    }

    fn prepare(&mut self, _: &mut State) {}

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        ui.label("stub");
    }
}
