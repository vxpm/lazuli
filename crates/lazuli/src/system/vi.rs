//! Video interface (VI).
use bitos::bitos;
use bitos::integer::{u4, u7, u9, u10, u24};
use gekko::{Address, FREQUENCY};

use crate::modules::render;
use crate::system::{System, pi, si};

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VerticalTiming {
    /// Length of the equalization pulse, in halflines.
    #[bits(0..4)]
    pub equalization_pulse: u4,
    /// Amount of lines in the active video of a field.
    #[bits(4..14)]
    pub active_video_lines: u10,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, Default)]
pub enum DisplayLatchMode {
    #[default]
    Off    = 0,
    Once   = 1,
    Twice  = 2,
    Always = 3,
}

#[bitos(2)]
#[derive(Debug, Clone, Copy, Default)]
pub enum VideoFormat {
    #[default]
    NTSC  = 0,
    Pal50 = 1,
    Pal60 = 2,
    Debug = 3,
}

#[bitos(1)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldMode {
    /// Interlaced rendering: both fields are used.
    #[default]
    Double = 0,
    /// Non-interlaced rendering: only one field is used. Also known as "double-strike".
    Single = 1,
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayConfig {
    /// Enable video timing generation and data request.
    #[bits(0)]
    pub enable: bool,
    /// Clears all data requests and puts the interface into its idle state.
    #[bits(1)]
    pub reset: bool,
    /// The current field mode.
    #[bits(2)]
    pub field_mode: FieldMode,
    /// Current video format.
    #[bits(8..10)]
    pub video_format: VideoFormat,
}

