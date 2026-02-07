#![expect(clippy::identity_op, reason = "seq expanded code")]
#![expect(clippy::erasing_op, reason = "seq expanded code")]
#![feature(portable_simd)]

use std::marker::PhantomData;
use std::simd::{ToBytes, simd_swizzle, u8x32, u16x16, u16x32};

use bitut::BitUtils;
use color::convert_range;
use multiversion::multiversion;
use seq_macro::seq;

#[rustfmt::skip]
pub use color;

pub type Pixel = color::Rgba8;
pub type PaletteIndex = u16;

pub trait Format {
    const TILE_WIDTH: usize;
    const TILE_HEIGHT: usize;
    const BYTES_PER_TILE: usize = 32;

    type Texel: Clone + Copy + Default;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Self::Texel);
    fn decode_tile(data: &[u8], set: impl FnMut(usize, usize, Self::Texel));
}

pub fn compute_size<F: Format>(width: usize, height: usize) -> usize {
    let width = width.div_ceil(F::TILE_WIDTH);
    let height = height.div_ceil(F::TILE_HEIGHT);
    width * height * F::BYTES_PER_TILE
}

/// Stride is in cache lines (32 bytes).
#[multiversion(targets = "simd")]
pub fn encode<F: Format>(
    stride: usize,
    width: usize,
    height: usize,
    data: &[F::Texel],
    buffer: &mut [u8],
) {
    assert!(buffer.len() >= compute_size::<F>(width, height));

    let cache_lines_per_tile = F::BYTES_PER_TILE / 32;
    let stride_in_tiles = stride / cache_lines_per_tile;
    let width_in_tiles = width.div_ceil(F::TILE_WIDTH);
    let height_in_tiles = height.div_ceil(F::TILE_HEIGHT);

    for tile_y in 0..height_in_tiles {
        for tile_x in 0..width_in_tiles {
            // where should data be written to?
            let tile_index = tile_y * stride_in_tiles + tile_x;
            let tile_offset = tile_index * F::BYTES_PER_TILE;
            let out = &mut buffer[tile_offset..][..F::BYTES_PER_TILE];

            // find pixels in this tile
            let base_x = tile_x * F::TILE_WIDTH;
            let base_y = tile_y * F::TILE_HEIGHT;
            F::encode_tile(out, |x, y| {
                assert!(x <= F::TILE_WIDTH);
                assert!(y <= F::TILE_HEIGHT);

                let x = base_x + x;
                let y = base_y + y;
                let image_index = y * width + x;
                data.get(image_index).copied().unwrap_or_default()
            });
        }
    }
}

#[multiversion(targets = "simd")]
pub fn decode<F: Format>(width: usize, height: usize, data: &[u8]) -> Vec<F::Texel> {
    let width_in_tiles = width.div_ceil(F::TILE_WIDTH);
    let height_in_tiles = height.div_ceil(F::TILE_HEIGHT);

    let full_width = width_in_tiles * F::TILE_WIDTH;
    let full_height = height_in_tiles * F::TILE_HEIGHT;
    assert!(data.len() >= compute_size::<F>(full_width, full_height));

    let mut texels = vec![F::Texel::default(); full_width * full_height];
    for tile_y in 0..height_in_tiles {
        for tile_x in 0..width_in_tiles {
            let tile_index = tile_y * width_in_tiles + tile_x;
            let tile_offset = tile_index * F::BYTES_PER_TILE;
            let tile_data = &data[tile_offset..][..F::BYTES_PER_TILE];

            let base_x = tile_x * F::TILE_WIDTH;
            let base_y = tile_y * F::TILE_HEIGHT;
            F::decode_tile(tile_data, |x, y, value| {
                assert!(x <= F::TILE_WIDTH);
                assert!(y <= F::TILE_HEIGHT);

                // since we're decoding from top to bottom, even if the index is in bounds but not
                // assigned to this coordinate, it will be overwritten later by the correct texel
                let x = base_x + x;
                let y = base_y + y;
                let image_index = y * width + x;

                // SAFETY: x and y are within tile width/height, and the texels buffer is big
                // enough to fit (height_in_tiles * width_in_tiles) tiles
                unsafe { *texels.get_unchecked_mut(image_index) = value };
            });
        }
    }

    texels.truncate(width * height);
    texels
}

pub trait ComponentSource {
    fn get(pixel: Pixel) -> u8;
}

pub struct RedChannel;

