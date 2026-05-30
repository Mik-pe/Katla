use std::sync::Arc;

use crate::buffer::AudioBuffer;
use crate::command_queue::AudioCategoryValue;
use crate::voice::{VoiceId, VoicePriority};

pub enum ScheduledEvent {
    PlayAt {
        buffer: Arc<AudioBuffer>,
        category: AudioCategoryValue,
        priority: VoicePriority,
        time_secs: f64,
    },
    StopAt {
        voice_id: VoiceId,
        time_secs: f64,
    },
    SetVolumeAt {
        voice_id: VoiceId,
        volume: f32,
        time_secs: f64,
    },
}
