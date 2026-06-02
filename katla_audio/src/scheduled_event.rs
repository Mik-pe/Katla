use std::sync::Arc;

use crate::buffer::AudioBuffer;
use crate::command_queue::AudioCategoryValue;
use crate::voice::{VoiceId, VoicePriority};

pub enum ScheduledEvent {
    Play {
        buffer: Arc<AudioBuffer>,
        category: AudioCategoryValue,
        priority: VoicePriority,
        time_secs: f64,
    },
    Stop {
        voice_id: VoiceId,
        time_secs: f64,
    },
    SetVolume {
        voice_id: VoiceId,
        volume: f32,
        time_secs: f64,
    },
}
