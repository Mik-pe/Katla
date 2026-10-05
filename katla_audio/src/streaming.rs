use std::path::Path;
use std::time::Duration;

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
    Ogg(Box<OggStreamState>),
    Mp3(Mp3StreamState),
    Flac(FlacStreamState),
}

struct OggStreamState {
    reader: lewton::inside_ogg::OggStreamReader<std::io::BufReader<std::fs::File>>,
}

struct Mp3StreamState {
    format_reader: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
    track_id: u32,
    scratch: Vec<f32>,
}

struct FlacStreamState {
    reader: claxon::FlacReader<std::io::BufReader<std::fs::File>>,
    path: std::path::PathBuf,
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
            inner: StreamingDecoderInner::Ogg(Box::new(OggStreamState { reader })),
            channels,
            sample_rate,
            exhausted: false,
        })
    }

    pub fn open_mp3(path: &Path) -> Result<Self, AudioError> {
        use std::fs::File;
        use symphonia::core::codecs::CodecParameters;
        use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::formats::probe::Hint;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;

        let file = File::open(path).map_err(AudioError::Io)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_reader = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| AudioError::DecodeFailed(format!("Failed to probe MP3: {e}")))?;

        let (track_id, params) = format_reader
            .tracks()
            .iter()
            .find_map(|t| match &t.codec_params {
                Some(CodecParameters::Audio(params)) if params.codec != CODEC_ID_NULL_AUDIO => {
                    Some((t.id, params))
                }
                _ => None,
            })
            .ok_or_else(|| AudioError::DecodeFailed("No supported audio track found".into()))?;

        let sample_rate = params.sample_rate.unwrap_or(44100);
        let channels = params
            .channels
            .as_ref()
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| AudioError::DecodeFailed(format!("Failed to create MP3 decoder: {e}")))?;

        Ok(StreamingDecoder {
            inner: StreamingDecoderInner::Mp3(Mp3StreamState {
                format_reader,
                decoder,
                track_id,
                scratch: Vec::new(),
            }),
            channels,
            sample_rate,
            exhausted: false,
        })
    }

    pub fn open_flac(path: &Path) -> Result<Self, AudioError> {
        use std::fs::File;
        let file = File::open(path).map_err(AudioError::Io)?;
        let reader = claxon::FlacReader::new(std::io::BufReader::new(file))
            .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse FLAC: {e}")))?;
        let info = reader.streaminfo();
        Ok(StreamingDecoder {
            inner: StreamingDecoderInner::Flac(FlacStreamState {
                reader,
                path: path.to_path_buf(),
            }),
            channels: info.channels as u16,
            sample_rate: info.sample_rate,
            exhausted: false,
        })
    }

    pub fn open(path: &Path) -> Result<Self, AudioError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("wav") | Some("WAV") => Self::open_wav(path),
            Some("ogg") | Some("OGG") => Self::open_ogg(path),
            Some("mp3") | Some("MP3") => Self::open_mp3(path),
            Some("flac") | Some("FLAC") => Self::open_flac(path),
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
            StreamingDecoderInner::Mp3(state) => {
                let mut all_samples = Vec::with_capacity(STREAM_CHUNK_SAMPLES);
                loop {
                    let packet = match state.format_reader.next_packet() {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            self.exhausted = true;
                            break;
                        }
                        Err(_) => {
                            break;
                        }
                    };

                    if packet.track_id != state.track_id {
                        continue;
                    }

                    let decoded = match state.decoder.decode(&packet) {
                        Ok(d) => d,
                        Err(_) => break,
                    };

                    state.scratch.clear();
                    decoded.copy_to_vec_interleaved(&mut state.scratch);
                    all_samples.extend_from_slice(&state.scratch);

                    if all_samples.len() >= STREAM_CHUNK_SAMPLES {
                        break;
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
            StreamingDecoderInner::Flac(state) => {
                let info = state.reader.streaminfo();
                let bits_per_sample = info.bits_per_sample;
                let mut all_samples = Vec::with_capacity(STREAM_CHUNK_SAMPLES);
                for s in state.reader.samples().take(STREAM_CHUNK_SAMPLES) {
                    match s {
                        Ok(s) => {
                            let v = match bits_per_sample {
                                16 => (s as i16) as f32 / i16::MAX as f32,
                                24 => s as f32 / 8388608.0,
                                32 => s as f32 / i32::MAX as f32,
                                _ => s as f32 / (1i64 << (bits_per_sample - 1)) as f32,
                            };
                            all_samples.push(v);
                        }
                        Err(_) => {
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
            StreamingDecoderInner::Mp3(state) => {
                use symphonia::core::formats::{SeekMode, SeekTo};
                use symphonia::core::units::Time;
                let seek_to = SeekTo::Time {
                    time: Time::ZERO,
                    track_id: Some(state.track_id),
                };
                state
                    .format_reader
                    .seek(SeekMode::Accurate, seek_to)
                    .map_err(|e| AudioError::DecodeFailed(format!("MP3 seek failed: {e}")))?;
            }
            StreamingDecoderInner::Flac(state) => {
                use std::fs::File;
                let file = File::open(&state.path).map_err(AudioError::Io)?;
                state.reader = claxon::FlacReader::new(std::io::BufReader::new(file))
                    .map_err(|e| AudioError::DecodeFailed(format!("FLAC seek failed: {e}")))?;
            }
        }
        self.exhausted = false;
        Ok(())
    }

    pub fn seek(&mut self, position: Duration) -> Result<(), AudioError> {
        if position <= Duration::ZERO {
            return self.seek_to_start();
        }

        let target_frame = (position.as_secs_f64() * self.sample_rate as f64).round() as u64;

        match &mut self.inner {
            StreamingDecoderInner::Wav(reader) => {
                reader
                    .seek(target_frame as u32)
                    .map_err(|e| AudioError::DecodeFailed(format!("WAV seek failed: {e}")))?;
            }
            StreamingDecoderInner::Ogg(state) => {
                state
                    .reader
                    .seek_absgp_pg(target_frame)
                    .map_err(|e| AudioError::DecodeFailed(format!("OGG seek failed: {e}")))?;
            }
            StreamingDecoderInner::Mp3(state) => {
                use symphonia::core::formats::{SeekMode, SeekTo};
                use symphonia::core::units::Time;
                let seek_to = SeekTo::Time {
                    time: Time::try_new(position.as_secs() as i64, position.subsec_nanos())
                        .expect("std Duration nanos are always valid Time nanos"),
                    track_id: Some(state.track_id),
                };
                state
                    .format_reader
                    .seek(SeekMode::Accurate, seek_to)
                    .map_err(|e| AudioError::DecodeFailed(format!("MP3 seek failed: {e}")))?;
            }
            StreamingDecoderInner::Flac(state) => {
                use std::fs::File;
                let file = File::open(&state.path).map_err(AudioError::Io)?;
                state.reader = claxon::FlacReader::new(std::io::BufReader::new(file))
                    .map_err(|e| AudioError::DecodeFailed(format!("FLAC seek failed: {e}")))?;
                let skip_samples = target_frame as usize * self.channels as usize;
                for _ in state.reader.samples().take(skip_samples) {
                    // discard
                }
            }
        }
        self.exhausted = false;
        Ok(())
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
