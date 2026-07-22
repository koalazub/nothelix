use nothelix::animation::decoders::gif;
use nothelix::animation::engine::AnimationEngine;
use nothelix::animation::renderer::{AnimationRenderer, TerminalCaps, select_renderer};
use nothelix::animation::renderers::kitty_replay::KittyReplayRenderer;
use nothelix::animation::renderers::static_fallback::StaticFallbackRenderer;
use std::time::{Duration, Instant};

const NATIVE_FRAME_PERIOD: Duration = Duration::from_millis(100);
const TINY_SIDE: u32 = 32;
const PLOTS_SIDE: u32 = 480;
const FIXTURE_FRAMES: u32 = 4;

fn main() {
    println!("animation perf — release build");
    bench_decode();
    bench_tick("static fallback (PNG re-encode)", 1, static_fallback());
    bench_tick("kitty replay (PNG + Kitty APC)", 2, kitty_replay());
    bench_idle_no_animations();
    bench_dedup_skip();
    bench_steady_state_fps();
    bench_large_frame();
}

struct TickTally {
    emitted: u32,
    wire_bytes: usize,
    work: Duration,
}

impl TickTally {
    fn mean_work_per_frame(&self) -> Duration {
        self.work
            .checked_div(self.emitted)
            .unwrap_or(Duration::ZERO)
    }

    fn mean_bytes_per_frame(&self) -> usize {
        self.wire_bytes
            .checked_div(self.emitted as usize)
            .unwrap_or(0)
    }

    fn throughput_kb_per_sec(&self) -> f64 {
        (self.wire_bytes as f64 / 1024.0) / self.work.as_secs_f64()
    }

    fn core_utilisation_pct(&self, wall_clock: Duration) -> f64 {
        (self.work.as_secs_f64() / wall_clock.as_secs_f64()) * 100.0
    }
}

fn drive(engine: &mut AnimationEngine, ticks: u32, period: Duration) -> TickTally {
    let start = Instant::now();
    let mut tally = TickTally {
        emitted: 0,
        wire_bytes: 0,
        work: Duration::ZERO,
    };
    for tick in 0..ticks {
        let began = Instant::now();
        if let Some(output) = engine.tick(start + period * tick)
            && !output.bytes.is_empty()
        {
            tally.wire_bytes += output.bytes.len();
            tally.emitted += 1;
        }
        tally.work += began.elapsed();
    }
    tally
}

fn animated_gif(side: u32, frames: u32) -> Vec<u8> {
    use ::image::{Delay, Frame, RgbaImage, codecs::gif::GifEncoder};
    let mut buf = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut buf);
        encoder
            .set_repeat(::image::codecs::gif::Repeat::Infinite)
            .expect("set gif repeat");
        for step in 0..frames as u8 {
            let mut image = RgbaImage::new(side, side);
            for pixel in image.pixels_mut() {
                *pixel = ::image::Rgba([
                    step.wrapping_mul(60),
                    0,
                    255u8.wrapping_sub(step.wrapping_mul(60)),
                    255,
                ]);
            }
            encoder
                .encode_frame(Frame::from_parts(
                    image,
                    0,
                    0,
                    Delay::from_saturating_duration(NATIVE_FRAME_PERIOD),
                ))
                .expect("encode gif frame");
        }
    }
    buf
}

fn tiny_gif() -> Vec<u8> {
    animated_gif(TINY_SIDE, FIXTURE_FRAMES)
}

fn gif_engine(id: u64, bytes: &[u8], renderer: Box<dyn AnimationRenderer>) -> AnimationEngine {
    let decoder = gif::open(bytes).expect("decode gif fixture");
    AnimationEngine::new(id, decoder, renderer)
}

fn static_fallback() -> Box<dyn AnimationRenderer> {
    StaticFallbackRenderer::try_new(&TerminalCaps::default()).expect("static fallback renderer")
}

fn kitty_replay() -> Box<dyn AnimationRenderer> {
    KittyReplayRenderer::try_new(&TerminalCaps::HOST_DEFAULT).expect("kitty replay renderer")
}