impl ComponentSource for RedChannel {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.r
    }
}

pub struct BlueChannel;

impl ComponentSource for BlueChannel {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.b
    }
}

pub struct GreenChannel;

impl ComponentSource for GreenChannel {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.g
    }
}

pub struct AlphaChannel;

impl ComponentSource for AlphaChannel {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.a
    }
}

pub struct Luma;

impl ComponentSource for Luma {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.y()
    }
}

pub struct FastLuma;

impl ComponentSource for FastLuma {
    #[inline(always)]
    fn get(pixel: Pixel) -> u8 {
        pixel.fast_y()
    }
}

pub struct I4<Source = Luma>(PhantomData<Source>);

impl<Source: ComponentSource> Format for I4<Source> {
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 8;

    type Texel = Pixel;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let intensity = convert_range::<255, 15>(Source::get(pixel));

                let index = y * Self::TILE_WIDTH + x;
                let current = data[index / 2];

                let new = if index % 2 == 0 {
                    current.with_bits(4, 8, intensity)
                } else {
                    current.with_bits(0, 4, intensity)
                };

                data[index / 2] = new;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let value = data[index / 2];
                let intensity = convert_range::<15, 255>(if index % 2 == 0 {
                    value.bits(4, 8)
                } else {
                    value.bits(0, 4)
                });

                set(
                    x,
                    y,
                    Pixel {
                        r: intensity,
                        g: intensity,
                        b: intensity,
                        a: intensity,
                    },
                )
            }
        }
    }
}

pub struct IA4<IntensitySource = Luma, AlphaSource = AlphaChannel>(
    PhantomData<(IntensitySource, AlphaSource)>,
);

impl<IntensitySource: ComponentSource, AlphaSource: ComponentSource> Format
    for IA4<IntensitySource, AlphaSource>
{
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let intensity = convert_range::<255, 15>(IntensitySource::get(pixel));
                let alpha = convert_range::<255, 15>(AlphaSource::get(pixel));

                let index = y * Self::TILE_WIDTH + x;
                data[index] = 0.with_bits(0, 4, intensity).with_bits(4, 8, alpha);
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let value = data[index];
                let intensity = convert_range::<15, 255>(value.bits(0, 4));
                let alpha = convert_range::<15, 255>(value.bits(4, 8));

                set(
                    x,
                    y,
                    Pixel {
                        r: intensity,
                        g: intensity,
                        b: intensity,
                        a: alpha,
                    },
                )
            }
        }
    }
}

pub struct I8<Source = Luma>(PhantomData<Source>);

impl<Source: ComponentSource> Format for I8<Source> {
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let intensity = Source::get(pixel);

                let index = y * Self::TILE_WIDTH + x;
                data[index] = intensity;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let intensity = data[index];

                set(
                    x,
                    y,
                    Pixel {
                        r: intensity,
                        g: intensity,
                        b: intensity,
                        a: intensity,
                    },
                )
            }
        }
    }
}

pub struct IA8<IntensitySource = Luma, AlphaSource = AlphaChannel>(
    PhantomData<(IntensitySource, AlphaSource)>,
);

impl<IntensitySource: ComponentSource, AlphaSource: ComponentSource> Format
    for IA8<IntensitySource, AlphaSource>
{
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    #[inline(always)]
    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let intensity = IntensitySource::get(pixel);
                let alpha = AlphaSource::get(pixel);

                let index = y * Self::TILE_WIDTH + x;
                data[2 * index] = alpha;
                data[2 * index + 1] = intensity;
            }
        }
    }

    #[inline(always)]
    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let alpha = data[2 * index];
                let intensity = data[2 * index + 1];

                set(
                    x,
                    y,
                    Pixel {
                        r: intensity,
                        g: intensity,
                        b: intensity,
                        a: alpha,
                    },
                )
            }
        }
    }
}

pub struct Rgb565;

impl Format for Rgb565 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    #[inline(always)]
    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        let pixels: [Pixel; 16] = std::array::from_fn(|i| get(i % 4, i / 4));
        let conv = pixels.map(|p| p.to_rgb565());
        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        let index = X + 4 * Y;
                        let value = conv[index].to_be_bytes();
                        data[2 * index] = value[0];
                        data[2 * index + 1] = value[1];
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        let pixels: [u16; 16] =
            std::array::from_fn(|i| u16::from_be_bytes([data[2 * i], data[2 * i + 1]]));
        let conv = pixels.map(Pixel::from_rgb565);
        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        set(X, Y, conv[X + 4 * Y]);
                    }
                }
            }
        }
    }
}

