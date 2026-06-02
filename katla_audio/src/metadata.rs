use std::path::Path;

use crate::error::AudioError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Ogg,
    Mp3,
    Flac,
}

#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_count: u64,
    pub duration_secs: f64,
}

fn detect_format(path: &Path) -> Result<AudioFormat, AudioError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("wav") | Some("WAV") => Ok(AudioFormat::Wav),
        Some("ogg") | Some("OGG") => Ok(AudioFormat::Ogg),
        Some("mp3") | Some("MP3") => Ok(AudioFormat::Mp3),
        Some("flac") | Some("FLAC") => Ok(AudioFormat::Flac),
        _ => Err(AudioError::FormatUnsupported(format!(
            "Unsupported audio format: {}",
            path.display()
        ))),
    }
}

fn metadata_wav(path: &Path) -> Result<AudioMetadata, AudioError> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to read WAV header: {e}")))?;
    let spec = reader.spec();
    let duration = reader.duration();
    let sample_count = duration as u64 * spec.channels as u64;
    let duration_secs = if spec.sample_rate > 0 {
        duration as f64 / spec.sample_rate as f64
    } else {
        0.0
    };

    Ok(AudioMetadata {
        format: AudioFormat::Wav,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        sample_count,
        duration_secs,
    })
}

fn metadata_ogg(path: &Path) -> Result<AudioMetadata, AudioError> {
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).map_err(AudioError::Io)?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let reader = lewton::inside_ogg::OggStreamReader::new(BufReader::new(file))
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse OGG header: {e}")))?;

    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels as u16;
    let (sample_count, duration_secs) = if sample_rate > 0 && file_size > 0 {
        let avg_bytes_per_sample: f64 = 6000.0;
        let estimated_samples =
            (file_size as f64 / avg_bytes_per_sample * sample_rate as f64 / 44100.0).round() as u64;
        (
            estimated_samples,
            estimated_samples as f64 / sample_rate as f64,
        )
    } else {
        (0, 0.0)
    };

    Ok(AudioMetadata {
        format: AudioFormat::Ogg,
        sample_rate,
        channels,
        sample_count,
        duration_secs,
    })
}

fn metadata_mp3(path: &Path) -> Result<AudioMetadata, AudioError> {
    use std::fs::File;
    use std::io::{Read as _, Seek, SeekFrom};
    use symphonia::core::codecs::CODEC_TYPE_NULL;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let mut file = File::open(path).map_err(AudioError::Io)?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut buf = [0u8; 4096];
    let bytes_read = file.read(&mut buf).map_err(AudioError::Io)?;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut found = false;

    let bitrate_table: [[u16; 16]; 3] = [
        [
            0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0,
        ],
        [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0,
        ],
        [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
        ],
    ];
    let sample_rate_table: [[u32; 4]; 3] = [
        [44100, 48000, 32000, 0],
        [22050, 24000, 16000, 0],
        [11025, 12000, 8000, 0],
    ];
    let samples_per_frame: [u16; 3] = [1152, 1152, 384];

    let mut pos = 0usize;
    while pos + 4 <= bytes_read {
        if buf[pos] == 0xFF && (buf[pos + 1] & 0xE0) == 0xE0 {
            let header = (buf[pos] as u32) << 24
                | (buf[pos + 1] as u32) << 16
                | (buf[pos + 2] as u32) << 8
                | buf[pos + 3] as u32;

            let version_bits = (header >> 19) & 0x3;
            let layer_bits = (header >> 17) & 0x3;
            let bitrate_idx = ((header >> 12) & 0xF) as usize;
            let sr_idx = ((header >> 10) & 0x3) as usize;
            let _padding = (header >> 9) & 0x1;
            let channel_mode = (header >> 6) & 0x3;

            let version = match version_bits {
                3 => 0,
                2 => 1,
                0 => 2,
                _ => {
                    pos += 1;
                    continue;
                }
            };
            let _layer = match layer_bits {
                3 => 0,
                _ => {
                    pos += 1;
                    continue;
                }
            };

            if bitrate_idx == 0 || bitrate_idx == 15 || sr_idx == 3 {
                pos += 1;
                continue;
            }

            sample_rate = sample_rate_table[version][sr_idx];
            channels = if channel_mode == 3 { 1 } else { 2 };
            found = true;

            let bitrate = bitrate_table[version][bitrate_idx] as u64 * 1000 / 8;
            if bitrate > 0 && file_size > 0 {
                let total_frames = file_size * 8 / (bitrate * 8);
                let sample_count = total_frames * samples_per_frame[version] as u64;
                let duration_secs = if sample_rate > 0 {
                    sample_count as f64 / sample_rate as f64
                } else {
                    0.0
                };
                return Ok(AudioMetadata {
                    format: AudioFormat::Mp3,
                    sample_rate,
                    channels,
                    sample_count,
                    duration_secs,
                });
            }
            break;
        }
        pos += 1;
    }

    if !found {
        file.seek(SeekFrom::Start(0)).map_err(AudioError::Io)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| AudioError::DecodeFailed(format!("Failed to probe MP3: {e}")))?;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| AudioError::DecodeFailed("No audio track found".into()))?;

        sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        channels = track
            .codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);
    }

    let duration_secs = if sample_rate > 0 && file_size > 0 {
        let estimated_bitrate = 128_000.0;
        file_size as f64 * 8.0 / estimated_bitrate
    } else {
        0.0
    };
    let sample_count = (duration_secs * sample_rate as f64).round() as u64;

    Ok(AudioMetadata {
        format: AudioFormat::Mp3,
        sample_rate,
        channels,
        sample_count,
        duration_secs,
    })
}

fn metadata_flac(path: &Path) -> Result<AudioMetadata, AudioError> {
    let reader = claxon::FlacReader::open(path)
        .map_err(|e| AudioError::DecodeFailed(format!("Failed to parse FLAC header: {e}")))?;
    let info = reader.streaminfo();

    let sample_rate = info.sample_rate;
    let channels = info.channels as u16;
    let sample_count = info.samples.map(|s| s * channels as u64).unwrap_or(0);
    let duration_secs = if sample_rate > 0 {
        info.samples
            .map(|s| s as f64 / sample_rate as f64)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(AudioMetadata {
        format: AudioFormat::Flac,
        sample_rate,
        channels,
        sample_count,
        duration_secs,
    })
}

pub fn audio_metadata(path: &Path) -> Result<AudioMetadata, AudioError> {
    let format = detect_format(path)?;
    match format {
        AudioFormat::Wav => metadata_wav(path),
        AudioFormat::Ogg => metadata_ogg(path),
        AudioFormat::Mp3 => metadata_mp3(path),
        AudioFormat::Flac => metadata_flac(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsupported_format() {
        let result = audio_metadata(Path::new("test.xyz"));
        assert!(result.is_err());
    }
}