#[bitos(64)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HorizontalTiming {
    // HTR1
    /// Width of the HSync pulse, in samples.
    #[bits(0..7)]
    pub sync_width: u7,
    /// Amount of samples between the start of HSync pulse and HBlank end.
    #[bits(7..17)]
    pub sync_start_to_blank_end: u10,
    /// Amount of samples between the half of the line and HBlank start.
    #[bits(17..27)]
    pub halfline_to_blank_start: u10,

    // HTR0
    /// Width of a halfline, in samples.
    #[bits(32..41)]
    pub halfline_width: u9,
    /// Amount of samples between the start of HSync pulse and color burst end.
    #[bits(48..55)]
    pub sync_start_to_color_burst_end: u7,
    /// Amount of samples between the start of HSync pulse and color burst start.
    #[bits(56..63)]
    pub sync_start_to_color_burst_start: u7,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FieldVerticalTiming {
    /// Length of the pre-blanking interval in half-lines.
    #[bits(0..10)]
    pub pre_blanking: u10,
    /// Length of the post-blanking interval in half-lines.
    #[bits(16..26)]
    pub post_blanking: u10,
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FieldBase {
    /// Bits 0..24 of the XFB address for this field.
    #[bits(0..24)]
    pub xfb_address_base: u24,
    #[bits(24..28)]
    pub horizontal_offset: u4,
    /// If set, shifts XFB address right by 5.
    #[bits(28)]
    pub shift_xfb_addr: bool,
}

impl FieldBase {
    /// Physical address of the XFB for this field.
    pub fn xfb_address(&self) -> Address {
        Address((self.xfb_address_base().value()) >> (5 * self.shift_xfb_addr() as usize))
    }
}

#[bitos(32)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayInterrupt {
    /// Sample count for the interrupt.
    #[bits(0..9)]
    pub horizontal_count: u9,
    /// Line count for the interrupt.
    #[bits(16..26)]
    pub vertical_count: u10,
    /// Whether this interrupt is enabled.
    #[bits(28)]
    pub enable: bool,
    /// Whether this interrupt is asserted. Clear on write.
    #[bits(31)]
    pub status: bool,
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HorizontalScaling {
    #[bits(0..9)]
    pub step_size: u9,
    #[bits(12)]
    pub enabled: bool,
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ExternalFramebufferWidth {
    /// Stride of the XFB divided by 16.
    #[bits(0..8)]
    pub stride_div_16: u8,
    /// Width of the XFB divided by 16.
    #[bits(8..15)]
    pub width_div_16: u7,
}

impl ExternalFramebufferWidth {
    /// Stride of the XFB.
    pub fn stride(&self) -> u16 {
        self.stride_div_16() as u16 * 16
    }

    /// Width of the XFB.
    pub fn width(&self) -> u16 {
        self.width_div_16().value() as u16 * 16
    }
}

#[bitos(16)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ClockMode {
    #[bits(0)]
    pub double: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMode {
    NonInterlaced,
    Interlaced,
    Progressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Default)]
pub struct Interface {
    pub display_config: DisplayConfig,
    pub vertical_timing: VerticalTiming,
    pub horizontal_timing: HorizontalTiming,
    pub top_vertical_timing: FieldVerticalTiming,
    pub top_base_left: FieldBase,
    pub top_base_right: u32,
    pub bottom_vertical_timing: FieldVerticalTiming,
    pub bottom_base_left: FieldBase,
    pub bottom_base_right: u32,
    pub vertical_count: u16,
    pub horizontal_count: u16,
    pub interrupts: [DisplayInterrupt; 4],
    pub xfb_width: ExternalFramebufferWidth,
    pub horizontal_scaling: HorizontalScaling,
    pub clock: ClockMode,
}

impl Interface {
    /// The current video clock frequency.
    pub fn video_clock(&self) -> u32 {
        if self.clock.double() {
            54_000_000
        } else {
            27_000_000
        }
    }

    /// How many CPU cycles long a sample (~ pixel) is.
    pub fn cycles_per_sample(&self) -> u32 {
        2 * FREQUENCY as u32 / self.video_clock()
    }

    /// How many CPU cycles long a halfline is.
    pub fn cycles_per_halfline(&self) -> u32 {
        self.cycles_per_sample() * self.horizontal_timing.halfline_width().value() as u32
    }

    /// How many halflines long a top field is.
    pub fn halflines_per_top_field(&self) -> u32 {
        3 * self.vertical_timing.equalization_pulse().value() as u32
            + self.top_vertical_timing.pre_blanking().value() as u32
            + 2 * self.vertical_timing.active_video_lines().value() as u32
            + self.top_vertical_timing.post_blanking().value() as u32
    }

    /// How many halflines long a bottom field is.
    pub fn halflines_per_bottom_field(&self) -> u32 {
        3 * self.vertical_timing.equalization_pulse().value() as u32
            + self.bottom_vertical_timing.pre_blanking().value() as u32
            + 2 * self.vertical_timing.active_video_lines().value() as u32
            + self.bottom_vertical_timing.post_blanking().value() as u32
    }

    /// How many halflines long a single frame is.
    pub fn halflines_per_frame(&self) -> u32 {
        self.halflines_per_top_field()
            + if self.display_config.field_mode() == FieldMode::Double {
                self.halflines_per_bottom_field()
            } else {
                0
            }
    }

    /// How many CPU cycles long a top field is.
    pub fn cycles_per_top_field(&self) -> u32 {
        self.cycles_per_halfline() * self.halflines_per_top_field()
    }

    /// How many CPU cycles long an even field is.
    pub fn cycles_per_bottom_field(&self) -> u32 {
        self.cycles_per_halfline() * self.halflines_per_bottom_field()
    }

    /// How many lines long a frame is.
    pub fn lines_per_frame(&self) -> u32 {
        self.halflines_per_frame() / 2
    }

    /// How many times a field is rendered in a second, on average.
    pub fn field_rate(&self) -> f64 {
        let cycles_per_frame =
            (self.cycles_per_top_field() + self.cycles_per_bottom_field()) as f64 / 2.0;

        FREQUENCY as f64 / cycles_per_frame
    }

    /// The refresh rate of the video output, i.e. how many times a frame is rendered in a second.
    pub fn frame_rate(&self) -> f64 {
        match self.display_config.field_mode() {
            FieldMode::Double => self.field_rate() / 2.0,
            FieldMode::Single => self.field_rate(),
        }
    }

    /// Address of the XFB for the top field.
    pub fn top_xfb_address(&self) -> Address {
        self.top_base_left.xfb_address()
    }

    /// Address of the XFB for the bottom field.
    pub fn bottom_xfb_address(&self) -> Address {
        self.bottom_base_left.xfb_address()
    }

    /// Returns the current video mode.
    pub fn video_mode(&self) -> VideoMode {
        if self.clock.double() {
            return VideoMode::Progressive;
        }

        match self.display_config.field_mode() {
            FieldMode::Single => VideoMode::NonInterlaced,
            FieldMode::Double => VideoMode::Interlaced,
        }
    }

    /// Dimensions of an external framebuffer.
    pub fn xfb_dimensions(&self) -> Dimensions {
        let width = self.xfb_width.width();
        let height = self.vertical_timing.active_video_lines().value();

        Dimensions { width, height }
    }

    /// Stride of the rows in an external framebuffer, in pixels.
    pub fn xfb_stride(&self) -> u16 {
        // YCbYCr format has 2 pixels every 4 bytes
        self.xfb_width.stride() / 2
    }

    /// Dimensions of the entire frame, which may consist of either one or two extenral
    /// framebuffers.
    pub fn frame_dimensions(&self) -> Dimensions {
        let xfb = self.xfb_dimensions();
        match self.display_config.field_mode() {
            FieldMode::Double => Dimensions {
                width: xfb.width,
                height: xfb.height * 2,
            },
            FieldMode::Single => xfb,
        }
    }

    /// Height of the video output.
    fn video_height(&self) -> u16 {
        let active_lines = self.vertical_timing.active_video_lines().value();
        let height_multiplier = match self.display_config.field_mode() {
            FieldMode::Double => 2,
            FieldMode::Single => 1,
        };

        height_multiplier * active_lines
    }

    /// Width of the video output.
    fn video_width(&self) -> u16 {
        self.horizontal_timing.halfline_width().value()
            + self.horizontal_timing.halfline_to_blank_start().value()
            - self.horizontal_timing.sync_start_to_blank_end().value()
    }

    /// Dimensions of the video output.
    pub fn video_dimensions(&self) -> Dimensions {
        Dimensions {
            width: self.video_width(),
            height: self.video_height(),
        }
    }

    /// Dimensions of the region in the video output which contain image data.
    pub fn video_dimensions_cropped(&self) -> Dimensions {
        Dimensions {
            width: self.xfb_dimensions().width,
            height: self.video_height(),
        }
    }

    pub fn write_interrupt<const N: usize>(&mut self, new: DisplayInterrupt) {
        const { assert!(N < 4) };
        self.interrupts[N] = new.with_status(self.interrupts[N].status() && new.status());
    }
}

pub fn update_display_interrupts(sys: &mut System) {
    let video_width = sys.video.video_width();
    let mut raised = false;
    for interrupt in sys.video.interrupts.iter_mut() {
        interrupt.set_status(false);
        if !interrupt.enable() {
            continue;
        }

        if interrupt.horizontal_count().value() > video_width {
            continue;
        }

        sys.video.horizontal_count = sys
            .video
            .horizontal_count
            .max(interrupt.horizontal_count().value());

        if interrupt.vertical_count().value() == sys.video.vertical_count {
            raised = true;
            interrupt.set_status(true);
        }
    }

    if raised {
        pi::check_interrupts(sys);
    }
}

pub fn vertical_count(sys: &mut System) {
    self::update_display_interrupts(sys);

    let start_of_top_field = sys.video.vertical_count == 1;
    let start_of_bottom_field = sys.video.display_config.field_mode() == FieldMode::Double
        && sys.video.vertical_count as u32 == sys.video.lines_per_frame() / 2 + 1;

    if start_of_top_field || start_of_bottom_field {
        self::present(sys);
    }

    sys.video.vertical_count += 1;
    sys.video.horizontal_count = 1;

    if sys.video.vertical_count as u32 > sys.video.lines_per_frame() {
        sys.video.vertical_count = 1;
    }

    if sys
        .video
        .vertical_count
        .is_multiple_of(sys.serial.poll.x_lines().value())
    {
        si::poll_controller(sys, 0);
        si::poll_controller(sys, 1);
        si::poll_controller(sys, 2);
        si::poll_controller(sys, 3);
    }

    let cycles_per_frame = (FREQUENCY as f64 / sys.video.frame_rate()) as u32;
    let cycles_per_line = cycles_per_frame
        .checked_div(sys.video.lines_per_frame())
        .unwrap_or(cycles_per_frame);

    sys.scheduler
        .schedule(cycles_per_line as u64, self::vertical_count);
}

pub fn update(sys: &mut System) {
    if sys.video.vertical_count as u32 > sys.video.lines_per_frame() {
        sys.video.horizontal_count = 1;
        sys.video.vertical_count = 1;
    }

    sys.scheduler.cancel(self::vertical_count);
    if sys.video.display_config.enable() {
        sys.scheduler.schedule_now(self::vertical_count);
    }
}

pub fn present(sys: &mut System) {
    if sys.gpu.xfb_copies.is_empty() {
        return;
    }

    let frame_dimensions = sys.video.frame_dimensions();
    let stride_in_pixels = sys.video.xfb_stride() as u32;
    let base_copy = sys.gpu.xfb_copies.iter().min_by_key(|x| x.addr).unwrap();

    let mut parts = Vec::with_capacity(sys.gpu.xfb_copies.len());
    for (id, copy) in sys.gpu.xfb_copies.iter().enumerate() {
        let delta_pixels = (copy.addr.value() - base_copy.addr.value()) / 2;
        let offset_x = delta_pixels % stride_in_pixels;
        let offset_y = delta_pixels / stride_in_pixels;
        dbg!(copy, offset_y);

        if offset_x >= frame_dimensions.width as u32 || offset_y >= frame_dimensions.height as u32 {
            continue;
        }

        parts.push(render::XfbPart {
            id: id as u32,
            offset_x,
            offset_y,
        });
    }

    sys.modules.render.exec(render::Action::PresentXfb(parts));
    sys.gpu.xfb_copies.clear();
}
