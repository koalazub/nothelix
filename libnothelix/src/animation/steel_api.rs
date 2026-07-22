use crate::animation::engine::{TickMetaSnapshot, TickStatus};
use crate::animation::registry::{
    lock_registry, register_engine, retire_engine, set_engine_paused,
};
use abi_stable::std_types::RVec;
use std::time::Instant;
use steel::steel_vm::ffi::FFIValue;

pub fn animation_register(mime: String, bytes: RVec<u8>) -> isize {
    match register_engine(&mime, &bytes) {
        Ok(engine_id) => engine_id as isize,
        Err(error) => error.code() as isize,
    }
}

pub fn animation_tick(engine_id: isize) -> isize {
    match lock_registry().get_mut(engine_id as u64) {
        Some(engine) => {
            engine.tick(Instant::now());
            0
        }
        None => TickStatus::UnknownEngine.code() as isize,
    }
}

pub fn animation_tick_bytes(engine_id: isize) -> FFIValue {
    let bytes = match lock_registry().get(engine_id as u64) {
        Some(engine) => engine.last_tick_bytes.clone(),
        None => Vec::new(),
    };
    FFIValue::ByteVector(RVec::from(bytes))
}

pub fn animation_tick_status(engine_id: isize) -> isize {
    match last_tick(engine_id) {
        Some(meta) => meta.status.code() as isize,
        None => TickStatus::UnknownEngine.code() as isize,
    }
}

pub fn animation_tick_height(engine_id: isize) -> isize {
    last_tick(engine_id).map_or(0, |meta| meta.height as isize)
}

pub fn animation_tick_delay_ms(engine_id: isize) -> isize {
    last_tick(engine_id).map_or(0, |meta| meta.next_delay_ms as isize)
}

pub fn animation_tick_frame_index(engine_id: isize) -> isize {
    last_tick(engine_id).map_or(0, |meta| meta.frame_index as isize)
}

pub fn animation_set_pause(engine_id: isize, paused: bool) -> isize {
    set_engine_paused(engine_id as u64, paused).code() as isize
}

pub fn animation_drop(engine_id: isize) -> FFIValue {
    FFIValue::ByteVector(RVec::from(retire_engine(engine_id as u64)))
}

fn last_tick(engine_id: isize) -> Option<TickMetaSnapshot> {
    lock_registry()
        .get(engine_id as u64)
        .map(|engine| engine.last_tick_meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::decoders::gif_fixture::tiny_gif_bytes;

    fn bytevector(value: FFIValue) -> RVec<u8> {
        match value {
            FFIValue::ByteVector(bytes) => bytes,
            other => panic!("expected ByteVector, got {other:?}"),
        }
    }

    #[test]
    fn steel_api_register_and_tick() {
        let id = animation_register("image/gif".into(), tiny_gif_bytes().into());
        assert!(id > 0, "expected positive engine_id, got {id}");

        let advance_rc = animation_tick(id);
        assert_eq!(advance_rc, 0, "tick advance failed: {advance_rc}");

        let bytes = bytevector(animation_tick_bytes(id));
        let status = animation_tick_status(id);
        assert!(status <= 1, "unexpected status {status}");
        if status == 0 {
            assert!(
                !bytes.is_empty(),
                "status 0 means bytes should be non-empty"
            );
        }

        assert!(animation_tick_height(id) >= 0);
        assert!(animation_tick_delay_ms(id) >= 0);
        assert!(animation_tick_frame_index(id) >= 0);

        let _ = animation_drop(id);
    }

    #[test]
    fn steel_api_unknown_mime_returns_minus_two() {
        let id = animation_register("image/nope".into(), RVec::from(vec![0u8; 16]));
        assert_eq!(id, -2isize);
    }

    #[test]
    fn steel_api_pause_resume() {
        let id = animation_register("image/gif".into(), tiny_gif_bytes().into());
        assert!(id > 0);

        assert_eq!(
            animation_set_pause(id, true),
            0isize,
            "pause should return 0"
        );

        let advance_rc = animation_tick(id);
        assert_eq!(advance_rc, 0, "tick advance failed: {advance_rc}");

        assert_eq!(
            animation_tick_status(id),
            2isize,
            "paused tick should publish status 2"
        );
        let bytes = bytevector(animation_tick_bytes(id));
        assert!(bytes.is_empty(), "paused tick should publish empty bytes");

        assert_eq!(
            animation_set_pause(id, false),
            0isize,
            "resume should return 0"
        );

        let _ = animation_drop(id);
    }

    #[test]
    fn steel_api_status_for_missing_engine_returns_minus_two() {
        assert_eq!(animation_tick_status(99998isize), -2isize);
    }
}
