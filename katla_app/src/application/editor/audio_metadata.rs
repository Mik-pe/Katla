//! Probe audio details only for the entity consumed by the inspector.

use std::path::Path;

use katla_audio::{AudioError, AudioMetadata};
use katla_ecs::EntityId;

/// Read current metadata for the selected entity, retrying on every snapshot.
#[inline]
pub(super) fn selected_audio_metadata(
    entity: EntityId,
    selected: Option<EntityId>,
    path: &str,
    read_metadata: impl FnOnce(&Path) -> Result<AudioMetadata, AudioError>,
) -> Option<AudioMetadata> {
    if selected != Some(entity) {
        return None;
    }
    read_metadata(Path::new(path)).ok()
}

#[cfg(test)]
use crate::cpu_bench_support as bench_support;

#[cfg(test)]
mod tests {
    use super::*;
    use katla_audio::metadata::AudioFormat;
    use katla_ecs::World;

    fn metadata(sample_rate: u32) -> AudioMetadata {
        AudioMetadata {
            format: AudioFormat::Wav,
            sample_rate,
            channels: 2,
            sample_count: u64::from(sample_rate) * 2,
            duration_secs: 1.0,
        }
    }

    #[test]
    fn test_only_selected_audio_entity_invokes_metadata_reader() {
        let mut world = World::new();
        let entities: Vec<_> = (0..128).map(|_| world.create_entity()).collect();
        let mut reads = Vec::new();
        for selected in [None, Some(entities[17]), Some(entities[89]), None] {
            for &entity in &entities {
                let path = format!("audio/{}.wav", entity.id());
                let result = selected_audio_metadata(entity, selected, &path, |path| {
                    reads.push(path.to_owned());
                    Ok(metadata(48_000))
                });
                assert_eq!(result.is_some(), selected == Some(entity));
            }
        }
        assert_eq!(
            reads,
            [
                Path::new(&format!("audio/{}.wav", entities[17].id())),
                Path::new(&format!("audio/{}.wav", entities[89].id())),
            ]
        );
    }

    #[test]
    fn test_selected_metadata_refreshes_after_path_edits_and_errors() {
        let mut world = World::new();
        let entity = world.create_entity();
        let mut reads = Vec::new();

        let first = selected_audio_metadata(entity, Some(entity), "first.wav", |path| {
            reads.push(path.to_owned());
            Ok(metadata(44_100))
        });
        assert_eq!(first.unwrap().sample_rate, 44_100);

        let missing = selected_audio_metadata(entity, Some(entity), "edited.wav", |path| {
            reads.push(path.to_owned());
            Err(AudioError::DecodeFailed("temporarily missing".into()))
        });
        assert!(missing.is_none());

        let recovered = selected_audio_metadata(entity, Some(entity), "edited.wav", |path| {
            reads.push(path.to_owned());
            Ok(metadata(48_000))
        });
        assert_eq!(recovered.unwrap().sample_rate, 48_000);

        let changed = selected_audio_metadata(entity, Some(entity), "edited.wav", |path| {
            reads.push(path.to_owned());
            Ok(metadata(96_000))
        });
        assert_eq!(changed.unwrap().sample_rate, 96_000);
        assert_eq!(
            reads,
            ["first.wav", "edited.wav", "edited.wav", "edited.wav"].map(std::path::PathBuf::from)
        );
    }

    #[test]
    fn test_audio_metadata_selection_checks_entity_generation() {
        let mut world = World::new();
        let stale = world.create_entity();
        world.destroy_entity(stale);
        let replacement = world.create_entity();
        let result = selected_audio_metadata(replacement, Some(stale), "audio.wav", |_| {
            panic!("a stale selection must not probe a replacement entity")
        });
        assert!(result.is_none());
        assert!(
            selected_audio_metadata(replacement, Some(replacement), "audio.wav", |_| {
                Ok(metadata(48_000))
            })
            .is_some()
        );
    }

    struct WavFixtures {
        directory: std::path::PathBuf,
        paths: Vec<String>,
    }

