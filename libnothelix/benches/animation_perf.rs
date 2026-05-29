//! Animation perf characterisation. Runs the tick loop and records
//! per-frame work, encode throughput, and idle cost so we can argue
//! about battery and CPU budget with concrete numbers, not adjectives.
//!
//! Run: cargo run --release --features gif --bench animation_perf

use std::time::{Duration, Instant};

use nothelix::animation::decoders::gif::GifSource;
use nothelix::animation::engine::AnimationEngine;
use nothelix::animation::renderer::{select_renderer, TerminalCaps};
use nothelix::animation::renderers::kitty_replay::KittyReplayRenderer;
use nothelix::animation::renderers::static_fallback::StaticFallbackRenderer;

/// Inline fixture builder (the test fixture lives behind #[cfg(test)] so the
/// bench rebuilds the same 4-frame 32x32 GIF here).
fn tiny_gif_bytes() -> Vec<u8> {
    use ::image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};
    let mut buf = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut buf);
        enc.set_repeat(::image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for k in 0..4u8 {
            let mut img = RgbaImage::new(32, 32);
            for p in img.pixels_mut() {
                *p = ::image::Rgba([k * 60, 0, 255 - k * 60, 255]);
            }
            let frame = Frame::from_parts(
                img,
                0,
                0,
                Delay::from_saturating_duration(Duration::from_millis(100)),
            );
            enc.encode_frame(frame).unwrap();
        }
    }
    buf
}

fn main() {
    println!("animation perf — release build");
    bench_decode();
    bench_tick_static_fallback();
    bench_tick_kitty_replay();
    bench_idle_no_animations();
    bench_dedup_skip();
    bench_steady_state_fps();
    bench_large_frame();
}

/// Build a 4-frame animated GIF at the given dimension.
fn animated_gif(side: u32, frames: u32) -> Vec<u8> {
    use ::image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};
    let mut buf = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut buf);
        enc.set_repeat(::image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for k in 0..frames as u8 {
            let mut img = RgbaImage::new(side, side);
            for p in img.pixels_mut() {
                *p = ::image::Rgba([
                    k.wrapping_mul(60),
                    0,
                    255u8.wrapping_sub(k.wrapping_mul(60)),
                    255,
                ]);
            }
            let frame = Frame::from_parts(
                img,
                0,
                0,
                Delay::from_saturating_duration(Duration::from_millis(100)),
            );
            enc.encode_frame(frame).unwrap();
        }
    }
    buf
}

fn bench_large_frame() {
    println!("\n[large frame] 480x480 4-frame GIF (≈ Plots.jl default size)");
    let bytes = animated_gif(480, 4);
    println!("  source size: {} bytes", bytes.len());
    let dec = GifSource::open(&bytes).unwrap();
    let caps = TerminalCaps {
        kitty_graphics: true,
        kitty_animation_protocol: false,
        max_fps: 60,
    };
    let r = KittyReplayRenderer::try_new(&caps).unwrap();
    let mut eng = AnimationEngine::new(99, dec, r, 64 * 1024 * 1024);

    let start = Instant::now();
    let mut frame_bytes = 0usize;
    let mut frames = 0u32;
    let mut tick_total = Duration::ZERO;
    for i in 0..40u32 {
        let now = start + Duration::from_millis(i as u64 * 100);
        let t0 = Instant::now();
        if let Some(out) = eng.tick(now) {
            if !out.bytes.is_empty() {
                frame_bytes += out.bytes.len();
                frames += 1;
            }
        }
        tick_total += t0.elapsed();
    }
    println!(
        "  40 ticks (4 unique frames × 10 cycles): {} unique-frame emissions, mean per-emitted-frame {:?}, total wire bytes {} ({} B/frame)",
        frames,
        if frames > 0 { tick_total / frames } else { Duration::ZERO },
        frame_bytes,
        if frames > 0 { frame_bytes / frames as usize } else { 0 },
    );
    let cpu_pct = (tick_total.as_secs_f64() / 4.0) * 100.0; // 4 sec wall-clock for 40 ticks @ 100ms
    println!(
        "  → at 10 fps native rate, CPU budget: {:.4}% of one core",
        cpu_pct
    );
}

fn time<F: FnMut()>(label: &str, iters: u32, mut f: F) -> Duration {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let per = elapsed / iters;
    println!(
        "  {:32}  {} iters,  total {:>8.3?},  per-iter {:>8.3?}",
        label, iters, elapsed, per
    );
    per
}

fn bench_decode() {
    println!("\n[decode] open + frame_at on tiny 4-frame 32x32 GIF");
    let bytes = tiny_gif_bytes();
    println!("  fixture size: {} bytes", bytes.len());

    let _ = time("open()", 1000, || {
        let _ = GifSource::open(&bytes).unwrap();
    });

    let mut dec = GifSource::open(&bytes).unwrap();
    let _ = time("frame_at(150ms)", 100_000, || {
        let _ = dec.frame_at(Duration::from_millis(150)).unwrap();
    });
}

