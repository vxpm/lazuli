#![feature(trim_prefix_suffix)]

mod cli;
mod runner;
mod windows;

use std::io::BufReader;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use eframe::egui;
use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use eyre_pretty::eyre::Result;
use lazuli::Lazuli;
use lazuli::cores::Cores;
use lazuli::disks::rvz::Rvz;
use lazuli::modules::debug::{DebugModule, NopDebugModule};
use lazuli::modules::disk::{DiskModule, NopDiskModule};
use lazuli::system::executable::Executable;
use lazuli::system::{self, Modules};
use modules::audio::CpalModule;
use modules::debug::{Addr2LineModule, MapFileModule};
use modules::disk::{IsoModule, RvzModule};
use modules::input::GilrsModule;
use nanorand::Rng;
use renderer::Renderer;
use runner::State;
use vtxjit::JitVertexModule;

use crate::runner::Runner;
use crate::windows::{AppWindow, AppWindowState};

struct App {
    last_update: Instant,
    renderer: Renderer,
    input: GilrsModule,
    windows: Vec<AppWindowState>,
    runner: Runner,
    cps: u64,
    organize: bool,
}

impl App {
    #[allow(clippy::default_constructed_unit_structs)]
    fn new(cc: &eframe::CreationContext<'_>, cfg: &cli::Config) -> Result<Self> {
        tracing::info!("starting app setup");

        let ipl = if let Some(path) = &cfg.ipl {
            Some(std::fs::read(path)?)
        } else {
            None
        };

        let disk: Box<dyn DiskModule> = if let Some(path) = &cfg.rom {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap();
            match extension {
                "iso" => {
                    let file = std::fs::File::open(path)?;
                    let reader = BufReader::new(file);
                    Box::new(IsoModule(Some(reader)))
                }
                "rvz" => {
                    let file = std::fs::File::open(path)?;
                    let reader = BufReader::new(file);
                    let rvz = Rvz::new(reader).unwrap();
                    let rvz = RvzModule::new(rvz);
                    Box::new(rvz)
                }
                _ => todo!(),
            }
        } else {
            Box::new(NopDiskModule)
        };

        let executable = if let Some(path) = &cfg.exec {
            Some(Executable::open(path)?)
        } else {
            None
        };

        // this is a mess lol
        let debug_module = if let Some(path) = cfg.debug.as_deref() {
            match path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .as_deref()
            {
                Some("elf") => {
                    let debug = Addr2LineModule::new(path);
                    debug.map_or_else(
                        || Box::new(NopDebugModule) as Box<dyn DebugModule>,
                        |d| Box::new(d) as Box<dyn DebugModule>,
                    )
                }
                Some("map") => Box::new(MapFileModule::new(path)) as Box<dyn DebugModule>,
                _ => Box::new(NopDebugModule),
            }
        } else {
            Box::new(NopDebugModule)
        };

        let wgpu_state = cc.wgpu_render_state.as_ref().unwrap();
        tracing::info!("wgpu device limits: {:?}", wgpu_state.device.limits());

        let renderer = Renderer::new(
            wgpu_state.device.clone(),
            wgpu_state.queue.clone(),
            wgpu_state.target_format,
        );

        let dirs = directories::ProjectDirs::from("", "", "lazuli").unwrap();
        let cache_dir = dirs.cache_dir();
        let jit_cache_path = cache_dir.join("ppcjit");

        if cfg.ppcjit.clear_cache {
            _ = std::fs::remove_dir_all(&jit_cache_path);
        }

        let cores = Cores {
            dsp: Box::new(cores::dsp::interpreter::Core::default()),
            cpu: Box::new(cores::cpu::jit::Core::new(cores::cpu::jit::Config {
                instr_per_block: cfg.ppcjit.instr_per_block,
                jit_settings: cores::cpu::jit::ppcjit::Settings {
                    codegen: cores::cpu::jit::ppcjit::CodegenSettings {
                        nop_syscalls: cfg.ppcjit.nop_syscalls,
                        force_fpu: cfg.ppcjit.force_fpu,
                        ignore_unimplemented: cfg.ppcjit.ignore_unimplemented_inst,
                        round_to_single: cfg.ppcjit.round_to_single,
                    },
                    cache_path: Some(jit_cache_path),
                },
            })),
        };

        let input = GilrsModule::new();
        let modules = Modules {
            audio: Box::new(CpalModule::new()),
            debug: debug_module,
            disk,
            input: Box::new(input.clone()),
            render: Box::new(renderer.clone()),
            vertex: Box::new(JitVertexModule::new()),
        };

        let lazuli = Lazuli::new(
            cores,
            modules,
            system::Config {
                ipl_lle: cfg.ipl_lle,
                ipl,
                sideload: executable,
                perform_efb_copies: false,
            },
        );

        let mut runner = runner::Runner::new(lazuli);
        if cfg.run {
            runner.start();
        }

        let windows: Option<Vec<AppWindowState>> = cc
            .storage
            .as_ref()
            .and_then(|s| s.get_string("windows"))
            .and_then(|s| ron::from_str(&s).ok());

        let (windows, create_default) = if let Some(windows) = windows {
            (windows, false)
        } else {
            (vec![], true)
        };

        let mut app = Self {
            last_update: Instant::now(),
            renderer,
            input,
            windows,
            runner,
            cps: 0,
            organize: false,
        };

        if create_default {
            app.create_window(windows::disasm());
            app.create_window(windows::control());
            app.create_window(windows::call_stack());
            app.create_window(windows::display());
            app.organize = true;
        }

        // if ui.button("Organize windows").clicked() {
        //     ui.memory_mut(|mem| mem.reset_areas());
        // }

        cc.egui_ctx.set_zoom_factor(1.0);

        Ok(app)
    }