    impl WavFixtures {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};

            static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "katla-editor-audio-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&directory).unwrap();
            let paths = [(44_100_u32, 1_u16), (48_000, 2)]
                .into_iter()
                .map(|(rate, channels)| {
                    let data_len = rate * u32::from(channels) * 2;
                    let mut bytes = Vec::with_capacity(44 + data_len as usize);
                    bytes.extend_from_slice(b"RIFF");
                    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
                    bytes.extend_from_slice(b"WAVEfmt ");
                    bytes.extend_from_slice(&16_u32.to_le_bytes());
                    bytes.extend_from_slice(&1_u16.to_le_bytes());
                    bytes.extend_from_slice(&channels.to_le_bytes());
                    bytes.extend_from_slice(&rate.to_le_bytes());
                    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
                    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
                    bytes.extend_from_slice(&16_u16.to_le_bytes());
                    bytes.extend_from_slice(b"data");
                    bytes.extend_from_slice(&data_len.to_le_bytes());
                    bytes.resize(44 + data_len as usize, 0);
                    let path = directory.join(format!("{rate}-{channels}.wav"));
                    std::fs::write(&path, bytes).unwrap();
                    path.to_str().unwrap().to_owned()
                })
                .collect();
            Self { directory, paths }
        }
    }

    impl Drop for WavFixtures {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn test_selected_audio_metadata_observes_file_replacement_and_recovery() {
        let fixture = WavFixtures::new();
        let mut world = World::new();
        let entity = world.create_entity();
        let read = || {
            selected_audio_metadata(
                entity,
                Some(entity),
                &fixture.paths[0],
                katla_audio::audio_metadata,
            )
        };
        assert_eq!(read().unwrap().sample_rate, 44_100);
        std::fs::remove_file(&fixture.paths[0]).unwrap();
        assert!(read().is_none());
        std::fs::copy(&fixture.paths[1], &fixture.paths[0]).unwrap();
        let replaced = read().unwrap();
        assert_eq!(replaced.sample_rate, 48_000);
        assert_eq!(replaced.channels, 2);
        assert_eq!(replaced.duration_secs, 1.0);
    }

    /// Set KATLA_EDITOR_AUDIO_REFERENCE=1 to run the original unconditional
    /// probe adapter. Copying this test into main does not change its collector.
    #[test]
    #[ignore = "CPU-only editor audio metadata benchmark"]
    fn test_benchmark_editor_audio_metadata() {
        use std::hint::black_box;

        let reference = std::env::var("KATLA_EDITOR_AUDIO_REFERENCE").as_deref() == Ok("1");
        let fixture = WavFixtures::new();
        println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
        for count in [1, 128, 1024] {
            let mut world = World::new();
            let entities: Vec<_> = (0..count).map(|_| world.create_entity()).collect();
            for (name, selected) in [
                ("editor_audio_metadata_none", None),
                ("editor_audio_metadata_selected", Some(entities[count / 2])),
            ] {
                bench_support::measure(name, count, || {
                    let mut calls = 0;
                    let mut checksum = 0.0;
                    for (index, &entity) in entities.iter().enumerate() {
                        let path = black_box(&fixture.paths[index % fixture.paths.len()]);
                        let mut read_metadata = |path: &Path| {
                            calls += 1;
                            katla_audio::audio_metadata(path)
                        };
                        let result = if reference {
                            read_metadata(Path::new(path)).ok()
                        } else {
                            selected_audio_metadata(entity, selected, path, read_metadata)
                        };
                        if selected == Some(entity) {
                            let metadata = result.expect("generated WAV metadata must be readable");
                            checksum += f64::from(metadata.sample_rate)
                                + f64::from(metadata.channels)
                                + metadata.duration_secs;
                        }
                    }
                    let expected_calls = if reference {
                        count
                    } else {
                        usize::from(selected.is_some())
                    };
                    assert_eq!(calls, expected_calls);
                    black_box(calls);
                    checksum
                });
            }
        }
    }
}