fn time<F: FnMut()>(label: &str, iters: u32, mut body: F) -> Duration {
    let start = Instant::now();
    for _ in 0..iters {
        body();
    }
    let elapsed = start.elapsed();
    let per_iter = elapsed / iters;
    println!("  {label:32}  {iters} iters,  total {elapsed:>8.3?},  per-iter {per_iter:>8.3?}");
    per_iter
}

fn bench_decode() {
    println!(
        "\n[decode] open + frame_at on tiny {FIXTURE_FRAMES}-frame {TINY_SIDE}x{TINY_SIDE} GIF"
    );
    let bytes = tiny_gif();
    println!("  fixture size: {} bytes", bytes.len());

    time("open()", 1_000, || {
        let _ = gif::open(&bytes).expect("decode gif fixture");
    });

    let mut decoder = gif::open(&bytes).expect("decode gif fixture");
    time("frame_at(150ms)", 100_000, || {
        let _ = decoder
            .frame_at(Duration::from_millis(150))
            .expect("frame at 150ms");
    });
}

fn bench_tick(label: &str, id: u64, renderer: Box<dyn AnimationRenderer>) {
    println!("\n[tick — {label}] tiny GIF, {TINY_SIDE}x{TINY_SIDE}");
    let mut engine = gif_engine(id, &tiny_gif(), renderer);
    let tally = drive(&mut engine, 1_000, NATIVE_FRAME_PERIOD);
    println!(
        "  1000 ticks, {} frames emitted, total bytes {}, work-time {:.3?}, throughput {:.1} kB/s",
        tally.emitted,
        tally.wire_bytes,
        tally.work,
        tally.throughput_kb_per_sec()
    );
}

fn bench_idle_no_animations() {
    println!("\n[idle] inventory selection cost when no engines registered");
    time("select_renderer(no kitty)", 100_000, || {
        let _ = select_renderer(&TerminalCaps::default());
    });
    time("select_renderer(kitty)", 100_000, || {
        let _ = select_renderer(&TerminalCaps::HOST_DEFAULT);
    });
}

fn bench_dedup_skip() {
    println!("\n[dedup] skip-on-content-id (renderer returns empty bytes)");
    let mut engine = gif_engine(3, &tiny_gif(), static_fallback());
    let now = Instant::now();
    let _ = engine.tick(now);
    let per_tick = time("tick same-frame (dedup)", 100_000, || {
        let _ = engine.tick(now);
    });
    println!(
        "  dedup hot-path per-tick: {per_tick:.3?}  (the cost while an animation sits paused on its current frame)"
    );
}

fn bench_steady_state_fps() {
    println!("\n[fps cap] simulate 60 fps drive of one engine for 1 second");
    let ticks = 60u32;
    let second = Duration::from_secs(1);
    let mut engine = gif_engine(4, &tiny_gif(), kitty_replay());
    let tally = drive(&mut engine, ticks, second / ticks);
    println!(
        "  {}-tick second: {} frames emitted, {} bytes, work-time {:.3?} ({:.4}% CPU at 60fps)",
        ticks,
        tally.emitted,
        tally.wire_bytes,
        tally.work,
        tally.core_utilisation_pct(second)
    );
    println!(
        "  → mean wire bytes per emitted frame: {} B",
        tally.mean_bytes_per_frame()
    );
}

fn bench_large_frame() {
    println!(
        "\n[large frame] {PLOTS_SIDE}x{PLOTS_SIDE} {FIXTURE_FRAMES}-frame GIF (≈ Plots.jl default size)"
    );
    let bytes = animated_gif(PLOTS_SIDE, FIXTURE_FRAMES);
    println!("  source size: {} bytes", bytes.len());
    let ticks = 40u32;
    let mut engine = gif_engine(99, &bytes, kitty_replay());
    let tally = drive(&mut engine, ticks, NATIVE_FRAME_PERIOD);
    println!(
        "  {} ticks ({} unique frames × 10 cycles): {} unique-frame emissions, mean per-emitted-frame {:?}, total wire bytes {} ({} B/frame)",
        ticks,
        FIXTURE_FRAMES,
        tally.emitted,
        tally.mean_work_per_frame(),
        tally.wire_bytes,
        tally.mean_bytes_per_frame(),
    );
    println!(
        "  → at 10 fps native rate, CPU budget: {:.4}% of one core",
        tally.core_utilisation_pct(NATIVE_FRAME_PERIOD * ticks)
    );
}