    fn create_window(&mut self, window: impl AppWindow) {
        let mut rng = nanorand::tls_rng();
        let id = rng.generate::<u64>();
        self.windows.push(AppWindowState {
            id: egui::Id::new(id),
            open: true,
            window: Box::new(window),
        });
    }
}

const FRAMETIME: Duration = Duration::new(0, (1_000_000_000.0 / 60.0) as u32);

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label("Lazuli");
                ui.menu_button("🗖 View", |ui| {
                    if ui.button("Control").clicked() {
                        self.create_window(windows::control());
                    }

                    if ui.button("Disassembly").clicked() {
                        self.create_window(windows::disasm());
                    }

                    if ui.button("Registers").clicked() {
                        self.create_window(windows::registers());
                    }

                    if ui.button("Call Stack").clicked() {
                        self.create_window(windows::call_stack());
                    }

                    if ui.button("OS Threads").clicked() {
                        self.create_window(windows::os_threads());
                    }

                    if ui.button("Variables").clicked() {
                        self.create_window(windows::variables());
                    }

                    if ui.button("Display").clicked() {
                        self.create_window(windows::display());
                    }

                    if ui.button("Renderer").clicked() {
                        self.create_window(windows::renderer());
                    }

                    ui.menu_button("Subsystems", |ui| {
                        if ui.button("Command Processor").clicked() {
                            self.create_window(windows::subsystem_cp());
                        }

                        if ui.button("Processor Interface").clicked() {
                            self.create_window(windows::subsystem_pi());
                        }
                    });
                });

                ui.label(format!(
                    "Speed: {}%",
                    ((self.cps as f64 / lazuli::gekko::FREQUENCY as f64) * 100.0).round()
                ));
            });
        });

        let was_running = self.runner.stop();
        self.runner.clear_breakpoint();

        {
            let mut state = self.runner.get();
            for window_state in &mut self.windows {
                window_state.window.prepare(&mut state);
            }

            self.cps = state
                .cycles_history
                .iter()
                .map(|c| c.0.value())
                .sum::<u64>()
                * 2;
        }

        ctx.input(|i| {
            let button = |key| i.key_down(key);
            let trigger = |key| if i.key_down(key) { 255 } else { 0 };
            let axis = |low, high| match (i.key_down(low), i.key_down(high)) {
                (true, false) => 0,
                (false, true) => 255,
                _ => 128,
            };

            self.input.update_fallback(|s| {
                s.analog_x = axis(egui::Key::A, egui::Key::D);
                s.analog_y = axis(egui::Key::S, egui::Key::W);
                s.analog_sub_x = axis(egui::Key::H, egui::Key::L);
                s.analog_sub_y = axis(egui::Key::J, egui::Key::K);
                s.analog_trigger_left = trigger(egui::Key::Q);
                s.analog_trigger_right = trigger(egui::Key::E);
                s.trigger_z = button(egui::Key::R);
                s.trigger_left = button(egui::Key::T);
                s.trigger_right = button(egui::Key::Y);
                s.pad_left = button(egui::Key::ArrowLeft);
                s.pad_right = button(egui::Key::ArrowRight);
                s.pad_down = button(egui::Key::ArrowDown);
                s.pad_up = button(egui::Key::ArrowUp);
                s.button_a = button(egui::Key::B);
                s.button_b = button(egui::Key::N);
                s.button_x = button(egui::Key::C);
                s.button_y = button(egui::Key::V);
                s.button_start = button(egui::Key::Space);
            });
        });

        if was_running {
            self.runner.start();
        }

        let mut context = windows::Ctx {
            step: false,
            running: was_running,
            renderer: &mut self.renderer,
        };

        egui::CentralPanel::default().show(ctx, |_| {
            let mut close = None;
            for (index, window_state) in self.windows.iter_mut().enumerate() {
                let mut open = true;
                let mut window = egui::Window::new(window_state.window.title())
                    .id(window_state.id)
                    .open(&mut open)
                    .resizable(true)
                    .min_size(egui::Vec2::ZERO);

                if let Some(size) = window_state.window.default_size() {
                    window = window.default_size(size);
                }

                window.show(ctx, |ui| {
                    window_state.window.show(ui, &mut context);
                });

                if !open {
                    close = Some(index);
                }
            }

            if let Some(close) = close {
                self.windows.remove(close);
            }
        });

        if context.running != was_running {
            if context.running {
                self.runner.start();
            } else {
                self.runner.stop();
            }
        }

        if context.step {
            self.runner.step();
        }

        let remaining = FRAMETIME.saturating_sub(self.last_update.elapsed());
        ctx.request_repaint_after(remaining);
        self.last_update = Instant::now() + remaining;

        if std::mem::replace(&mut self.organize, false) {
            ctx.request_discard("organize");
            ctx.memory_mut(|mem| mem.reset_areas());
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let windows = self.windows.iter().collect::<Vec<_>>();
        storage.set_string("windows", ron::to_string(&windows).unwrap());
    }
}

