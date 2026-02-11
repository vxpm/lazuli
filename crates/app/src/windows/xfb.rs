use eframe::egui::{self, Vec2};
use lazuli::system;
use serde::{Deserialize, Serialize};

use crate::State;
use crate::windows::{AppWindow, Ctx};

#[inline]
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (y as f32, cb as f32 - 128.0, cr as f32 - 128.0);

    let r = y + 1.371 * cr;
    let g = y - 0.698 * cr - 0.336 * cb;
    let b = y + 1.732 * cb;

    [r, g, b].map(|x| x.clamp(0.0, 255.0) as u8)
}

#[derive(Default, Serialize, Deserialize)]
pub struct Window {
    #[serde(skip)]
    bottom: bool,
    #[serde(skip)]
    xfb_enabled: bool,
    #[serde(skip)]
    xfb_resolution: (u16, u16),
    #[serde(skip)]
    xfb_data: Vec<u8>,
    #[serde(skip)]
    texture: Option<egui::TextureHandle>,
}

#[typetag::serde(name = "xfb")]
impl AppWindow for Window {
    fn title(&self) -> &str {
        "XFB"
    }

    fn prepare(&mut self, state: &mut State) {
        let emulator = &state.lazuli;
        if !emulator.sys.video.display_config.enable() {
            self.xfb_enabled = false;
            return;
        }

        let dims = emulator.sys.video.xfb_dimensions();
        self.xfb_resolution = (dims.width, dims.height);

        let Some(xfb) = (if self.bottom {
            system::vi::bottom_xfb(&emulator.sys)
        } else {
            system::vi::top_xfb(&emulator.sys)
        }) else {
            return;
        };

        self.xfb_data.clear();
        self.xfb_data.extend_from_slice(xfb);
    }

    fn show(&mut self, ui: &mut egui::Ui, _: &mut Ctx) {
        let texture = match &mut self.texture {
            Some(tex) => tex,
            None => {
                let tex = ui.ctx().load_texture(
                    "xfb",
                    egui::ColorImage::example(),
                    egui::TextureOptions::LINEAR,
                );
                self.texture = Some(tex.clone());
                self.texture.as_mut().unwrap()
            }
        };

        let resolution = self.xfb_resolution;
        if resolution.0 == 0 || resolution.1 == 0 {
            ui.label("VI bad resolution");
            return;
        }

        let mut pixels = Vec::with_capacity(self.xfb_data.len() / 2);
        for ycbcr in self.xfb_data.chunks_exact(4) {
            let [r, g, b] = ycbcr_to_rgb(ycbcr[0], ycbcr[1], ycbcr[3]);
            pixels.push(egui::Color32::from_rgb(r, g, b));

            let [r, g, b] = ycbcr_to_rgb(ycbcr[2], ycbcr[1], ycbcr[3]);
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }

        let size = [resolution.0 as usize, resolution.1 as usize];
        let source_size = egui::Vec2::new(size[0] as f32, size[1] as f32);
        texture.set(
            egui::ColorImage {
                size,
                source_size,
                pixels,
            },
            egui::TextureOptions::LINEAR,
        );

        egui::Frame::canvas(ui.style()).show(ui, |ui| {
            let aspect_ratio = 4.0 / 3.0;
            let available_height = (ui.available_height() - 20.0).max(0.0);

            let size = if ui.available_width() < available_height {
                Vec2::new(ui.available_width(), ui.available_width() / aspect_ratio)
            } else {
                Vec2::new(available_height * aspect_ratio, available_height)
            };

            let tex_size = texture.size_vec2();
            let sized_texture = egui::load::SizedTexture::new(texture, tex_size);
            ui.add(egui::Image::new(sized_texture).fit_to_exact_size(size));
        });
    }
}
