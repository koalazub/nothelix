//! Animated media engine. Library-agnostic — accepts any MIME bundle
//! the decoder table understands.

pub mod cache;
pub mod decoder;
pub mod decoders;
pub mod engine;
pub mod registry;
pub mod renderer;
pub mod renderers;

use std::ffi::{c_char, CStr};
use std::time::Instant;

#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_register(
    mime_ptr: *const c_char,
    bytes_ptr: *const u8,
    bytes_len: usize,
    out_engine_id: *mut u64,
) -> i32 {
    if mime_ptr.is_null() || bytes_ptr.is_null() || out_engine_id.is_null() {
        return -10;
    }
    let mime = match CStr::from_ptr(mime_ptr).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let bytes = std::slice::from_raw_parts(bytes_ptr, bytes_len);
    let factory = match decoder::lookup_decoder(mime) {
        Some(f) => f,
        None => return -2,
    };
    let dec = match factory(bytes) {
        Ok(d) => d,
        Err(_) => return -3,
    };
    let caps = renderer::TerminalCaps {
        kitty_graphics: true, // wired from doctor probe in plugin (Task 22)
        kitty_animation_protocol: false,
        max_fps: 60,
    };
    let r = renderer::select_renderer(&caps);
    let mut reg = registry::registry().lock().unwrap();
    let id = reg.allocate_id();
    let eng = engine::AnimationEngine::new(id, dec, r, 64 * 1024 * 1024);
    reg.insert(id, eng);
    *out_engine_id = id;
    0
}

#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_tick(
    engine_id: u64,
    out_payload_ptr: *mut *mut u8,
    out_payload_len: *mut usize,
    out_height: *mut u16,
    out_next_delay_ms: *mut u32,
) -> i32 {
    if out_payload_ptr.is_null()
        || out_payload_len.is_null()
        || out_height.is_null()
        || out_next_delay_ms.is_null()
    {
        return -10;
    }
    let mut reg = registry::registry().lock().unwrap();
    let eng = match reg.get_mut(engine_id) {
        Some(e) => e,
        None => return -1,
    };
    let out = match eng.tick(Instant::now()) {
        Some(o) => o,
        None => {
            *out_payload_ptr = std::ptr::null_mut();
            *out_payload_len = 0;
            *out_height = 0;
            *out_next_delay_ms = 0;
            return 2; // finished or paused
        }
    };
    *out_height = out.height;
    *out_next_delay_ms = out.next_delay_ms;
    if out.bytes.is_empty() {
        *out_payload_ptr = std::ptr::null_mut();
        *out_payload_len = 0;
        return 1; // no new frame to send
    }
    let mut boxed = out.bytes.into_boxed_slice();
    *out_payload_ptr = boxed.as_mut_ptr();
    *out_payload_len = boxed.len();
    std::mem::forget(boxed);
    0
}

#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_free_buffer(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
    }
}

#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_drop(engine_id: u64) {
    if let Ok(mut reg) = registry::registry().lock() {
        if let Some(mut eng) = reg.drop_engine(engine_id) {
            // Emit teardown bytes to clean up the renderer; we drop them since
            // the caller can call drop without first requesting teardown bytes
            // (the plugin sends teardown via tick output during normal flow).
            let _ = eng.teardown();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_set_pause(engine_id: u64, paused: bool) -> i32 {
    let mut reg = registry::registry().lock().unwrap();
    if let Some(eng) = reg.get_mut(engine_id) {
        let now = Instant::now();
        if paused { eng.pause(now) } else { eng.resume(now) }
        0
    } else {
        -1
    }
}

#[cfg(test)]
mod ffi_tests {
    use super::*;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
    use std::ffi::CString;

    #[test]
    fn register_and_tick_via_ffi() {
        let bytes = tiny_gif_bytes();
        let mime = CString::new("image/gif").unwrap();
        let mut id: u64 = 0;
        let rc = unsafe {
            nothelix_animation_register(mime.as_ptr(), bytes.as_ptr(), bytes.len(), &mut id)
        };
        assert_eq!(rc, 0);
        assert!(id > 0);

        let mut payload_ptr: *mut u8 = std::ptr::null_mut();
        let mut payload_len: usize = 0;
        let mut height: u16 = 0;
        let mut delay: u32 = 0;
        let rc = unsafe {
            nothelix_animation_tick(id, &mut payload_ptr, &mut payload_len, &mut height, &mut delay)
        };
        assert!(rc <= 1);
        if rc == 0 {
            unsafe { nothelix_animation_free_buffer(payload_ptr, payload_len); }
        }
        unsafe { nothelix_animation_drop(id); }
    }

    #[test]
    fn unknown_mime_returns_minus_two() {
        let mime = CString::new("image/nope").unwrap();
        let bytes: Vec<u8> = vec![0; 16];
        let mut id: u64 = 0;
        let rc = unsafe {
            nothelix_animation_register(mime.as_ptr(), bytes.as_ptr(), bytes.len(), &mut id)
        };
        assert_eq!(rc, -2);
    }

    #[test]
    fn null_args_return_minus_ten() {
        let mut id: u64 = 0;
        let rc = unsafe {
            nothelix_animation_register(std::ptr::null(), std::ptr::null(), 0, &mut id)
        };
        assert_eq!(rc, -10);
    }
}
