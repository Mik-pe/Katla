//! Persistent Metal pipeline archive with keyed invalidation.
//!
//! Owns one `MTLBinaryArchive` plus a JSON metadata sidecar at an application
//! cache location. The sidecar records the archive schema version, OS, GPU
//! registry identity, Metal feature family, and engine version; a mismatch
//! (or a corrupt archive) forces a clean rebuild instead of reusing compiled
//! code that no longer matches the engine.
//!
//! Render pipeline descriptors consult the archive at creation time via
//! `setBinaryArchives`, and every successfully created pipeline is registered
//! back into the archive. `flush` serializes atomically (temp file + rename).

use std::fs;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSArray, NSURL};
use objc2_metal::{
    MTLBinaryArchive, MTLBinaryArchiveDescriptor, MTLDevice, MTLRenderPipelineDescriptor,
};

use crate::error::RendererError;

const SCHEMA_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct ArchiveMetadata {
    schema_version: u32,
    os_version: String,
    gpu_registry_id: u64,
    apple_silicon: bool,
    engine_version: String,
}

impl ArchiveMetadata {
    fn current(device: &ProtocolObject<dyn MTLDevice>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            os_version: os_version_string(),
            gpu_registry_id: device.registryID(),
            apple_silicon: device.supportsFamily(objc2_metal::MTLGPUFamily::Apple7),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Why a cached pipeline archive could not be reused as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveRejection {
    /// No cached artifacts existed.
    Absent,
    /// Sidecar metadata described a different machine or engine build.
    MetadataMismatch,
    /// The archive file existed but Metal rejected it.
    Corrupt,
}

/// Structured snapshot of pipeline-cache behaviour for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PipelineCacheStats {
    /// True when the archive opened from disk with matching metadata.
    pub opened_from_disk: bool,
    /// Set when a fresh archive was created instead of loading the cache.
    pub rejection: Option<ArchiveRejection>,
    /// Pipelines registered into the archive this session.
    pub pipelines_registered: usize,
    /// Time spent opening or creating the archive.
    pub open_duration: std::time::Duration,
    /// Time spent on the most recent successful serialization.
    pub last_flush_duration: Option<std::time::Duration>,
}

pub(crate) struct MetalPipelineArchive {
    archive: Retained<ProtocolObject<dyn MTLBinaryArchive>>,
    metadata: ArchiveMetadata,
    path: PathBuf,
    sidecar_path: PathBuf,
    pub(crate) registered_pipelines: std::cell::Cell<usize>,
    rejection: Option<ArchiveRejection>,
    open_duration: std::time::Duration,
    last_flush_duration: std::cell::Cell<Option<std::time::Duration>>,
}

impl MetalPipelineArchive {
    /// Open the cached archive when its metadata matches this machine, or
    /// create a fresh empty archive. Corrupt, partial, or stale artifacts are
    /// deleted and rebuilt.
    pub(crate) fn open_or_create(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Self, RendererError> {
        Self::open_or_create_in(device, &cache_directory()?)
    }

    pub(crate) fn open_or_create_in(
        device: &ProtocolObject<dyn MTLDevice>,
        dir: &std::path::Path,
    ) -> Result<Self, RendererError> {
        fs::create_dir_all(dir).map_err(|e| {
            RendererError::InitializationFailed(format!("Pipeline cache dir unavailable: {e}"))
        })?;
        let path = dir.to_path_buf().join("katla-pipelines.mtlbin");
        let sidecar_path = dir.to_path_buf().join("katla-pipelines.meta.json");
        let metadata = ArchiveMetadata::current(device);

        let cached_metadata_matches = read_metadata(&sidecar_path)
            .map(|cached| cached == metadata)
            .unwrap_or(false);

        let rejection = if !path.exists() && !sidecar_path.exists() {
            Some(ArchiveRejection::Absent)
        } else if !cached_metadata_matches {
            Some(ArchiveRejection::MetadataMismatch)
        } else {
            None
        };

        let descriptor = MTLBinaryArchiveDescriptor::new();
        let opening_existing = cached_metadata_matches && path.exists();
        if opening_existing {
            let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
                path.to_string_lossy().as_ref(),
            ));
            descriptor.setUrl(Some(&url));
        } else {
            // Stale or missing cache: drop both artifacts and start empty.
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(&sidecar_path);
        }

