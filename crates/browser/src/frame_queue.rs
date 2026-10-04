use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
pub(crate) struct FrameQueue {
    pending: Arc<AtomicUsize>,
}

pub(crate) struct FramePermit(Arc<AtomicUsize>);

impl FrameQueue {
    pub(crate) fn try_acquire(&self) -> Option<FramePermit> {
        // Two slots overlap CPU/GPU work while bounding stale submitted views.
        self.pending
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending < 2).then_some(pending + 1)
            })
            .ok()
            .map(|_| FramePermit(Arc::clone(&self.pending)))
    }
}

impl Drop for FramePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_two_frames_and_coalesces_until_one_completes() {
        let queue = FrameQueue::default();
        let first = queue.try_acquire().unwrap();
        let second = queue.try_acquire().unwrap();
        assert!(queue.try_acquire().is_none());
        drop(first);
        let third = queue.try_acquire().unwrap();
        assert!(queue.try_acquire().is_none());
        drop(second);
        drop(third);
        assert!(queue.try_acquire().is_some());
    }

    #[test]
    fn an_abandoned_frame_releases_its_slot() {
        let queue = FrameQueue::default();
        drop(queue.try_acquire().unwrap());
        let _first = queue.try_acquire().unwrap();
        let _second = queue.try_acquire().unwrap();
        assert!(queue.try_acquire().is_none());
    }
}
