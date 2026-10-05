use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelLevels {
    pub peak: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LevelsSnapshot {
    pub master: ChannelLevels,
    pub sfx: ChannelLevels,
    pub music: ChannelLevels,
    pub ambient: ChannelLevels,
}

pub struct LevelsBuffer {
    buffers: UnsafeCell<[LevelsSnapshot; 2]>,
    index: AtomicUsize,
}

// SAFETY: LevelsBuffer uses a double-buffer pattern where the audio thread
// writes to slot `index` and the main thread reads from the other slot after
// swapping. Only one thread accesses each slot at a time.
unsafe impl Sync for LevelsBuffer {}

impl LevelsBuffer {
    pub fn new() -> Self {
        LevelsBuffer {
            buffers: UnsafeCell::new([LevelsSnapshot::default(); 2]),
            index: AtomicUsize::new(0),
        }
    }

    pub fn write(&self, snapshot: &LevelsSnapshot) {
        let idx = self.index.load(Ordering::Relaxed);
        // SAFETY: Called from the audio thread only. The main thread reads
        // the other slot after swap, so there is no concurrent access.
        unsafe {
            (*self.buffers.get())[idx] = *snapshot;
        }
    }

    pub fn read(&self) -> LevelsSnapshot {
        let previous = self.index.fetch_xor(1, Ordering::AcqRel);
        // SAFETY: After the swap, `previous` is now the write slot for the
        // audio thread. The main thread reads the snapshot that was just
        // retired — no concurrent access.
        unsafe { (*self.buffers.get())[previous] }
    }
}