        let opened_at = std::time::Instant::now();
        let archive = device.newBinaryArchiveWithDescriptor_error(&descriptor);
        match archive {
            Ok(archive) => {
                if opening_existing {
                    log::info!("Pipeline cache opened from {}", path.display());
                } else {
                    log::info!("Pipeline cache rebuilt empty (stale, missing, or partial)");
                }
                Ok(Self {
                    archive,
                    metadata,
                    path,
                    sidecar_path,
                    registered_pipelines: std::cell::Cell::new(0),
                    rejection: rejection.filter(|_| !opening_existing),
                    open_duration: opened_at.elapsed(),
                    last_flush_duration: std::cell::Cell::new(None),
                })
            }
            Err(err) => {
                // An unreadable archive must never brick startup: clear the
                // artifacts and fall back to an empty archive.
                let _ = fs::remove_file(&path);
                let _ = fs::remove_file(&sidecar_path);
                let empty = MTLBinaryArchiveDescriptor::new();
                let archive = device
                    .newBinaryArchiveWithDescriptor_error(&empty)
                    .map_err(|e| {
                        RendererError::InitializationFailed(format!(
                            "Failed to create pipeline archive: {}",
                            e.localizedDescription()
                        ))
                    })?;
                log::warn!(
                    "Pipeline cache rejected ({}); rebuilt empty",
                    err.localizedDescription()
                );
                Ok(Self {
                    archive,
                    metadata,
                    path,
                    sidecar_path,
                    registered_pipelines: std::cell::Cell::new(0),
                    rejection: Some(ArchiveRejection::Corrupt),
                    open_duration: opened_at.elapsed(),
                    last_flush_duration: std::cell::Cell::new(None),
                })
            }
        }
    }

    /// Structured snapshot of this cache session for diagnostics.
    pub(crate) fn stats(&self) -> PipelineCacheStats {
        PipelineCacheStats {
            opened_from_disk: self.rejection.is_none(),
            rejection: self.rejection,
            pipelines_registered: self.registered_pipelines.get(),
            open_duration: self.open_duration,
            last_flush_duration: self.last_flush_duration.get(),
        }
    }

    /// Consult the archive when compiling a render pipeline and register the
    /// descriptor so a future run can reuse the compiled functions.
    pub(crate) fn attach_to_render_descriptor(&self, descriptor: &MTLRenderPipelineDescriptor) {
        let archive_ref: &ProtocolObject<dyn MTLBinaryArchive> = self.archive.as_ref();
        let archives: Retained<NSArray<ProtocolObject<dyn MTLBinaryArchive>>> =
            NSArray::from_slice(std::slice::from_ref(&archive_ref));
        descriptor.setBinaryArchives(Some(&archives));
    }

    /// Register a successfully created render pipeline into the archive.
    /// Duplicate registrations are a no-op per Metal; other failures degrade
    /// to a warning because a cold archive only costs compile time, never
    /// correctness.
    pub(crate) fn register_render_pipeline(&self, descriptor: &MTLRenderPipelineDescriptor) {
        self.registered_pipelines
            .set(self.registered_pipelines.get() + 1);
        if let Err(err) = self
            .archive
            .addRenderPipelineFunctionsWithDescriptor_error(descriptor)
        {
            log::debug!(
                "Archive registration skipped: {}",
                err.localizedDescription()
            );
        }
    }

    /// Register a successfully created compute pipeline into the archive.
    pub(crate) fn register_compute_pipeline(
        &self,
        function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
    ) {
        self.registered_pipelines
            .set(self.registered_pipelines.get() + 1);
        let descriptor = objc2_metal::MTLComputePipelineDescriptor::new();
        descriptor.setComputeFunction(Some(function));
        if let Err(err) = self
            .archive
            .addComputePipelineFunctionsWithDescriptor_error(&descriptor)
        {
            log::debug!(
                "Archive registration skipped: {}",
                err.localizedDescription()
            );
        }
    }

    /// Serialize the archive atomically: write to a temp file, then rename
    /// over the target. Interrupted writes leave the previous archive intact.
    /// An archive with no registered pipelines has nothing to persist and is
    /// skipped (Metal rejects serializing empty archives).
    pub(crate) fn flush(&self) {
        if self.registered_pipelines.get() == 0 {
            return;
        }
        let tmp = self.path.with_extension("mtlbin.tmp");
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
            tmp.to_string_lossy().as_ref(),
        ));
        let flush_started = std::time::Instant::now();
        if let Err(err) = self.archive.serializeToURL_error(&url) {
            log::warn!(
                "Pipeline archive flush failed: {}",
                err.localizedDescription()
            );
            let _ = fs::remove_file(&tmp);
            return;
        }
        if fs::rename(&tmp, &self.path).is_ok() {
            self.last_flush_duration.set(Some(flush_started.elapsed()));
            let _ = serde_json::to_writer(
                fs::File::create(&self.sidecar_path).expect("sidecar create"),
                &self.metadata,
            );
        }
    }
}

fn read_metadata(sidecar: &PathBuf) -> Option<ArchiveMetadata> {
    let file = fs::File::open(sidecar).ok()?;
    serde_json::from_reader(file).ok()
}

fn cache_directory() -> Result<PathBuf, RendererError> {
    if let Some(dir) = std::env::var_os("KATLA_PIPELINE_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| RendererError::InitializationFailed("No user cache directory".into()))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Caches")
        .join("dev.ravboet.katla")
        .join("pipelines"))
}

