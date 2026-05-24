//! End-to-end behavioral tests for the animation engine pipeline.
//!
//! These exercise the system the way the plugin actually drives it:
//! register an engine from a real MIME bundle, tick it on a clock,
//! pause/resume, drop, observe state transitions across the FFI boundary
//! and through the registry. They run without an editor — every public
//! contract that does not require a live render surface is here.

use std::ffi::CString;
use std::time::{Duration, Instant};

use nothelix::animation::*;

mod fixtures {
    use std::time::Duration;

    pub fn tiny_gif() -> Vec<u8> {
        use ::image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};
        let mut buf = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut buf);
            enc.set_repeat(::image::codecs::gif::Repeat::Infinite).unwrap();
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

    pub fn static_webp() -> Vec<u8> {
        use ::image::codecs::webp::WebPEncoder;
        use ::image::{ColorType, ImageEncoder};
        let rgba = [0u8, 200, 50, 255].repeat(64);
        let mut buf = Vec::new();
        WebPEncoder::new_lossless(&mut buf)
            .write_image(&rgba, 8, 8, ColorType::Rgba8.into())
            .unwrap();
        buf
    }

    pub fn animated_apng() -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let palette = [
            [255u8, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
        ];
        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(&mut buf, 4, 4);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.set_animated(palette.len() as u32, 0).unwrap();
            enc.set_frame_delay(100, 1000).unwrap();
            let mut writer = enc.write_header().unwrap();
            for color in &palette {
                let frame: Vec<u8> = color.iter().copied().cycle().take(4 * 4 * 4).collect();
                writer.write_image_data(&frame).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }
}

/// Helper: register through the C-ABI and return the engine_id.
fn register(mime: &str, bytes: &[u8]) -> Result<u64, i32> {
    let mime_c = CString::new(mime).unwrap();
    let mut id: u64 = 0;
    let rc = unsafe {
        nothelix_animation_register(mime_c.as_ptr(), bytes.as_ptr(), bytes.len(), &mut id)
    };
    if rc == 0 {
        Ok(id)
    } else {
        Err(rc)
    }
}

/// Helper: tick once, returning (status, bytes_or_empty).
fn tick(id: u64) -> (i32, Vec<u8>) {
    let mut p: *mut u8 = std::ptr::null_mut();
    let mut l: usize = 0;
    let mut h: u16 = 0;
    let mut d: u32 = 0;
    let rc = unsafe { nothelix_animation_tick(id, &mut p, &mut l, &mut h, &mut d) };
    let bytes = if rc == 0 && l > 0 {
        let v = unsafe { std::slice::from_raw_parts(p, l).to_vec() };
        unsafe { nothelix_animation_free_buffer(p, l) };
        v
    } else {
        Vec::new()
    };
    (rc, bytes)
}

fn drop_engine(id: u64) {
    unsafe { nothelix_animation_drop(id) };
}

// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_register_tick_pause_resume_drop() {
    let id = register("image/gif", &fixtures::tiny_gif()).expect("register tiny gif");

    // First tick produces a frame.
    let (rc, bytes) = tick(id);
    assert_eq!(rc, 0, "first tick should produce a frame");
    assert!(!bytes.is_empty());

    // Pause — subsequent tick returns 2 (paused/finished sentinel) with no bytes.
    let pause_rc = unsafe { nothelix_animation_set_pause(id, true) };
    assert_eq!(pause_rc, 0);
    let (rc_paused, bytes_paused) = tick(id);
    assert_eq!(rc_paused, 2, "paused tick should return status 2");
    assert!(bytes_paused.is_empty());

    // Resume. The next tick (a moment later in the GIF's looping schedule)
    // produces output again.
    let resume_rc = unsafe { nothelix_animation_set_pause(id, false) };
    assert_eq!(resume_rc, 0);
    std::thread::sleep(Duration::from_millis(120));
    let (rc_resumed, _bytes_resumed) = tick(id);
    assert!(
        rc_resumed == 0 || rc_resumed == 1,
        "resumed tick should be frame (0) or no-change (1), got {rc_resumed}"
    );

    drop_engine(id);

    // After drop, ticks against the gone engine_id return -1.
    let (rc_gone, _) = tick(id);
    assert_eq!(rc_gone, -1, "tick on dropped engine should return -1");
}

#[test]
fn unknown_mime_returns_negative_error_code() {
    let err = register("image/whatever", b"raw bytes").expect_err("should fail");
    assert_eq!(err, -2, "unknown MIME should return -2");
}

#[test]
fn malformed_bytes_for_known_mime_returns_decode_error() {
    let err = register("image/gif", b"NOT A GIF AT ALL").expect_err("should fail");
    assert_eq!(err, -3, "malformed bytes should return -3");
}

#[test]
fn null_pointers_return_minus_ten() {
    let mut id: u64 = 0;
    let rc = unsafe {
        nothelix_animation_register(std::ptr::null(), std::ptr::null(), 0, &mut id)
    };
    assert_eq!(rc, -10);
}