pub struct FastRgb565;

impl Format for FastRgb565 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    #[inline(always)]
    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        let pixels: [Pixel; 16] = std::array::from_fn(|i| get(i % 4, i / 4));
        let conv = pixels.map(|p| p.to_rgb565_fast());
        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        let index = X + 4 * Y;
                        let value = conv[index].to_be_bytes();
                        data[2 * index] = value[0];
                        data[2 * index + 1] = value[1];
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        let pixels: [u16; 16] =
            std::array::from_fn(|i| u16::from_be_bytes([data[2 * i], data[2 * i + 1]]));
        let conv = pixels.map(Pixel::from_rgb565_fast);
        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        set(X, Y, conv[X + 4 * Y]);
                    }
                }
            }
        }
    }
}

pub struct SimdRgb565;

impl Format for SimdRgb565 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    #[inline(always)]
    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        let pixels: [Pixel; 16] = std::array::from_fn(|i| get(i % 4, i / 4));
        let conv = pixels.map(|p| p.to_rgb565_fast());
        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        let index = X + 4 * Y;
                        let value = conv[index].to_be_bytes();
                        data[2 * index] = value[0];
                        data[2 * index + 1] = value[1];
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        // 01. convert endianness
        let bytes = u8x32::from_slice(data);
        let values = u16x16::from_be_bytes(bytes);

        // 02. extract each channel
        let mask_5 = u16x16::splat(0x1F);
        let mask_6 = u16x16::splat(0x3F);

        let blue = values & mask_5;
        let green = (values >> 5) & mask_6;
        let red = values >> 11;

        // 03. convert each channel to the 0..256 range
        let blue = (blue << 3) | (blue >> 2);
        let green = (green << 2) | (green >> 4);
        let red = (red << 3) | (red >> 2);

        // 04. channels as bytes
        let blue: u8x32 = blue.to_le_bytes();
        let green: u8x32 = green.to_le_bytes();
        let red: u8x32 = red.to_le_bytes();

        // 05. swizzle channels into pairs
        const SWIZZLE_CHANNELS: [usize; 32] = [
            0, 32, 2, 34, 4, 36, 6, 38, 8, 40, 10, 42, 12, 44, 14, 46, //
            16, 48, 18, 50, 20, 52, 22, 54, 24, 56, 26, 58, 28, 60, 30, 62,
        ];

        let alpha = u8x32::splat(255);
        let red_green = simd_swizzle!(red, green, SWIZZLE_CHANNELS);
        let blue_alpha = simd_swizzle!(blue, alpha, SWIZZLE_CHANNELS);
        let red_green = u16x16::from_le_bytes(red_green);
        let blue_alpha = u16x16::from_le_bytes(blue_alpha);

        // 06. swizzle pairs into texels
        const SWIZZLE_PAIRS: [usize; 32] = [
            0, 16, 1, 17, 2, 18, 3, 19, 4, 20, 5, 21, 6, 22, 7, 23, //
            8, 24, 9, 25, 10, 26, 11, 27, 12, 28, 13, 29, 14, 30, 15, 31,
        ];

        let rgba: u16x32 = simd_swizzle!(red_green, blue_alpha, SWIZZLE_PAIRS);

        // 07. store
        let rgba = rgba.to_le_bytes().to_array();
        let rgba: &[Pixel; 16] = zerocopy::transmute_ref!(&rgba);

        seq! {
            Y in 0..4 {
                seq! {
                    X in 0..4 {
                        set(X, Y, rgba[X + 4 * Y]);
                    }
                }
            }
        }
    }
}

pub struct Rgb5A3;

impl Format for Rgb5A3 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = Pixel;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let [high, low] = pixel.to_rgb5a3().to_be_bytes();

                let index = y * Self::TILE_WIDTH + x;
                data[2 * index] = high;
                data[2 * index + 1] = low;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let value = u16::from_be_bytes([data[2 * index], data[2 * index + 1]]);
                set(x, y, Pixel::from_rgb5a3(value))
            }
        }
    }
}

pub struct Rgba8;

