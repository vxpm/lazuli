use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use gxtex::{FastLuma, FastRgb565, Format, IA8, Luma, Pixel, Rgb565, compute_size};

fn bench<F: Format<Texel = Pixel>>(c: &mut Criterion, name: &str) {
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

    let required_width = (img.width() as usize).next_multiple_of(F::TILE_WIDTH);
    let required_height = (img.height() as usize).next_multiple_of(F::TILE_HEIGHT);
    let mut encoded = vec![0; compute_size::<F>(required_width, required_height)];
    gxtex::encode::<F>(
        required_width / F::TILE_WIDTH,
        img.width() as usize,
        img.height() as usize,
        black_box(&pixels),
        &mut encoded,
    );

    let mut group = c.benchmark_group(format!("{name} Decoding"));
    group.throughput(criterion::Throughput::Bytes(encoded.len() as u64));

    group.bench_function("Accurate", |b| {
        b.iter_with_large_drop(|| {
            gxtex::decode::<F>(
                img.width() as usize,
                img.height() as usize,
                black_box(&encoded),
            )
        })
    });
    group.finish();

    let mut group = c.benchmark_group(format!("{name} Encoding"));
    group.throughput(criterion::Throughput::Bytes(encoded.len() as u64));

    group.bench_function("Accurate", |b| {
        b.iter_with_large_drop(|| {
            gxtex::encode::<F>(
                required_width / F::TILE_WIDTH,
                img.width() as usize,
                img.height() as usize,
                black_box(&pixels),
                &mut encoded,
            );
        })
    });
    group.finish();
}

fn bench_with_fast<Accurate: Format<Texel = Pixel>, Fast: Format<Texel = Pixel>>(
    c: &mut Criterion,
    name: &str,
) {
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

    let required_width = (img.width() as usize).next_multiple_of(Accurate::TILE_WIDTH);
    let required_height = (img.height() as usize).next_multiple_of(Accurate::TILE_HEIGHT);
    let mut encoded = vec![0; compute_size::<Rgb565>(required_width, required_height)];
    gxtex::encode::<Accurate>(
        required_width / Rgb565::TILE_WIDTH,
        img.width() as usize,
        img.height() as usize,
        black_box(&pixels),
        &mut encoded,
    );

    let mut group = c.benchmark_group(format!("{name} Decoding"));
    group.throughput(criterion::Throughput::Bytes(encoded.len() as u64));

    group.bench_function("Accurate", |b| {
        b.iter_with_large_drop(|| {
            gxtex::decode::<Accurate>(
                img.width() as usize,
                img.height() as usize,
                black_box(&encoded),
            )
        })
    });

    group.bench_function("Fast", |b| {
        b.iter_with_large_drop(|| {
            gxtex::decode::<Fast>(
                img.width() as usize,
                img.height() as usize,
                black_box(&encoded),
            )
        })
    });
    group.finish();

    let mut group = c.benchmark_group(format!("{name} Encoding"));
    group.throughput(criterion::Throughput::Bytes(encoded.len() as u64));

    group.bench_function("Accurate", |b| {
        b.iter_with_large_drop(|| {
            gxtex::encode::<Accurate>(
                required_width / Rgb565::TILE_WIDTH,
                img.width() as usize,
                img.height() as usize,
                black_box(&pixels),
                &mut encoded,
            );
        })
    });

    group.bench_function("Fast", |b| {
        b.iter_with_large_drop(|| {
            gxtex::encode::<Fast>(
                required_width / FastRgb565::TILE_WIDTH,
                img.width() as usize,
                img.height() as usize,
                black_box(&pixels),
                &mut encoded,
            );
        })
    });

    group.finish();
}

fn formats(c: &mut Criterion) {
    bench_with_fast::<Rgb565, FastRgb565>(c, "RGB565");
    bench_with_fast::<IA8<Luma, Luma>, IA8<FastLuma, FastLuma>>(c, "IA8");
    bench_with_fast::<Rgb565, FastRgb565>(c, "SIMD");
}

criterion_group!(benches, formats);
criterion_main!(benches);