#[test]
fn dedup_skips_repeat_transmissions_at_same_clock() {
    // Tick twice in immediate succession — the second tick lands on the
    // same GIF frame as the first, so dedup must skip transmission.
    let id = register("image/gif", &fixtures::tiny_gif()).expect("register");
    let (rc1, bytes1) = tick(id);
    assert_eq!(rc1, 0);
    assert!(!bytes1.is_empty());

    // Without sleeping the engine clock advances by ~µs — same GIF frame.
    let (rc2, bytes2) = tick(id);
    assert_eq!(rc2, 1, "second tick at near-zero elapsed should dedup");
    assert!(bytes2.is_empty());

    drop_engine(id);
}

#[test]
fn distinct_engines_are_independent() {
    let a = register("image/gif", &fixtures::tiny_gif()).expect("a");
    let b = register("image/gif", &fixtures::tiny_gif()).expect("b");
    assert_ne!(a, b, "registry must allocate distinct ids");

    // Pause a; b still ticks.
    let _ = unsafe { nothelix_animation_set_pause(a, true) };
    let (a_rc, _) = tick(a);
    assert_eq!(a_rc, 2);
    let (b_rc, b_bytes) = tick(b);
    assert_eq!(b_rc, 0, "engine b should still tick while a is paused");
    assert!(!b_bytes.is_empty());

    drop_engine(a);
    drop_engine(b);
}

#[test]
fn animated_apng_yields_more_than_one_distinct_frame_over_time() {
    let id = register("image/apng", &fixtures::animated_apng()).expect("register apng");
    let mut distinct: std::collections::HashSet<u64> = Default::default();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(800) && distinct.len() < 3 {
        let (rc, bytes) = tick(id);
        if rc == 0 && !bytes.is_empty() {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&bytes, &mut h);
            distinct.insert(std::hash::Hasher::finish(&h));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop_engine(id);
    assert!(
        distinct.len() >= 2,
        "expected ≥2 distinct frames over 800ms of a 3-frame 100ms-each APNG, got {}",
        distinct.len()
    );
}

#[test]
fn static_webp_registers_and_yields_one_frame() {
    let id = register("image/webp", &fixtures::static_webp()).expect("static webp");
    let (rc, bytes) = tick(id);
    assert_eq!(rc, 0, "first tick on static webp should produce a frame");
    assert!(!bytes.is_empty());
    // Subsequent ticks must dedup (static = same content_id forever).
    let (rc2, bytes2) = tick(id);
    assert_eq!(rc2, 1);
    assert!(bytes2.is_empty());
    drop_engine(id);
}

#[test]
fn pause_freezes_elapsed_so_resume_does_not_skip_frames() {
    // Pause a fresh engine immediately after one tick. Sleep past the
    // GIF's first-frame boundary. Resume. The next tick must still see
    // the engine's elapsed time as the offset captured at pause —
    // i.e. NOT have advanced through the sleep — so we should still
    // be on (or near) frame 0, not frame 4.
    let id = register("image/gif", &fixtures::tiny_gif()).expect("register");
    let _ = tick(id); // seed last_content_id at frame 0
    let _ = unsafe { nothelix_animation_set_pause(id, true) };

    std::thread::sleep(Duration::from_millis(500));

    let _ = unsafe { nothelix_animation_set_pause(id, false) };
    // Probe the engine's reported frame index via the steel_api meta accessor.
    // animation_tick_bytes drives the tick and populates last_tick_*.
    let _ = steel_api::animation_tick_bytes(id as isize);
    let frame_idx = steel_api::animation_tick_frame_index(id as isize);
    drop_engine(id);

    // 500ms of real time elapsed but the engine was paused — at most
    // we've advanced one frame's worth (~100ms) since resume.
    assert!(
        frame_idx <= 1,
        "pause must freeze elapsed; expected frame_idx ≤1 after pause+sleep+resume, got {frame_idx}"
    );
}

#[test]
fn steel_api_register_drop_round_trip() {
    let id = steel_api::animation_register("image/gif".into(), fixtures::tiny_gif());
    assert!(id > 0);
    let teardown = steel_api::animation_drop(id);
    // Teardown bytes are renderer-dependent; for static-fallback it's empty.
    // For kitty native it would be the delete escape. We only require: no panic.
    let _ = teardown;
}

#[test]
fn dropped_engine_id_yields_negative_status() {
    let id = steel_api::animation_register("image/gif".into(), fixtures::tiny_gif());
    assert!(id > 0);
    let _ = steel_api::animation_drop(id);
    let status = steel_api::animation_tick_status(id);
    assert_eq!(
        status, -2isize,
        "tick_status on dropped engine should return -2"
    );
}

#[test]
fn registering_animated_mime_through_steel_api_uses_animated_decoder_path() {
    // The Steel-side caller wouldn't know whether libnothelix has a real
    // decoder or just the registry stub for that MIME. Exercise the
    // happy paths and a stub path:
    let gif_id = steel_api::animation_register("image/gif".into(), fixtures::tiny_gif());
    assert!(gif_id > 0);

    // video/mp4 has a decoder in the registry but the open() returns
    // UnsupportedCodec, which surfaces as -3 from the FFI.
    #[cfg(feature = "video")]
    {
        // Synthesize a few bytes that can't actually parse as MP4 — even
        // if openh264 is wired, the bytes are wrong, so we must surface
        // a negative code, not register an empty engine.
        let mp4_id = steel_api::animation_register("video/mp4".into(), vec![0u8; 32]);
        assert!(mp4_id < 0, "stub or real decoder must reject garbage bytes");
    }

    let _ = steel_api::animation_drop(gif_id);
}