fn os_version_string() -> String {
    use objc2_foundation::NSProcessInfo;
    let info = NSProcessInfo::processInfo();
    info.operatingSystemVersionString().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_cache_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "katla-pipeline-archive-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn metadata_for_current_device() -> ArchiveMetadata {
        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        ArchiveMetadata::current(&ctx.device)
    }

    fn trivial_compute_function(
        ctx: &crate::metal::context::MetalContext,
    ) -> Retained<ProtocolObject<dyn objc2_metal::MTLFunction>> {
        const WGSL: &str = r#"
            @group(0) @binding(0) var<storage, read_write> out: array<u32>;
            @compute @workgroup_size(1)
            fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
                out[gid.x] = gid.x;
            }
        "#;
        let compiled = crate::metal::shader::compile_wgsl_to_metal(
            &ctx.device,
            WGSL,
            &["cs_main"],
            crate::metal::shader::ShaderProfile::Graphics,
        )
        .unwrap();
        compiled.module.entry_points.get("cs_main").unwrap().clone()
    }

    #[test]
    fn test_pipeline_archive_flush_writes_artifacts() {
        let dir = temp_cache_dir("flush");

        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        let archive = MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).unwrap();
        assert!(
            !archive.stats().opened_from_disk,
            "empty cache dir is a fresh build"
        );
        let function = trivial_compute_function(&ctx);
        archive.register_compute_pipeline(&function);
        archive.flush();

        assert!(dir.join("katla-pipelines.mtlbin").exists());
        assert!(dir.join("katla-pipelines.meta.json").exists());
        let bytes = fs::read(dir.join("katla-pipelines.mtlbin")).unwrap();
        assert!(!bytes.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pipeline_archive_rejects_corrupt_file() {
        let dir = temp_cache_dir("corrupt");
        fs::write(dir.join("katla-pipelines.mtlbin"), b"garbage").unwrap();

        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        // Write a matching sidecar so the cache believes it is valid; the corrupt
        // archive itself must still be rejected and rebuilt.
        let meta = serde_json::to_string(&ArchiveMetadata::current(&ctx.device)).unwrap();
        fs::write(dir.join("katla-pipelines.meta.json"), meta).unwrap();

        let archive = MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).unwrap();
        assert!(
            !archive.stats().opened_from_disk,
            "corrupt archive bytes must not load"
        );
        let function = trivial_compute_function(&ctx);
        archive.register_compute_pipeline(&function);
        archive.flush();

        let flushed = fs::read(dir.join("katla-pipelines.mtlbin")).unwrap();
        assert_ne!(flushed, b"garbage", "corrupt archive was not rebuilt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pipeline_archive_rebuilds_on_metadata_mismatch() {
        let dir = temp_cache_dir("mismatch");

        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        let function = trivial_compute_function(&ctx);

        // First population: nothing on disk yet, so this is a fresh build.
        let archive = MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).unwrap();
        assert!(!archive.stats().opened_from_disk);
        archive.register_compute_pipeline(&function);
        archive.flush();

        // Stale sidecar: schema older than the current one.
        let mut stale: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("katla-pipelines.meta.json")).unwrap(),
        )
        .unwrap();
        stale["schema_version"] = serde_json::json!(SCHEMA_VERSION - 1);
        fs::write(
            dir.join("katla-pipelines.meta.json"),
            serde_json::to_string(&stale).unwrap(),
        )
        .unwrap();

        // The mismatch must invalidate the cached archive and rebuild empty.
        let reopened = MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).unwrap();
        assert!(
            !reopened.stats().opened_from_disk,
            "stale metadata must invalidate the archive"
        );
        assert_eq!(reopened.registered_pipelines.get(), 0);

        // Register + flush to repopulate the cache with a current sidecar;
        // the next open must then read from disk.
        reopened.register_compute_pipeline(&function);
        reopened.flush();
        let again = MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).unwrap();
        assert!(again.stats().opened_from_disk);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pipeline_archive_metadata_tracks_device() {
        let meta = metadata_for_current_device();
        assert_eq!(meta.schema_version, SCHEMA_VERSION);
        assert!(meta.gpu_registry_id > 0, "registry id must be captured");
        assert!(!meta.os_version.is_empty());
    }

    #[test]
    fn test_pipeline_cache_stats_classifies_absent_cache() {
        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        let dir = temp_cache_dir("stats-absent");
        let archive =
            MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).expect("archive opens");
        let stats = archive.stats();
        assert!(!stats.opened_from_disk);
        assert_eq!(stats.rejection, Some(ArchiveRejection::Absent));
        assert_eq!(stats.pipelines_registered, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pipeline_cache_stats_survive_reopen_cycle() {
        let ctx = crate::metal::context::MetalContext::init_headless().unwrap();
        let dir = temp_cache_dir("stats-reopen");
        let first =
            MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).expect("first opens");
        let rejection = first.stats().rejection;
        first.flush();
        drop(first);

        let second =
            MetalPipelineArchive::open_or_create_in(&ctx.device, &dir).expect("second opens");
        let stats = second.stats();
        // The rebuild is recorded; a reload (same session artifacts, matching
        // metadata) reports no rejection.
        if rejection == Some(ArchiveRejection::Absent) {
            // flush of an empty archive writes nothing; second open is Absent too.
            assert_eq!(stats.rejection, Some(ArchiveRejection::Absent));
        } else {
            assert!(stats.opened_from_disk);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
