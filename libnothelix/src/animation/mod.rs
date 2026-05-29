// Steel's `register_fn` (used by the `steel_api` submodule) marshals
// values from the Steel VM and requires the registered fn's signature
// to take owned types (`String`, `RVec<u8>`), not borrows. The owned type
// is load-bearing for the FFI dispatcher.
#![allow(clippy::needless_pass_by_value)]

//! Animated media engine. Library-agnostic — accepts any MIME bundle
//! the decoder table understands.

pub mod cache;
pub mod config;
pub mod decoder;
pub mod decoders;
pub mod engine;
pub mod registry;
pub mod renderer;
pub mod renderers;

use std::ffi::{c_char, CStr};
use std::time::Instant;

/// # Safety
/// `mime_ptr` must be a valid pointer to a NUL-terminated C string;
/// `bytes_ptr` must point to a readable buffer of length `bytes_len`;
/// `out_engine_id` must be a valid writable `*mut u64`. All pointers
/// must be valid for the duration of the call.
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
    let mut reg = registry::lock_registry();
    let id = reg.allocate_id();
    let eng = engine::AnimationEngine::new(id, dec, r, 64 * 1024 * 1024);
    reg.insert(id, eng);
    *out_engine_id = id;
    0
}

/// # Safety
/// All four out-pointers (`out_payload_ptr`, `out_payload_len`,
/// `out_height`, `out_next_delay_ms`) must be valid writable pointers
/// for the duration of the call. The caller owns the returned payload
/// buffer and must release it via `nothelix_animation_free_buffer`.
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
    let mut reg = registry::lock_registry();
    let eng = match reg.get_mut(engine_id) {
        Some(e) => e,
        None => return -1,
    };
    let out = if let Some(o) = eng.tick(Instant::now()) {
        o
    } else {
        *out_payload_ptr = std::ptr::null_mut();
        *out_payload_len = 0;
        *out_height = 0;
        *out_next_delay_ms = 0;
        return 2; // finished or paused
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

/// # Safety
/// `ptr` must be a pointer returned by `nothelix_animation_tick` (or
/// null) with the matching `len`. Calling this with a foreign pointer
/// or a different length than was returned is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_free_buffer(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        let slice = std::ptr::slice_from_raw_parts_mut(ptr, len);
        let _ = Box::from_raw(slice);
    }
}

/// # Safety
/// `engine_id` must be one previously returned by
/// `nothelix_animation_register` (or any value — unknown ids are a
/// no-op). The function is safe to call multiple times for the same id;
/// later calls become no-ops.
#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_drop(engine_id: u64) {
    let mut reg = registry::lock_registry();
    if let Some(mut eng) = reg.drop_engine(engine_id) {
        // Emit teardown bytes to clean up the renderer; we drop them since
        // the caller can call drop without first requesting teardown bytes
        // (the plugin sends teardown via tick output during normal flow).
        let _ = eng.teardown();
    }
}

/// # Safety
/// `engine_id` must be one previously returned by
/// `nothelix_animation_register`. Unknown ids return `-1` without
/// touching state. No raw pointers are dereferenced.
#[no_mangle]
pub unsafe extern "C" fn nothelix_animation_set_pause(engine_id: u64, paused: bool) -> i32 {
    let mut reg = registry::lock_registry();
    if let Some(eng) = reg.get_mut(engine_id) {
        let now = Instant::now();
        if paused {
            eng.pause(now)
        } else {
            eng.resume(now)
        }
        0
    } else {
        -1
    }
}

/// Steel-friendly wrappers around the animation registry.
///
/// These use only Steel-marshallable types (`String`, `isize`, `bool`, and
/// bytevectors — `RVec<u8>` for arguments, `FFIValue::ByteVector` for returns,
/// which is all stock steel-core supports for byte payloads).
/// They are separate from the unsafe C-ABI `nothelix_animation_*` exports because
/// Steel cannot call raw `extern "C"` functions through `register_fn`.
///
/// The tick API uses approach (A): `animation_tick_bytes` returns the frame bytes
/// (empty vec when no new frame), and a set of free accessor functions
/// (`animation_tick_status`, etc.) read the metadata from the engine's
/// `last_tick_meta` snapshot which is updated on every `tick()` call.
///
/// Integer types: Steel's `IntV(isize)` maps to `isize` in Rust for both
/// arguments and return values. Using `isize` ensures `IntoFFIVal` and
/// `FromFFIArg` are both satisfied without any wrapper.
pub mod steel_api {
    use super::decoder::lookup_decoder;
    use super::engine::AnimationEngine;
    use super::registry::lock_registry;
    use super::renderer::{select_renderer, TerminalCaps};
    use abi_stable::std_types::RVec;
    use std::time::Instant;
    use steel::steel_vm::ffi::FFIValue;