fn bench_tick_static_fallback() {
    println!("\n[tick — static fallback (PNG re-encode)] tiny GIF, 32x32");
    let bytes = tiny_gif_bytes();
    let dec = GifSource::open(&bytes).unwrap();
    let r = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
    let mut eng = AnimationEngine::new(1, dec, r, 1_000_000);
    let start = Instant::now();
    let mut frame_bytes = 0usize;
    let mut frames = 0u32;
    for i in 0..1000u32 {
        let now = start + Duration::from_millis(i as u64 * 100);
        if let Some(out) = eng.tick(now) {
            if !out.bytes.is_empty() {
                frame_bytes += out.bytes.len();
                frames += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    println!(
        "  1000 ticks, {} frames emitted, total bytes {}, mean per-tick {:.3?}, throughput {:.1} kB/s",
        frames,
        frame_bytes,
        elapsed / 1000,
        (frame_bytes as f64 / 1024.0) / elapsed.as_secs_f64()
    );
}

fn bench_tick_kitty_replay() {
    println!("\n[tick — kitty replay (PNG + Kitty APC)] tiny GIF, 32x32");
    let bytes = tiny_gif_bytes();
    let dec = GifSource::open(&bytes).unwrap();
    let caps = TerminalCaps {
        kitty_graphics: true,
        kitty_animation_protocol: false,
        max_fps: 60,
    };
    let r = KittyReplayRenderer::try_new(&caps).unwrap();
    let mut eng = AnimationEngine::new(2, dec, r, 1_000_000);
    let start = Instant::now();
    let mut frame_bytes = 0usize;
    let mut frames = 0u32;
    for i in 0..1000u32 {
        let now = start + Duration::from_millis(i as u64 * 100);
        if let Some(out) = eng.tick(now) {
            if !out.bytes.is_empty() {
                frame_bytes += out.bytes.len();
                frames += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    println!(
        "  1000 ticks, {} frames emitted, total bytes {}, mean per-tick {:.3?}, throughput {:.1} kB/s",
        frames,
        frame_bytes,
        elapsed / 1000,
        (frame_bytes as f64 / 1024.0) / elapsed.as_secs_f64()
    );
}

fn bench_idle_no_animations() {
    println!("\n[idle] inventory selection cost when no engines registered");
    let caps = TerminalCaps::default();
    let _ = time("select_renderer(no kitty)", 100_000, || {
        let _ = select_renderer(&caps);
    });
    let caps = TerminalCaps {
        kitty_graphics: true,
        kitty_animation_protocol: false,
        max_fps: 60,
    };
    let _ = time("select_renderer(kitty)", 100_000, || {
        let _ = select_renderer(&caps);
    });
}

fn bench_dedup_skip() {
    println!("\n[dedup] skip-on-content-id (renderer should return empty Vec)");
    let bytes = tiny_gif_bytes();
    let dec = GifSource::open(&bytes).unwrap();
    let r = StaticFallbackRenderer::try_new(&TerminalCaps::default()).unwrap();
    let mut eng = AnimationEngine::new(3, dec, r, 1_000_000);
    // First tick to seed last_content_id
    let now = Instant::now();
    let _ = eng.tick(now);
    // Subsequent ticks at same time → same frame → renderer skips
    let elapsed = time("tick same-frame (dedup)", 100_000, || {
        let _ = eng.tick(now);
    });
    println!(
        "  dedup hot-path per-tick: {:.3?}  (this is the cost when an animation is paused on its current frame)",
        elapsed
    );
}

fn bench_steady_state_fps() {
    println!("\n[fps cap] simulate 60 fps drive of one engine for 1 second");
    let bytes = tiny_gif_bytes();
    let dec = GifSource::open(&bytes).unwrap();
    let caps = TerminalCaps {
        kitty_graphics: true,
        kitty_animation_protocol: false,
        max_fps: 60,
    };
    let r = KittyReplayRenderer::try_new(&caps).unwrap();
    let mut eng = AnimationEngine::new(4, dec, r, 1_000_000);
    let start = Instant::now();
    let frames_per_sec = 60u32;
    let frame_period = Duration::from_secs_f64(1.0 / frames_per_sec as f64);
    let mut total_bytes = 0usize;
    let mut frames_emitted = 0u32;
    let mut work_time = Duration::ZERO;
    for i in 0..frames_per_sec {
        let now = start + frame_period * i;
        let t0 = Instant::now();
        if let Some(out) = eng.tick(now) {
            if !out.bytes.is_empty() {
                total_bytes += out.bytes.len();
                frames_emitted += 1;
            }
        }
        work_time += t0.elapsed();
    }
    println!(
        "  60-tick second: {} frames emitted, {} bytes, work-time {:.3?} ({:.4}% CPU at 60fps)",
        frames_emitted,
        total_bytes,
        work_time,
        (work_time.as_secs_f64() / 1.0) * 100.0
    );
    println!(
        "  → mean wire bytes per emitted frame: {} B",
        if frames_emitted > 0 {
            total_bytes / frames_emitted as usize
        } else {
            0
        }
    );
}
