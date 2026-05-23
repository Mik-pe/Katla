use std::path::Path;

use crate::buffer::AudioBuffer;

const STREAM_CHUNK_SAMPLES: usize = 44100 * 2 * 2; // ~2 seconds of stereo at 44.1kHz

pub struct StreamingDecoder {
    wav_reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    channels: u16,
    sample_rate: u32,
    exhausted: bool,
}

impl StreamingDecoder {
    pub fn open_wav(path: &Path) -> Result<Self, String> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| format!("Failed to open WAV for streaming: {e}"))?;
        let spec = reader.spec();
        Ok(StreamingDecoder {
            wav_reader: reader,
            channels: spec.channels,
            sample_rate: spec.sample_rate,
            exhausted: false,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn read_chunk(&mut self) -> Option<AudioBuffer> {
        if self.exhausted {
            return None;
        }

        let spec = self.wav_reader.spec();
        let mut samples = Vec::with_capacity(STREAM_CHUNK_SAMPLES);

        let remaining = STREAM_CHUNK_SAMPLES;
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.wav_reader.samples::<f32>().take(remaining) {
                    match s {
                        Ok(v) => samples.push(v),
                        Err(_) => {
                            self.exhausted = true;
                            break;
                        }
                    }
                }
            }
            hound::SampleFormat::Int => {
                for s in self.wav_reader.samples::<i16>().take(remaining) {
                    match s {
                        Ok(v) => samples.push(v as f32 / i16::MAX as f32),
                        Err(_) => {
                            self.exhausted = true;
                            break;
                        }
                    }
                }
            }
        }

        if samples.is_empty() {
            self.exhausted = true;
            return None;
        }

        Some(AudioBuffer {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples,
        })
    }

    pub fn seek_to_start(&mut self) -> Result<(), String> {
        self.wav_reader
            .seek(0)
            .map_err(|e| format!("Seek failed: {e}"))?;
        self.exhausted = false;
        Ok(())
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