impl Format for Rgba8 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;
    const BYTES_PER_TILE: usize = 64;

    type Texel = Pixel;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Pixel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let index = y * Self::TILE_WIDTH + x;
                let offset = 2 * index;

                let ar_offset = offset;
                let gb_offset = 32 + offset;

                data[ar_offset] = pixel.a;
                data[ar_offset + 1] = pixel.r;
                data[gb_offset] = pixel.g;
                data[gb_offset + 1] = pixel.b;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let offset = 2 * index;

                let ar_offset = offset;
                let gb_offset = 32 + offset;

                let (a, r) = (data[ar_offset], data[ar_offset + 1]);
                let (g, b) = (data[gb_offset], data[gb_offset + 1]);

                set(x, y, Pixel { r, g, b, a })
            }
        }
    }
}

pub struct Cmpr;

impl Format for Cmpr {
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 8;

    type Texel = Pixel;

    fn encode_tile(_: &mut [u8], _: impl Fn(usize, usize) -> Pixel) {
        unimplemented!("cmpr encoding not implemented")
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Pixel)) {
        for sub_y in 0..2 {
            for sub_x in 0..2 {
                let sub_base_x = sub_x * 4;
                let sub_base_y = sub_y * 4;
                let sub_base_index = sub_y * 2 + sub_x;
                let sub_offset = 8 * sub_base_index;

                // read palette (first 4 bytes)
                let a = u16::from_be_bytes([data[sub_offset], data[sub_offset + 1]]);
                let b = u16::from_be_bytes([data[sub_offset + 2], data[sub_offset + 3]]);

                let mut palette = [Pixel::default(); 4];
                palette[0] = Pixel::from_rgb565(a);
                palette[1] = Pixel::from_rgb565(b);

                if a > b {
                    palette[2] = palette[0].lerp(palette[1], 1.0 / 3.0);
                    palette[3] = palette[0].lerp(palette[1], 2.0 / 3.0);
                } else {
                    palette[2] = palette[0].lerp(palette[1], 0.5);
                }

                // read pixels (last 4 bytes)
                let mut indices = data[sub_offset + 4..][..4]
                    .iter()
                    .copied()
                    .flat_map(|b| [b.bits(6, 8), b.bits(4, 6), b.bits(2, 4), b.bits(0, 2)]);

                for inner_y in 0..4 {
                    for inner_x in 0..4 {
                        let index = indices.next().unwrap();
                        let pixel = palette[index as usize];

                        let x = sub_base_x + inner_x;
                        let y = sub_base_y + inner_y;
                        set(x, y, pixel);
                    }
                }
            }
        }
    }
}

pub struct CI4;

impl Format for CI4 {
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 8;

    type Texel = PaletteIndex;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Self::Texel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let palette_index = get(x, y);
                let index = y * Self::TILE_WIDTH + x;
                let current = data[index / 2];

                let new = if index % 2 == 0 {
                    current.with_bits(4, 8, palette_index as u8)
                } else {
                    current.with_bits(0, 4, palette_index as u8)
                };

                data[index / 2] = new;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Self::Texel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let value = data[index / 2];
                let palette_index = if index % 2 == 0 {
                    value.bits(4, 8)
                } else {
                    value.bits(0, 4)
                } as u16;

                set(x, y, palette_index)
            }
        }
    }
}

pub struct CI8;

impl Format for CI8 {
    const TILE_WIDTH: usize = 8;
    const TILE_HEIGHT: usize = 4;

    type Texel = PaletteIndex;

    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Self::Texel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let palette_index = get(x, y);
                let index = y * Self::TILE_WIDTH + x;
                data[index] = palette_index as u8;
            }
        }
    }

    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Self::Texel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let palette_index = data[index];

                set(x, y, palette_index as PaletteIndex)
            }
        }
    }
}

pub struct CI14X2;

impl Format for CI14X2 {
    const TILE_WIDTH: usize = 4;
    const TILE_HEIGHT: usize = 4;

    type Texel = PaletteIndex;

    #[inline(always)]
    fn encode_tile(data: &mut [u8], get: impl Fn(usize, usize) -> Self::Texel) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let pixel = get(x, y);
                let low = pixel.bits(0, 8) as u8;
                let high = pixel.bits(8, 14) as u8;

