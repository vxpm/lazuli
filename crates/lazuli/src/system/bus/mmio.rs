use crate::Address;

/// Allows the usage of const values in patterns. It's a neat trick!
struct ConstTrick<const N: u16>;
impl<const N: u16> ConstTrick<N> {
    const OUTPUT: u16 = N;
}

macro_rules! mmio {
    ($($addr:expr, $size:expr, $name:ident);* $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u32)]
        pub enum Mmio {
            $(
                $name = ($size << 16) | $addr
            ),*
        }

        impl Mmio {
            #[inline(always)]
            pub fn address(self) -> Address {
                Address(0x0C00_0000 | (self as u32 & 0xFFFF))
            }

            #[inline(always)]
            pub fn size(self) -> u32 {
                (self as u32) >> 16
            }

            /// Given an offset into the `0x0C00_0000` region, returns the MMIO register at that
            /// address and the offset into it.
            pub fn find(offset: u16) -> Option<(Self, usize)> {
                match offset {
                    $(
                        $addr..ConstTrick::<{ $addr + $size }>::OUTPUT => Some((Self::$name, (offset - $addr) as usize)),
                    )*
                    _ => None,
                }
            }
        }
    };
}

mmio! {
    // OFFSET, LENGTH, NAME;

    // === Command Processor ===
    0x0000, 2, CpStatus;
    0x0002, 2, CpControl;
    0x0004, 2, CpClear;
    0x0020, 2, CpFifoStartLow;
    0x0022, 2, CpFifoStartHigh;
    0x0024, 2, CpFifoEndLow;
    0x0026, 2, CpFifoEndHigh;
    0x0028, 2, CpHighWatermarkLow;
    0x002A, 2, CpHighWatermarkHigh;
    0x002C, 2, CpLowWatermarkLow;
    0x002E, 2, CpLowWatermarkHigh;
    0x0030, 2, CpFifoCountLow;
    0x0032, 2, CpFifoCountHigh;
    0x0034, 2, CpFifoWritePtrLow;
    0x0036, 2, CpFifoWritePtrHigh;
    0x0038, 2, CpFifoReadPtrLow;
    0x003A, 2, CpFifoReadPtrHigh;
    0x003C, 2, CpFifoBreakpointLow;
    0x003E, 2, CpFifoBreakpointHigh;

    // === Pixel Engine ===
    0x100A, 2, PixelInterruptStatus;
    0x100E, 2, PixelToken;

    // === Video Interface ===
    0x2000, 2, VideoVerticalTiming;
    0x2002, 2, VideoDisplayConfig;
    0x2004, 8, VideoHorizontalTiming;
    0x200C, 4, VideoOddVerticalTiming;
    0x2010, 4, VideoEvenVerticalTiming;
    0x2014, 4, VideoOddBbInterval;
    0x2018, 4, VideoEvenBbInterval;
    0x201C, 4, VideoTopBaseLeft;
    0x2020, 4, VideoTopBaseRight;
    0x2024, 4, VideoBottomBaseLeft;
    0x2028, 4, VideoBottomBaseRight;
    0x202C, 2, VideoVerticalCount;
    0x202E, 2, VideoHorizontalCount;
    0x2030, 4, VideoDisplayInterrupt0;
    0x2034, 4, VideoDisplayInterrupt1;
    0x2038, 4, VideoDisplayInterrupt2;
    0x203C, 4, VideoDisplayInterrupt3;
    0x2048, 2, VideoExternalFramebufferWidth;
    0x204A, 2, VideoHorizontalScaling;

    // Filter Coefficient Table
    0x204C, 4, VideoFilterCoeff0;
    0x2050, 4, VideoFilterCoeff1;
    0x2054, 4, VideoFilterCoeff2;
    0x2058, 4, VideoFilterCoeff3;
    0x205C, 4, VideoFilterCoeff4;
    0x2060, 4, VideoFilterCoeff5;
    0x2064, 4, VideoFilterCoeff6;

    0x206C, 2, VideoClock;
    0x206E, 2, VideoDtvStatus;
    0x2070, 2, VideoUnknown2070;

    // === Processor Interface ===
    0x3000, 4, ProcessorInterruptCause;
    0x3004, 4, ProcessorInterruptMask;
    0x300C, 4, ProcessorFifoStart;
    0x3010, 4, ProcessorFifoEnd;
    0x3014, 4, ProcessorFifoCurrent;
    0x3024, 4, ProcessorDvdReset;
    0x302C, 4, ProcessorConsoleType;

    // === Memory Interface ===
    0x4010, 2, MemoryProtection;
    0x401C, 2, MemoryInterruptMask;
    0x4020, 2, MemoryInterrupt;

    // === DSP Interface ===
    0x5000, 4, DspSendMailbox;
    0x5004, 4, DspRecvMailbox;
    0x500A, 2, DspControl;
    0x5012, 2, DspAramSize;
    0x5016, 2, DspAramMode;
    0x501A, 2, DspAramRefresh;
    0x5020, 4, DspAramDmaRamBase;
    0x5024, 4, DspAramDmaAramBase;
    0x5028, 4, DspAramDmaControl;
    0x5030, 4, AudioDmaBase;
    0x5036, 2, AudioDmaControl;
    0x503A, 2, AudioDmaRemaining;

    // === Disk Interface ===
    0x6000, 4, DiskStatus;
    0x6004, 4, DiskCover;
    0x6008, 4, DiskCommand0;
    0x600C, 4, DiskCommand1;
    0x6010, 4, DiskCommand2;
    0x6014, 4, DiskDmaBase;
    0x6018, 4, DiskDmaLength;
    0x601C, 4, DiskControl;
    0x6020, 4, DiskImmediateData;
    0x6024, 4, DiskConfiguration;

    // === Serial Interface ===
    0x6400, 4, SerialOutputBuf0;
    0x6404, 4, SerialInput0High;
    0x6408, 4, SerialInput0Low;

    0x640C, 4, SerialOutputBuf1;
    0x6410, 4, SerialInput1High;
    0x6414, 4, SerialInput1Low;

    0x6418, 4, SerialOutputBuf2;
    0x641C, 4, SerialInput2High;
    0x6420, 4, SerialInput2Low;

    0x6424, 4, SerialOutputBuf3;
    0x6428, 4, SerialInput3High;
    0x642C, 4, SerialInput3Low;

    0x6430, 4, SerialPoll;
    0x6434, 4, SerialCommControl;
    0x6438, 4, SerialStatus;
    0x643C, 4, SerialExiClock;
    0x6480, 128, SerialBuffer;

    // === External Interface ===
    0x6800, 4, ExiChannel0Param;
    0x6804, 4, ExiChannel0DmaBase;
    0x6808, 4, ExiChannel0DmaLength;
    0x680C, 4, ExiChannel0Control;
    0x6810, 4, ExiChannel0Immediate;

    0x6814, 4, ExiChannel1Param;
    0x6818, 4, ExiChannel1DmaBase;
    0x681C, 4, ExiChannel1DmaLength;
    0x6820, 4, ExiChannel1Control;
    0x6824, 4, ExiChannel1Immediate;

    0x6828, 4, ExiChannel2Param;
    0x682C, 4, ExiChannel2DmaBase;
    0x6830, 4, ExiChannel2DmaLength;
    0x6834, 4, ExiChannel2Control;
    0x6838, 4, ExiChannel2Immediate;

    // === Audio Interface ===
    0x6C00, 4, AudioControl;
    0x6C04, 4, AudioVolume;
    0x6C08, 4, AudioSampleCounter;
    0x6C0C, 4, AudioInterruptSample;

    // === Fake STDOUT ===
    0x7000, 1, FakeStdout;

    // === PI FIFO===
    0x8000, 32, ProcessorFifo;
}

impl Mmio {
    pub(super) fn log_reads(self) -> bool {
        !matches!(
            self,
            Mmio::DiskControl
                | Mmio::DspSendMailbox
                | Mmio::DspRecvMailbox
                | Mmio::ProcessorInterruptCause
        )
    }
}