    /// Register an animation from raw bytes. Returns the `engine_id` (> 0) on
    /// success, or a negative error code:
    ///   -1 lock failure
    ///   -2 unknown MIME type
    ///   -3 decode failure
    pub fn animation_register(mime: String, bytes: RVec<u8>) -> isize {
        let factory = match lookup_decoder(&mime) {
            Some(f) => f,
            None => return -2,
        };
        let dec = match factory(&bytes) {
            Ok(d) => d,
            Err(_) => return -3,
        };
        let caps = TerminalCaps {
            kitty_graphics: true,
            kitty_animation_protocol: false,
            max_fps: 60,
        };
        let r = select_renderer(&caps);
        let mut reg = lock_registry();
        let id = reg.allocate_id();
        let eng = AnimationEngine::new(id, dec, r, 64 * 1024 * 1024);
        reg.insert(id, eng);
        id as isize
    }

    /// Advance the engine by one tick.
    ///
    /// Side effects: updates `last_tick_meta` and `last_tick_bytes` on the
    /// engine. Returns `0` on success, `-1` on lock failure, `-2` when the
    /// `engine_id` is unknown. The actual frame bytes and metadata are read
    /// out via the dedicated accessor functions afterwards
    /// (`animation_tick_bytes`, `_status`, `_height`, `_delay_ms`,
    /// `_frame_index`) — splitting "advance" from "read" keeps a single
    /// scheduling step (`(animation-tick) (animation-tick-status)
    /// (animation-tick-bytes)`) from advancing the engine multiple times
    /// per frame.
    pub fn animation_tick(engine_id: isize) -> isize {
        let mut reg = lock_registry();
        let Some(eng) = reg.get_mut(engine_id as u64) else {
            return -2;
        };
        eng.tick(Instant::now());
        0
    }

    /// Read the frame bytes captured by the most recent `animation_tick`.
    /// Pure accessor — does NOT advance the engine. Returns an empty
    /// bytevector when the last tick had no new frame to emit (status 1)
    /// or when the engine has never been ticked.
    pub fn animation_tick_bytes(engine_id: isize) -> FFIValue {
        let bytes = lock_registry()
            .get(engine_id as u64)
            .map(|e| e.last_tick_bytes.clone())
            .unwrap_or_default();
        FFIValue::ByteVector(RVec::from(bytes))
    }

    /// Return the status code from the last `animation_tick_bytes` call on `engine_id`.
    ///   0 = new frame bytes available
    ///   1 = no change (same frame content, bytes empty)
    ///   2 = finished or paused
    ///  -1 = decode error
    ///  -2 = `engine_id` not found (never registered or already dropped)
    pub fn animation_tick_status(engine_id: isize) -> isize {
        lock_registry()
            .get(engine_id as u64)
            .map_or(-2, |e| e.last_tick_meta.status)
    }

    /// Return the frame height (in terminal cells) from the last tick.
    pub fn animation_tick_height(engine_id: isize) -> isize {
        lock_registry()
            .get(engine_id as u64)
            .map_or(0, |e| e.last_tick_meta.height)
    }

    /// Return the suggested delay until the next tick (milliseconds) from the last tick.
    pub fn animation_tick_delay_ms(engine_id: isize) -> isize {
        lock_registry()
            .get(engine_id as u64)
            .map_or(0, |e| e.last_tick_meta.next_delay_ms)
    }

    /// Return the frame index from the last tick.
    pub fn animation_tick_frame_index(engine_id: isize) -> isize {
        lock_registry()
            .get(engine_id as u64)
            .map_or(0, |e| e.last_tick_meta.frame_index)
    }

    /// Set pause state for the engine. Returns 0 on success, -1 if not found.
    pub fn animation_set_pause(engine_id: isize, paused: bool) -> isize {
        let mut reg = lock_registry();
        let Some(eng) = reg.get_mut(engine_id as u64) else {
            return -1;
        };
        let now = Instant::now();
        if paused {
            eng.pause(now);
        } else {
            eng.resume(now);
        }
        0
    }

