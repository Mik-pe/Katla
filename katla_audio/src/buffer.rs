use std::path::Path;

use crate::error::AudioError;

pub enum SampleFormat {
    F32(Vec<f32>),
    I16(Vec<i16>),
}

pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: SampleFormat,
}

pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn from_decoded(decoded: DecodedAudio) -> Self {
        let samples = match decoded.samples {
            SampleFormat::F32(s) => s,
            SampleFormat::I16(s) => s.iter().map(|&s| s as f32 / i16::MAX as f32).collect(),
        };
        AudioBuffer {
            sample_rate: decoded.sample_rate,
            channels: decoded.channels,
            samples,
        }
    }
}

pub fn load_wav(path: &Path) -> Result<AudioBuffer, AudioError> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to open WAV: {e}")))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.unwrap_or(0.0))
            .collect(),
        hound::SampleFormat::Int => reader
            .into_samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32).unwrap_or(0.0))
            .collect(),
    };

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples,
    })
}

pub fn load_ogg(path: &Path) -> Result<AudioBuffer, AudioError> {
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).map_err(|e| AudioError::Io(e))?;
    let mut reader = lewton::inside_ogg::OggStreamReader::new(BufReader::new(file))
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse OGG: {e}")))?;

    let mut all_samples: Vec<f32> = Vec::new();
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels as u16;

    loop {
        let packet = reader
            .read_dec_packet()
            .map_err(|e| AudioError::DecodeFailed(format!("OGG decode error: {e}")))?;
        match packet {
            Some(samples) => {
                for frame in samples {
                    for sample in frame {
                        all_samples.push(sample as f32 / i16::MAX as f32);
                    }
                }
            }
            None => break,
        }
    }

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples: all_samples,
    })
}

pub fn load_mp3(path: &Path) -> Result<AudioBuffer, AudioError> {
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).map_err(|e| AudioError::Io(e))?;
    let mut reader = minimp3::Decoder::new(BufReader::new(file));

    let mut all_samples: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;
    let mut channels = 0u16;

    loop {
        match reader.next_frame() {
            Ok(frame) => {
                sample_rate = frame.sample_rate as u32;
                channels = frame.channels as u16;
                for sample in frame.data {
                    all_samples.push(sample as f32 / i16::MAX as f32);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(e) => {
                return Err(AudioError::DecodeFailed(format!("MP3 decode error: {e}")));
            }
        }
    }

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples: all_samples,
    })
}

pub fn load_flac(path: &Path) -> Result<AudioBuffer, AudioError> {
    let mut reader = claxon::FlacReader::open(path)
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse FLAC: {e}")))?;

    let info = reader.streaminfo();
    let sample_rate = info.sample_rate;
    let channels = info.channels as u16;
    let bits_per_sample = info.bits_per_sample;

    let all_samples: Vec<f32> = reader
        .samples()
        .map(|s| {
            s.map(|s| match bits_per_sample {
                16 => (s as i16) as f32 / i16::MAX as f32,
                24 => (s >> 8) as i16 as f32 / i16::MAX as f32,
                32 => s as f32 / i32::MAX as f32,
                _ => s as f32 / (1i64 << (bits_per_sample - 1)) as f32,
            })
            .unwrap_or(0.0)
        })
        .collect();

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples: all_samples,
    })
}

pub fn load_audio(path: &Path) -> Result<AudioBuffer, AudioError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("wav") | Some("WAV") => load_wav(path),
        Some("ogg") | Some("OGG") => load_ogg(path),
        Some("mp3") | Some("MP3") => load_mp3(path),
        Some("flac") | Some("FLAC") => load_flac(path),
        _ => Err(AudioError::FormatUnsupported(format!(
            "Unsupported audio format: {}",
            path.display()
        ))),
    }
}