fn setup_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, fmt};

    let file = std::fs::File::options()
        .truncate(true)
        .create(true)
        .write(true)
        .open("log.log")
        .unwrap();

    let (file_nb, _guard_file) = tracing_appender::non_blocking(file);
    let file_layer = fmt::layer().with_writer(file_nb).with_ansi(false);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(
        "cli=debug,lazuli=debug,lazuli::system::gx=info,common=debug,ppcjit=debug,renderer=debug,dspint=debug,cores=debug,modules=debug",
    ));

    let subscriber = tracing_subscriber::registry()
        .with(file_layer)
        .with(env_filter);

    subscriber.init();

    _guard_file
}

fn main() -> Result<()> {
    eyre_pretty::install()?;
    let _tracing_guard = setup_tracing();
    let cfg = cli::Config::parse();

    let device_descriptor = Arc::new(|adapter: &wgpu::Adapter| {
        let info = adapter.get_info();

        let mut required_features = wgpu::Features::empty();
        required_features |= wgpu::Features::DUAL_SOURCE_BLENDING;
        required_features |= wgpu::Features::FLOAT32_FILTERABLE;
        required_features |= wgpu::Features::PUSH_CONSTANTS;
        required_features |= wgpu::Features::CLEAR_TEXTURE;

        if matches!(
            info.device_type,
            wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu
        ) {
            required_features |= wgpu::Features::MAPPABLE_PRIMARY_BUFFERS;
        }

        let mut required_limits = wgpu::Limits::defaults();
        required_limits.max_texture_dimension_2d = 8192;
        required_limits.max_push_constant_size = 64 + 32;

        wgpu::DeviceDescriptor {
            label: Some("lazuli wgpu device"),
            required_features,
            required_limits,
            ..Default::default()
        }
    });

    let icon = eframe::icon_data::from_png_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/logo_256.png"
    )))
    .unwrap();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_icon(icon),
        wgpu_options: WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(WgpuSetupCreateNew {
                instance_descriptor: wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::PRIMARY,
                    ..Default::default()
                },
                power_preference: wgpu::PowerPreference::HighPerformance,
                device_descriptor,
                ..Default::default()
            }),
            ..Default::default()
        },
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "Lazuli",
        options,
        Box::new(|cc| {
            let app = App::new(cc, &cfg)?;
            Ok(Box::new(app))
        }),
    )?;

    Ok(())
}