    /// Drop the engine and return any renderer teardown bytes (e.g. Kitty
    /// "delete image" escapes) so the caller can flush them to the terminal.
    pub fn animation_drop(engine_id: isize) -> FFIValue {
        let bytes = lock_registry()
            .drop_engine(engine_id as u64)
            .map(|mut eng| eng.teardown())
            .unwrap_or_default();
        FFIValue::ByteVector(RVec::from(bytes))
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
            nothelix_animation_tick(
                id,
                &mut payload_ptr,
                &mut payload_len,
                &mut height,
                &mut delay,
            )
        };
        assert!(rc <= 1);
        if rc == 0 {
            unsafe {
                nothelix_animation_free_buffer(payload_ptr, payload_len);
            }
        }
        unsafe {
            nothelix_animation_drop(id);
        }
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
        let rc =
            unsafe { nothelix_animation_register(std::ptr::null(), std::ptr::null(), 0, &mut id) };
        assert_eq!(rc, -10);
    }
}

#[cfg(test)]
mod steel_api_tests {
    use super::steel_api;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;
    use abi_stable::std_types::RVec;
    use steel::steel_vm::ffi::FFIValue;

    /// Unwrap the `FFIValue::ByteVector` the byte-returning FFI fns
    /// produce — any other variant is a wire-contract break.
    fn bytevector(value: FFIValue) -> RVec<u8> {
        match value {
            FFIValue::ByteVector(bytes) => bytes,
            other => panic!("expected ByteVector, got {other:?}"),
        }
    }

    #[test]
    fn steel_api_register_and_tick() {
        let id = steel_api::animation_register("image/gif".into(), tiny_gif_bytes().into());
        assert!(id > 0, "expected positive engine_id, got {id}");

        // Advance the engine. animation_tick returns 0 on success and
        // publishes the playback status into the last-tick snapshot
        // (accessed via animation_tick_status).
        let advance_rc = steel_api::animation_tick(id);
        assert_eq!(advance_rc, 0, "tick advance failed: {advance_rc}");

        let bytes = bytevector(steel_api::animation_tick_bytes(id));
        let status = steel_api::animation_tick_status(id);
        // Status must be 0 (new frame) or 1 (no change); first tick is always 0.
        assert!(status <= 1, "unexpected status {status}");
        if status == 0 {
            assert!(
                !bytes.is_empty(),
                "status 0 means bytes should be non-empty"
            );
        }

        // Metadata accessors should return sane values.
        let height = steel_api::animation_tick_height(id);
        let delay = steel_api::animation_tick_delay_ms(id);
        let fidx = steel_api::animation_tick_frame_index(id);
        assert!(height >= 0, "height should be non-negative");
        assert!(delay >= 0, "delay should be non-negative");
        assert!(fidx >= 0, "frame_index should be non-negative");

        // Teardown should not panic.
        let _ = steel_api::animation_drop(id);
    }

    #[test]
    fn steel_api_unknown_mime_returns_minus_two() {
        let id: isize =
            steel_api::animation_register("image/nope".into(), RVec::from(vec![0u8; 16]));
        assert_eq!(id, -2isize);
    }

    #[test]
    fn steel_api_pause_resume() {
        let id = steel_api::animation_register("image/gif".into(), tiny_gif_bytes().into());
        assert!(id > 0);

        let rc = steel_api::animation_set_pause(id, true);
        assert_eq!(rc, 0isize, "pause should return 0");

        // Advancing while paused publishes status 2 + empty bytes into
        // the last-tick snapshot. animation_tick itself returns 0 on
        // successful advance.
        let advance_rc = steel_api::animation_tick(id);
        assert_eq!(advance_rc, 0, "tick advance failed: {advance_rc}");

        let status = steel_api::animation_tick_status(id);
        assert_eq!(status, 2isize, "paused tick should publish status 2");
        let bytes = bytevector(steel_api::animation_tick_bytes(id));
        assert!(bytes.is_empty(), "paused tick should publish empty bytes");

        let rc = steel_api::animation_set_pause(id, false);
        assert_eq!(rc, 0isize, "resume should return 0");

        let _ = steel_api::animation_drop(id);
    }

    #[test]
    fn steel_api_status_for_missing_engine_returns_minus_two() {
        assert_eq!(steel_api::animation_tick_status(99998isize), -2isize);
    }
}
