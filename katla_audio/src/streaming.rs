use std::path::Path;

use crate::buffer::AudioBuffer;
use crate::error::AudioError;

const STREAM_CHUNK_SAMPLES: usize = 44100 * 2 * 2;

pub struct StreamingDecoder {
    inner: StreamingDecoderInner,
    channels: u16,
    sample_rate: u32,
    exhausted: bool,
}

enum StreamingDecoderInner {
    Wav(hound::WavReader<std::io::BufReader<std::fs::File>>),
    Ogg(OggStreamState),
}

struct OggStreamState {
    reader: lewton::inside_ogg::OggStreamReader<std::io::BufReader<std::fs::File>>,
}

impl StreamingDecoder {
    pub fn open_wav(path: &Path) -> Result<Self, AudioError> {
        let reader = hound::WavReader::open(path).map_err(|e| {
            AudioError::DecodeFailed(format!("Failed to open WAV for streaming: {e}"))
        })?;
        let spec = reader.spec();
        Ok(StreamingDecoder {
            inner: StreamingDecoderInner::Wav(reader),
            channels: spec.channels,
            sample_rate: spec.sample_rate,
            exhausted: false,
        })
    }

    pub fn open_ogg(path: &Path) -> Result<Self, AudioError> {
        use std::fs::File;
        let file = File::open(path).map_err(AudioError::Io)?;
        let reader = lewton::inside_ogg::OggStreamReader::new(std::io::BufReader::new(file))
            .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse OGG: {e}")))?;
        let channels = reader.ident_hdr.audio_channels as u16;
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        Ok(StreamingDecoder {
            inner: StreamingDecoderInner::Ogg(OggStreamState { reader }),
            channels,
            sample_rate,
            exhausted: false,
        })
    }

    pub fn open(path: &Path) -> Result<Self, AudioError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("wav") | Some("WAV") => Self::open_wav(path),
            Some("ogg") | Some("OGG") => Self::open_ogg(path),
            _ => Err(AudioError::FormatUnsupported(format!(
                "Unsupported streaming format: {}",
                path.display()
            ))),
        }
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

        match &mut self.inner {
            StreamingDecoderInner::Wav(reader) => {
                let spec = reader.spec();
                let mut samples = Vec::with_capacity(STREAM_CHUNK_SAMPLES);
                let remaining = STREAM_CHUNK_SAMPLES;
                match spec.sample_format {
                    hound::SampleFormat::Float => {
                        for s in reader.samples::<f32>().take(remaining) {
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
                        for s in reader.samples::<i16>().take(remaining) {
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
            StreamingDecoderInner::Ogg(state) => {
                let mut all_samples = Vec::with_capacity(STREAM_CHUNK_SAMPLES);
                let mut samples_collected = 0;
                while samples_collected < STREAM_CHUNK_SAMPLES {
                    let packet = state.reader.read_dec_packet().ok().flatten();
                    match packet {
                        Some(frame_samples) => {
                            for frame in frame_samples {
                                for sample in frame {
                                    all_samples.push(sample as f32 / i16::MAX as f32);
                                }
                            }
                            samples_collected = all_samples.len();
                        }
                        None => {
                            self.exhausted = true;
                            break;
                        }
                    }
                }
                if all_samples.is_empty() {
                    self.exhausted = true;
                    return None;
                }
                Some(AudioBuffer {
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    samples: all_samples,
                })
            }
        }
    }

    pub fn seek_to_start(&mut self) -> Result<(), AudioError> {
        match &mut self.inner {
            StreamingDecoderInner::Wav(reader) => {
                reader
                    .seek(0)
                    .map_err(|e| AudioError::DecodeFailed(format!("Seek failed: {e}")))?;
            }
            StreamingDecoderInner::Ogg(state) => {
                state
                    .reader
                    .seek_absgp_pg(0)
                    .map_err(|e| AudioError::DecodeFailed(format!("OGG seek failed: {e}")))?;
            }
        }
        self.exhausted = false;
        Ok(())
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
