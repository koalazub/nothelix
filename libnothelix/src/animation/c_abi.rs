use crate::animation::registry::{
    lock_registry, register_engine, retire_engine, set_engine_paused,
};
use std::ffi::{CStr, c_char};
use std::time::Instant;

#[repr(i32)]
enum RegisterCode {
    Registered = 0,
    MimeNotUtf8 = -1,
    NullArgument = -10,
}

#[repr(i32)]
enum TickCode {
    Frame = 0,
    NoChange = 1,
    Stopped = 2,
    UnknownEngine = -1,
    NullArgument = -10,
}

/// # Safety
/// `mime_ptr` must point to a NUL-terminated C string; `bytes_ptr` must point
/// to a readable buffer of length `bytes_len`; `out_engine_id` must be a valid
/// writable `*mut u64`. All pointers must stay valid for the whole call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nothelix_animation_register(
    mime_ptr: *const c_char,
    bytes_ptr: *const u8,
    bytes_len: usize,
    out_engine_id: *mut u64,
) -> i32 {
    unsafe {
        if mime_ptr.is_null() || bytes_ptr.is_null() || out_engine_id.is_null() {
            return RegisterCode::NullArgument as i32;
        }
        let Ok(mime) = CStr::from_ptr(mime_ptr).to_str() else {
            return RegisterCode::MimeNotUtf8 as i32;
        };
        match register_engine(mime, std::slice::from_raw_parts(bytes_ptr, bytes_len)) {
            Ok(engine_id) => {
                *out_engine_id = engine_id;
                RegisterCode::Registered as i32
            }
            Err(error) => error.code(),
        }
    }
}

/// # Safety
/// All four out-pointers must be valid writable pointers for the whole call.
/// The caller owns the returned payload buffer and must release it through
/// `nothelix_animation_free_buffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nothelix_animation_tick(
    engine_id: u64,
    out_payload_ptr: *mut *mut u8,
    out_payload_len: *mut usize,
    out_height: *mut u16,
    out_next_delay_ms: *mut u32,
) -> i32 {
    unsafe {
        if out_payload_ptr.is_null()
            || out_payload_len.is_null()
            || out_height.is_null()
            || out_next_delay_ms.is_null()
        {
            return TickCode::NullArgument as i32;
        }
        let mut registry = lock_registry();
        let Some(engine) = registry.get_mut(engine_id) else {
            return TickCode::UnknownEngine as i32;
        };
        let Some(output) = engine.tick(Instant::now()) else {
            *out_payload_ptr = std::ptr::null_mut();
            *out_payload_len = 0;
            *out_height = 0;
            *out_next_delay_ms = 0;
            return TickCode::Stopped as i32;
        };
        *out_height = output.height;
        *out_next_delay_ms = output.next_delay_ms;
        if output.bytes.is_empty() {
            *out_payload_ptr = std::ptr::null_mut();
            *out_payload_len = 0;
            return TickCode::NoChange as i32;
        }
        let payload = Box::<[u8]>::from(output.bytes);
        *out_payload_len = payload.len();
        *out_payload_ptr = Box::into_raw(payload).cast::<u8>();
        TickCode::Frame as i32
    }
}

/// # Safety
/// `ptr` must be a pointer returned by `nothelix_animation_tick` (or null)
/// paired with the exact `len` that call reported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nothelix_animation_free_buffer(ptr: *mut u8, len: usize) {
    unsafe {
        if !ptr.is_null() && len > 0 {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
        }
    }
}

/// # Safety
/// No raw pointers are dereferenced; unknown ids are a no-op, so repeated
/// calls for the same id are safe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nothelix_animation_drop(engine_id: u64) {
    let _ = retire_engine(engine_id);
}

/// # Safety
/// No raw pointers are dereferenced; unknown ids return `-1` without touching
/// any engine state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nothelix_animation_set_pause(engine_id: u64, paused: bool) -> i32 {
    set_engine_paused(engine_id, paused).code()
}

#[cfg(test)]
mod tests {
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
