//! Animation smoke test for `nothelix doctor --smoke --animation`.
//!
//! Builds a deterministic 4-frame GIF, registers it through the libnothelix
//! animation FFI, ticks ≥ 12 times, asserts ≥ 4 distinct frames were emitted
//! to the wire. Exits 0 on success, non-zero with a single-line failure
//! message on stderr.

use std::ffi::CString;
use std::time::{Duration, Instant};

fn main() {
    match smoke() {
        Ok(distinct) => {
            println!("[ok] animation smoke: {distinct} distinct frames emitted");
            std::process::exit(0);
        }
        Err(msg) => {
            eprintln!("[fail] animation smoke: {msg}");
            std::process::exit(1);
        }
    }
}

fn smoke() -> Result<usize, String> {
    let bytes = build_tiny_gif();
    let mime = CString::new("image/gif").map_err(|e| e.to_string())?;
    let mut id: u64 = 0;

    let rc = unsafe {
        nothelix::animation::nothelix_animation_register(
            mime.as_ptr(),
            bytes.as_ptr(),
            bytes.len(),
            &mut id,
        )
    };
    if rc != 0 {
        return Err(format!("register returned {rc}"));
    }
    if id == 0 {
        return Err("register produced engine_id 0".into());
    }

    let mut distinct: std::collections::HashSet<u64> = Default::default();
    let mut frames_seen = 0usize;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) && distinct.len() < 4 {
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut l: usize = 0;
        let mut h: u16 = 0;
        let mut d: u32 = 0;
        let rc = unsafe {
            nothelix::animation::nothelix_animation_tick(id, &mut p, &mut l, &mut h, &mut d)
        };
        if rc < 0 {
            return Err(format!("tick returned {rc}"));
        }
        if rc == 0 && l > 0 {
            let slice = unsafe { std::slice::from_raw_parts(p, l) };
            let mut hh = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(slice, &mut hh);
            distinct.insert(std::hash::Hasher::finish(&hh));
            frames_seen += 1;
            unsafe { nothelix::animation::nothelix_animation_free_buffer(p, l) };
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    unsafe { nothelix::animation::nothelix_animation_drop(id) };

    if distinct.len() < 4 {
        return Err(format!(
            "only {} distinct frames after {} ticks",
            distinct.len(),
            frames_seen
        ));
    }
    Ok(distinct.len())
}

fn build_tiny_gif() -> Vec<u8> {
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