                let index = y * Self::TILE_WIDTH + x;
                data[2 * index] = high;
                data[2 * index + 1] = low;
            }
        }
    }

    #[inline(always)]
    fn decode_tile(data: &[u8], mut set: impl FnMut(usize, usize, Self::Texel)) {
        for y in 0..Self::TILE_HEIGHT {
            for x in 0..Self::TILE_WIDTH {
                let index = y * Self::TILE_WIDTH + x;
                let high = data[2 * index];
                let low = data[2 * index + 1];

                let palette_index = ((high as PaletteIndex) << 8) | (low as PaletteIndex);
                set(x, y, palette_index)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn test_format<F: Format<Texel = Pixel>>(input: &str, name: &str) {
        let img = image::open(input).unwrap();
        let texels = img
            .to_rgba8()
            .pixels()
            .map(|p| Pixel {
                r: p.0[0],
                g: p.0[1],
                b: p.0[2],
                a: p.0[3],
            })
            .collect::<Vec<_>>();

        let required_width = (img.width() as usize).next_multiple_of(F::TILE_WIDTH);
        let required_height = (img.height() as usize).next_multiple_of(F::TILE_HEIGHT);
        let mut encoded = vec![0; compute_size::<F>(required_width, required_height)];

        let stride = F::BYTES_PER_TILE / 32 * required_width / F::TILE_WIDTH;
        encode::<F>(
            stride,
            img.width() as usize,
            img.height() as usize,
            &texels,
            &mut encoded,
        );

        let decoded = decode::<F>(img.width() as usize, img.height() as usize, &encoded);
        let img = image::RgbaImage::from_vec(
            img.width(),
            img.height(),
            decoded
                .into_iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect(),
        )
        .unwrap();

        _ = std::fs::create_dir("local");
        img.save(format!("local/test_out_{name}.png")).unwrap();
    }

    #[test]
    fn test_basic() {
        test_format::<I4<Luma>>("resources/waterfall.webp", "I4");
        test_format::<IA4<Luma, AlphaChannel>>("resources/waterfall.webp", "IA4");
        test_format::<I8<Luma>>("resources/waterfall.webp", "I8");
        test_format::<IA8<Luma, AlphaChannel>>("resources/waterfall.webp", "IA8");
        test_format::<Rgb565>("resources/waterfall.webp", "RGB565");
        test_format::<Rgb5A3>("resources/waterfall.webp", "RGB5A3");
        test_format::<Rgba8>("resources/waterfall.webp", "RGBA8");
    }

    #[test]
    fn test_fast() {
        test_format::<FastRgb565>("resources/waterfall.webp", "FAST_RGB565");
        test_format::<IA8<FastLuma, AlphaChannel>>("resources/waterfall.webp", "FAST_IA8");
    }

    #[test]
    fn test_simd() {
        test_format::<SimdRgb565>("resources/waterfall.webp", "SIMD_RGB565");
    }

    #[test]
    fn test_bad() {
        test_format::<Rgba8>("resources/bad.png", "bad");
        test_format::<Rgba8>("resources/badbig.png", "bigbad");
    }

    #[test]
    fn test_collage() {
        let img = image::open("resources/waterfall.webp").unwrap();
        let pixels = img
            .to_rgba8()
            .pixels()
            .map(|p| Pixel {
                r: p.0[0],
                g: p.0[1],
                b: p.0[2],
                a: p.0[3],
            })
            .collect::<Vec<_>>();

        let width = 2 * img.width() as usize;
        let height = 2 * img.height() as usize;
        let stride_cache = width / Rgba8::TILE_WIDTH * 2;
        let stride_bytes = stride_cache / 2 * Rgba8::BYTES_PER_TILE;
        let mut encoded = vec![0; compute_size::<Rgba8>(width, height)];

        encode::<Rgba8>(
            stride_cache,
            width / 2,
            height / 2,
            &pixels,
            &mut encoded[0..],
        );

        encode::<Rgba8>(
            stride_cache,
            width / 2,
            height / 2,
            &pixels,
            &mut encoded[stride_bytes / 2..],
        );

        encode::<Rgba8>(
            stride_cache,
            width / 2,
            height / 2,
            &pixels,
            &mut encoded[(height / Rgba8::TILE_HEIGHT / 2) * stride_bytes..],
        );

        encode::<Rgba8>(
            stride_cache,
            width / 2,
            height / 2,
            &pixels,
            &mut encoded[(height / Rgba8::TILE_HEIGHT / 2) * stride_bytes + stride_bytes / 2..],
        );

        let decoded = decode::<Rgba8>(width, height, &encoded);
        let img = image::RgbaImage::from_vec(
            width as u32,
            height as u32,
            decoded
                .into_iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect(),
        )
        .unwrap();

        _ = std::fs::create_dir("local");
        img.save(format!("local/collage.png")).unwrap();
    }
}
