use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::voice::VoiceId;

const QUEUE_CAPACITY: usize = 256;
const QUEUE_MASK: usize = QUEUE_CAPACITY - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCategoryValue {
    Sfx,
    Music,
    Ambient,
}

pub enum AudioCommand {
    Stop(VoiceId),
    StopAll,
    SetVolume(VoiceId, f32),
    SetPan(VoiceId, f32),
    SetPitch(VoiceId, f32),
    SetMasterVolume(f32),
    SetCategoryVolume(AudioCategoryValue, f32),
}

struct CommandSlot {
    command: UnsafeCell<Option<AudioCommand>>,
}

pub struct CommandQueue {
    slots: Box<[CommandSlot]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl CommandQueue {
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(QUEUE_CAPACITY);
        for _ in 0..QUEUE_CAPACITY {
            slots.push(CommandSlot {
                command: UnsafeCell::new(None),
            });
        }
        CommandQueue {
            slots: slots.into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, cmd: AudioCommand) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) & QUEUE_MASK;
        if next_tail == self.head.load(Ordering::Acquire) {
            return false;
        }
        unsafe {
            *self.slots[tail].command.get() = Some(cmd);
        }
        self.tail.store(next_tail, Ordering::Release);
        true
    }

    pub fn pop(&self) -> Option<AudioCommand> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let cmd = unsafe { (*self.slots[head].command.get()).take() }?;
        self.head.store((head + 1) & QUEUE_MASK, Ordering::Release);
        Some(cmd)
    }
}

unsafe impl Send for CommandQueue {}
unsafe impl Sync for CommandQueue {}
