use crate::animation::decoder::DecodedFrame;
use std::collections::VecDeque;

pub struct FrameCache {
    budget_bytes: usize,
    used_bytes: usize,
    entries: VecDeque<(u64, DecodedFrame)>, // (frame_index, frame); back = most recent
}

impl FrameCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            entries: VecDeque::new(),
        }
    }

    /// Look up a cached frame by `frame_index`. On hit, promotes the entry to most-recent.
    pub fn get(&mut self, frame_index: u64) -> Option<DecodedFrame> {
        let pos = self.entries.iter().position(|(i, _)| *i == frame_index)?;
        let entry = self.entries.remove(pos)?;
        let frame = entry.1.clone();
        self.entries.push_back(entry);
        Some(frame)
    }

    /// Insert a frame, evicting LRU entries until under budget.
    /// If a single frame exceeds the budget, refuse the insert (keep existing entries).
    pub fn put(&mut self, frame: DecodedFrame) {
        let frame_size = frame.rgba.len();
        if frame_size > self.budget_bytes {
            return;
        }
        // Replace existing entry for the same frame_index.
        if let Some(pos) = self.entries.iter().position(|(i, _)| *i == frame.frame_index) {
            if let Some(old) = self.entries.remove(pos) {
                self.used_bytes = self.used_bytes.saturating_sub(old.1.rgba.len());
            }
        }
        while self.used_bytes + frame_size > self.budget_bytes {
            if let Some((_, evicted)) = self.entries.pop_front() {
                self.used_bytes = self.used_bytes.saturating_sub(evicted.rgba.len());
            } else {
                break;
            }
        }
        self.used_bytes += frame_size;
        self.entries.push_back((frame.frame_index, frame));
    }

    pub fn used(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn make_frame(idx: u64, size: usize) -> DecodedFrame {
        DecodedFrame {
            rgba: Arc::from(vec![0u8; size].as_slice()),
            width: 1,
            height: 1,
            frame_index: idx,
            presentation_offset: Duration::ZERO,
            content_id: idx,
        }
    }

    #[test]
    fn lru_evicts_when_over_budget() {
        let mut c = FrameCache::new(250);
        c.put(make_frame(0, 100));
        c.put(make_frame(1, 100));
        c.put(make_frame(2, 100)); // forces eviction of 0
        assert!(c.get(0).is_none());
        assert!(c.get(1).is_some());
        assert!(c.get(2).is_some());
        assert!(c.used() <= 250);
    }

    #[test]
    fn get_promotes_to_recent() {
        let mut c = FrameCache::new(250);
        c.put(make_frame(0, 100));
        c.put(make_frame(1, 100));
        let _ = c.get(0); // promote 0
        c.put(make_frame(2, 100)); // should evict 1, not 0
        assert!(c.get(0).is_some());
        assert!(c.get(1).is_none());
    }

    #[test]
    fn put_replaces_same_index() {
        let mut c = FrameCache::new(1_000);
        c.put(make_frame(5, 100));
        c.put(make_frame(5, 200)); // replace
        assert_eq!(c.len(), 1);
        assert_eq!(c.used(), 200);
    }

    #[test]
    fn frame_larger_than_budget_is_refused() {
        let mut c = FrameCache::new(50);
        c.put(make_frame(0, 100)); // refused
        assert!(c.is_empty());
        assert_eq!(c.used(), 0);
    }
}
