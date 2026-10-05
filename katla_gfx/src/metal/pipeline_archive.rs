//! Metal 4 asynchronous compiler and persistent, content-addressed archives.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSString, NSURL};
use objc2_metal::*;
use sha2::{Digest, Sha256};

use crate::error::RendererError;

const SCHEMA_VERSION: u32 = 2;
const SHADER_ABI_VERSION: u32 = super::binding_schema::BINDING_ABI_VERSION;
const COMPILER_VERSION: &str = "objc2-metal-0.3.2;naga-29;metal4;msl3.0;default-options";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct ArchiveMetadata {
    schema_version: u32,
    os_version: String,
    gpu_registry_id: u64,
    families: Vec<isize>,
    compiler_version: String,
    shader_abi_version: u32,
}

impl ArchiveMetadata {
    fn current(device: &ProtocolObject<dyn MTLDevice>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            os_version: objc2_foundation::NSProcessInfo::processInfo()
                .operatingSystemVersionString()
                .to_string(),
            gpu_registry_id: device.registryID(),
            families: [
                MTLGPUFamily::Apple7,
                MTLGPUFamily::Apple8,
                MTLGPUFamily::Apple9,
                MTLGPUFamily::Apple10,
                MTLGPUFamily::Mac2,
                MTLGPUFamily::Metal3,
                MTLGPUFamily::Metal4,
            ]
            .into_iter()
            .filter(|family| device.supportsFamily(*family))
            .map(|family| family.0)
            .collect(),
            compiler_version: format!(
                "{COMPILER_VERSION};lock={}",
                hash_bytes(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../Cargo.lock"
                )))
            ),
            shader_abi_version: SHADER_ABI_VERSION,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    metadata: ArchiveMetadata,
    archives: BTreeMap<String, String>,
}

/// Reason the persisted artifacts were not reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveRejection {
    Absent,
    MetadataMismatch,
    Corrupt,
}

/// Observed native archive hits and asynchronous compiler work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PipelineCacheStats {
    pub opened_from_disk: bool,
    pub rejection: Option<ArchiveRejection>,
    pub pipelines_registered: usize,
    pub hits: usize,
    pub misses: usize,
    pub open_duration: Duration,
    pub last_flush_duration: Option<Duration>,
    pub last_compile_duration: Option<Duration>,
    pub last_save_error: Option<String>,
    pub last_lookup_error: Option<String>,
}

/// Immutable, fully resolved Metal libraries and pipeline states returned
/// from a compiler worker to the registration or reload owner.
pub(crate) struct CompletedMetalObject(pub Retained<AnyObject>);
// SAFETY: The contained objects are immutable MTLLibrary or pipeline states,
// which Metal permits using concurrently; retaining only extends ownership.
unsafe impl Send for CompletedMetalObject {}

pub(crate) enum CompilerInput {
    Library(Retained<MTL4LibraryDescriptor>),
    Render(Retained<MTL4RenderPipelineDescriptor>),
    Compute(Retained<MTL4ComputePipelineDescriptor>),
}

struct CompilerJob {
    compiler: Retained<ProtocolObject<dyn MTL4Compiler>>,
    input: CompilerInput,
}
// SAFETY: Each job transfers exclusive ownership of configured descriptors
// to one worker. Metal compiler contexts and referenced libraries are thread-safe.
unsafe impl Send for CompilerJob {}

impl CompilerJob {
    fn run(self) -> Result<CompletedMetalObject, String> {
        let result = match self.input {
            CompilerInput::Library(descriptor) => self
                .compiler
                .newLibraryWithDescriptor_error(&descriptor)
                .map(|state| unsafe { Retained::cast_unchecked(state) }),
            CompilerInput::Render(descriptor) => self
                .compiler
                .newRenderPipelineStateWithDescriptor_compilerTaskOptions_error(&descriptor, None)
                .map(|state| unsafe { Retained::cast_unchecked(state) }),
            CompilerInput::Compute(descriptor) => self
                .compiler
                .newComputePipelineStateWithDescriptor_compilerTaskOptions_error(&descriptor, None)
                .map(|state| unsafe { Retained::cast_unchecked(state) }),
        };
        result
            .map(CompletedMetalObject)
            .map_err(|error| error.localizedDescription().to_string())
    }
}

pub(crate) fn compile_on_worker(
    compiler: Retained<ProtocolObject<dyn MTL4Compiler>>,
    input: CompilerInput,
) -> Result<CompletedMetalObject, RendererError> {
    let job = CompilerJob { compiler, input };
    std::thread::Builder::new()
        .name("katla-metal-compiler".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || objc2::rc::autoreleasepool(|_| job.run()))
        .map_err(|error| RendererError::InitializationFailed(error.to_string()))?
        .join()
        .map_err(|_| {
            RendererError::ResourceCreationFailed("Metal compiler worker panicked".into())
        })?
        .map_err(RendererError::ResourceCreationFailed)
}

pub(crate) fn new_compiler(
    device: &ProtocolObject<dyn MTLDevice>,
    serializer: Option<&ProtocolObject<dyn MTL4PipelineDataSetSerializer>>,
) -> Result<Retained<ProtocolObject<dyn MTL4Compiler>>, RendererError> {
    let descriptor = MTL4CompilerDescriptor::new();
    descriptor.setPipelineDataSetSerializer(serializer);
    device
        .newCompilerWithDescriptor_error(&descriptor)
        .map_err(|e| RendererError::InitializationFailed(e.localizedDescription().to_string()))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    const HEX: [u8; 16] = *b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [HEX[(byte >> 4) as usize], HEX[(byte & 0x0f) as usize]])
        .map(char::from)
        .collect()
}

struct Compilation {
    compiler: Retained<ProtocolObject<dyn MTL4Compiler>>,
    serializer: Retained<ProtocolObject<dyn MTL4PipelineDataSetSerializer>>,
}

pub(crate) struct MetalPipelineArchive {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    path: PathBuf,
    manifest: Mutex<Manifest>,
    stats: Mutex<PipelineCacheStats>,
}
// SAFETY: MTLDevice is thread-safe. Mutable manifest/stats are mutex-protected;
// compiler and serializer objects are owned by individual compilation jobs.
unsafe impl Send for MetalPipelineArchive {}
unsafe impl Sync for MetalPipelineArchive {}

impl MetalPipelineArchive {
    pub(crate) fn open_or_create(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Self, RendererError> {
        Self::open_or_create_in(device, &cache_directory()?)
    }

    pub(crate) fn open_or_create_in(
        device: &ProtocolObject<dyn MTLDevice>,
        dir: &Path,
    ) -> Result<Self, RendererError> {
        let start = Instant::now();
        fs::create_dir_all(dir).map_err(|e| RendererError::InitializationFailed(e.to_string()))?;
        let metadata = ArchiveMetadata::current(device);
        let (manifest, rejection) = read_manifest(&dir.join("manifest.json"), &metadata);
        log::info!(
            "pipeline_cache event=open rejection={rejection:?} elapsed_us={}",
            start.elapsed().as_micros()
        );
        Ok(Self {
            device: device.retain(),
            path: dir.to_owned(),
            manifest: Mutex::new(manifest),
            stats: Mutex::new(PipelineCacheStats {
                opened_from_disk: rejection.is_none(),
                rejection,
                pipelines_registered: 0,
                hits: 0,
                misses: 0,
                open_duration: start.elapsed(),
                last_flush_duration: None,
                last_compile_duration: None,
                last_save_error: None,
                last_lookup_error: None,
            }),
        })
    }

    pub(crate) fn stats(&self) -> PipelineCacheStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn lookup(&self, key: &str) -> Option<Retained<ProtocolObject<dyn MTL4Archive>>> {
        let expected = self.manifest.lock().ok()?.archives.get(key)?.clone();
        let path = self.path.join(format!("{key}.mtl4"));
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!(
                    "pipeline_cache event=invalidate key={key} reason=archive_read error={error}"
                );
                self.manifest.lock().ok()?.archives.remove(key);
                return None;
            }
        };
        if hash_bytes(&bytes) != expected || !valid_archive_container(&bytes) {
            log::warn!("pipeline_cache event=invalidate key={key} reason=archive_integrity");
            self.manifest.lock().ok()?.archives.remove(key);
            return None;
        }
        let started = Instant::now();
        match self.device.newArchiveWithURL_error(&file_url(&path)) {
            Ok(archive) => {
                log::debug!(
                    "pipeline_cache event=load key={key} elapsed_us={}",
                    started.elapsed().as_micros()
                );
                Some(archive)
            }
            Err(error) => {
                log::warn!(
                    "pipeline_cache event=invalidate key={key} reason={}",
                    error.localizedDescription()
                );
                self.manifest.lock().ok()?.archives.remove(key);
                None
            }
        }
    }

    fn compiler(&self) -> Result<Compilation, RendererError> {
        let descriptor = MTL4PipelineDataSetSerializerDescriptor::new();
        descriptor.setConfiguration(MTL4PipelineDataSetSerializerConfiguration::CaptureBinaries);
        let serializer = self
            .device
            .newPipelineDataSetSerializerWithDescriptor(&descriptor);
        let compiler = new_compiler(&self.device, Some(&serializer))?;
        Ok(Compilation {
            compiler,
            serializer,
        })
    }

    fn persist(
        &self,
        key: &str,
        serializer: &ProtocolObject<dyn MTL4PipelineDataSetSerializer>,
        elapsed: Duration,
    ) {
        let start = Instant::now();
        let operation = || -> Result<(), String> {
            static SAVE_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
            let _save_lock = SAVE_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .map_err(|e| e.to_string())?;
            let mut manifest = self.manifest.lock().map_err(|e| e.to_string())?;
            let (existing, _) = read_manifest(&self.path.join("manifest.json"), &manifest.metadata);
            manifest.archives.extend(existing.archives);
            let path = self.path.join(format!("{key}.mtl4"));
            let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
            serializer
                .serializeAsArchiveAndFlushToURL_error(&file_url(&temporary))
                .map_err(|e| e.localizedDescription().to_string())?;
            let bytes = fs::read(&temporary).map_err(|e| e.to_string())?;
            fs::File::open(&temporary)
                .and_then(|file| file.sync_all())
                .map_err(|e| e.to_string())?;
            fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
            manifest.archives.insert(key.into(), hash_bytes(&bytes));
            atomic_write(
                &self.path.join("manifest.json"),
                &serde_json::to_vec(&*manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        };
        match operation() {
            Ok(()) => {
                let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
                stats.pipelines_registered += 1;
                stats.last_flush_duration = Some(start.elapsed());
                stats.last_compile_duration = Some(elapsed);
                log::info!(
                    "pipeline_cache event=compile key={key} elapsed_us={} save_us={}",
                    elapsed.as_micros(),
                    start.elapsed().as_micros()
                );
            }
            Err(error) => {
                self.stats
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .last_save_error = Some(error.clone());
                log::warn!("pipeline_cache event=save_failed key={key} reason={error}");
            }
        }
    }

    pub(crate) fn create_render_pipeline(
        &self,
        descriptor: &MTLRenderPipelineDescriptor,
        state_identity: &str,
    ) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>, RendererError> {
        let (native, key) = render_descriptor(descriptor, state_identity)?;
        if let Some(archive) = self.lookup(&key) {
            let found = archive.newRenderPipelineStateWithDescriptor_error(&native);
            if let Err(error) = &found {
                log::warn!(
                    "pipeline_cache event=lookup_failed key={key} reason={}",
                    error.localizedDescription()
                );
                self.stats
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .last_lookup_error = Some(error.localizedDescription().to_string());
            }
            if let Ok(state) = found {
                self.stats.lock().unwrap_or_else(|e| e.into_inner()).hits += 1;
                log::debug!("pipeline_cache event=hit key={key}");
                return Ok(state);
            }
        }
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).misses += 1;
        log::debug!("pipeline_cache event=miss key={key}");
        let Compilation {
            compiler,
            serializer,
        } = self.compiler()?;
        let started = Instant::now();
        let state = compile_on_worker(compiler.clone(), CompilerInput::Render(native.clone()))?;
        self.persist(&key, &serializer, started.elapsed());
        Ok(unsafe { Retained::cast_unchecked(state.0) })
    }

    pub(crate) fn create_compute_pipeline(
        &self,
        function: &ProtocolObject<dyn MTLFunction>,
        workgroup: [u32; 3],
    ) -> Result<Retained<ProtocolObject<dyn MTLComputePipelineState>>, RendererError> {
        let (function_descriptor, function_key) = super::shader::function_descriptor(function)?;
        let key = hash_bytes(format!("compute;{function_key};workgroup={workgroup:?};multiple=false;threads=0;link=false;indirect=false;constants=[];{COMPILER_VERSION};abi={SHADER_ABI_VERSION}").as_bytes());
        let native = MTL4ComputePipelineDescriptor::new();
        native.setComputeFunctionDescriptor(Some(&function_descriptor));
        if let Some(archive) = self.lookup(&key) {
            match archive.newComputePipelineStateWithDescriptor_error(&native) {
                Ok(state) => {
                    self.stats.lock().unwrap_or_else(|e| e.into_inner()).hits += 1;
                    log::debug!("pipeline_cache event=hit key={key}");
                    return Ok(state);
                }
                Err(error) => {
                    let message = error.localizedDescription().to_string();
                    self.stats
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .last_lookup_error = Some(message.clone());
                    log::warn!("pipeline_cache event=lookup_failed key={key} reason={message}");
                }
            }
        }
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).misses += 1;
        log::debug!("pipeline_cache event=miss key={key}");
        let Compilation {
            compiler,
            serializer,
        } = self.compiler()?;
        let started = Instant::now();
        let state = compile_on_worker(compiler.clone(), CompilerInput::Compute(native.clone()))?;
        self.persist(&key, &serializer, started.elapsed());
        Ok(unsafe { Retained::cast_unchecked(state.0) })
    }
}

fn valid_archive_container(bytes: &[u8]) -> bool {
    // Metal archives use a fat container. Reject incomplete headers and slices
    // before handing them to the native loader, which assumes valid bounds.
    if bytes.get(..4) != Some(&[0xcb, 0xfe, 0xba, 0xbe]) {
        return false;
    }
    let read_u32 = |offset| {
        bytes
            .get(offset..offset + 4)
            .map(|value| u32::from_be_bytes([value[0], value[1], value[2], value[3]]) as usize)
    };
    let Some(count) = read_u32(4) else {
        return false;
    };
    let Some(header_end) = count.checked_mul(20).and_then(|n| n.checked_add(8)) else {
        return false;
    };
    if count == 0 || header_end > bytes.len() {
        return false;
    }
    for index in 0..count {
        let record = 8 + index * 20;
        let (Some(offset), Some(length)) = (read_u32(record + 8), read_u32(record + 12)) else {
            return false;
        };
        let Some(end) = offset.checked_add(length) else {
            return false;
        };
        if offset < header_end || length < 4 || end > bytes.len() {
            return false;
        }
    }
    true
}

fn render_descriptor(
    descriptor: &MTLRenderPipelineDescriptor,
    state_identity: &str,
) -> Result<(Retained<MTL4RenderPipelineDescriptor>, String), RendererError> {
    let vertex = descriptor
        .vertexFunction()
        .ok_or_else(|| RendererError::InvalidOperation("Missing vertex function".into()))?;
    let (vertex_descriptor, vertex_key) = super::shader::function_descriptor(&vertex)?;
    let fragment = descriptor
        .fragmentFunction()
        .map(|function| super::shader::function_descriptor(&function))
        .transpose()?;
    let native = MTL4RenderPipelineDescriptor::new();
    native.setVertexFunctionDescriptor(Some(&vertex_descriptor));
    native.setFragmentFunctionDescriptor(
        fragment
            .as_ref()
            .map(|(function, _)| function.as_ref() as &MTL4FunctionDescriptor),
    );
    native.setVertexDescriptor(descriptor.vertexDescriptor().as_deref());
    unsafe {
        native.setRasterSampleCount(descriptor.rasterSampleCount());
    }
    native.setRasterizationEnabled(descriptor.isRasterizationEnabled());
    native.setAlphaToCoverageState(if descriptor.isAlphaToCoverageEnabled() {
        MTL4AlphaToCoverageState::Enabled
    } else {
        MTL4AlphaToCoverageState::Disabled
    });
    native.setAlphaToOneState(if descriptor.isAlphaToOneEnabled() {
        MTL4AlphaToOneState::Enabled
    } else {
        MTL4AlphaToOneState::Disabled
    });
    native.setInputPrimitiveTopology(descriptor.inputPrimitiveTopology());
    unsafe {
        native.setMaxVertexAmplificationCount(descriptor.maxVertexAmplificationCount());
    }
    native.setSupportIndirectCommandBuffers(if descriptor.supportIndirectCommandBuffers() {
        MTL4IndirectCommandBufferSupportState::Enabled
    } else {
        MTL4IndirectCommandBufferSupportState::Disabled
    });
    let mut identity = format!(
        "render;vs={vertex_key};fs={:?};state={state_identity};sample={};raster={};alpha_coverage={};alpha_one={};topology={:?};depth={:?};stencil={:?};constants=[];{COMPILER_VERSION};abi={SHADER_ABI_VERSION}",
        fragment.as_ref().map(|(_, key)| key),
        descriptor.rasterSampleCount(),
        descriptor.isRasterizationEnabled(),
        descriptor.isAlphaToCoverageEnabled(),
        descriptor.isAlphaToOneEnabled(),
        descriptor.inputPrimitiveTopology(),
        descriptor.depthAttachmentPixelFormat(),
        descriptor.stencilAttachmentPixelFormat()
    );
    identity.push_str(&format!(
        ";amplification={};indirect={}",
        descriptor.maxVertexAmplificationCount(),
        descriptor.supportIndirectCommandBuffers()
    ));
    for index in 0..8 {
        let source = unsafe {
            descriptor
                .colorAttachments()
                .objectAtIndexedSubscript(index)
        };
        let target = unsafe { native.colorAttachments().objectAtIndexedSubscript(index) };
        target.setPixelFormat(source.pixelFormat());
        target.setBlendingState(if source.isBlendingEnabled() {
            MTL4BlendState::Enabled
        } else {
            MTL4BlendState::Disabled
        });
        target.setSourceRGBBlendFactor(source.sourceRGBBlendFactor());
        target.setDestinationRGBBlendFactor(source.destinationRGBBlendFactor());
        target.setRgbBlendOperation(source.rgbBlendOperation());
        target.setSourceAlphaBlendFactor(source.sourceAlphaBlendFactor());
        target.setDestinationAlphaBlendFactor(source.destinationAlphaBlendFactor());
        target.setAlphaBlendOperation(source.alphaBlendOperation());
        target.setWriteMask(source.writeMask());
        identity.push_str(&format!(
            ";color={:?},{},{:?},{:?},{:?},{:?},{:?},{:?},{:?}",
            source.pixelFormat(),
            source.isBlendingEnabled(),
            source.sourceRGBBlendFactor(),
            source.destinationRGBBlendFactor(),
            source.rgbBlendOperation(),
            source.sourceAlphaBlendFactor(),
            source.destinationAlphaBlendFactor(),
            source.alphaBlendOperation(),
            source.writeMask()
        ));
    }
    if let Some(vertex) = descriptor.vertexDescriptor() {
        for index in 0..31 {
            let attribute = unsafe { vertex.attributes().objectAtIndexedSubscript(index) };
            let layout = unsafe { vertex.layouts().objectAtIndexedSubscript(index) };
            identity.push_str(&format!(
                ";attribute={:?},{},{};layout={},{:?},{}",
                attribute.format(),
                attribute.offset(),
                attribute.bufferIndex(),
                layout.stride(),
                layout.stepFunction(),
                layout.stepRate()
            ));
        }
    }
    let key = hash_bytes(identity.as_bytes());
    native.setLabel(Some(&NSString::from_str(&key)));
    Ok((native, key))
}

fn read_manifest(path: &Path, expected: &ArchiveMetadata) -> (Manifest, Option<ArchiveRejection>) {
    let read = fs::read(path);
    let rejection = match read {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Some(ArchiveRejection::Absent)
        }
        Err(_) => Some(ArchiveRejection::Corrupt),
        Ok(bytes) => match serde_json::from_slice::<Manifest>(&bytes) {
            Ok(manifest) if manifest.metadata == *expected => return (manifest, None),
            Ok(_) => Some(ArchiveRejection::MetadataMismatch),
            Err(_) => Some(ArchiveRejection::Corrupt),
        },
    };
    (
        Manifest {
            metadata: expected.clone(),
            archives: BTreeMap::new(),
        },
        rejection,
    )
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn cache_directory() -> Result<PathBuf, RendererError> {
    if let Some(dir) = std::env::var_os("KATLA_PIPELINE_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| RendererError::InitializationFailed("No user cache directory".into()))?;
    Ok(PathBuf::from(home).join("Library/Caches/dev.ravboet.katla/pipelines-metal4"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> ArchiveMetadata {
        ArchiveMetadata {
            schema_version: SCHEMA_VERSION,
            os_version: "macOS test".into(),
            gpu_registry_id: 1,
            families: vec![1009],
            compiler_version: COMPILER_VERSION.into(),
            shader_abi_version: SHADER_ABI_VERSION,
        }
    }
    #[test]
    fn test_archive_version_and_corrupt_metadata_recovery() {
        let directory =
            std::env::temp_dir().join(format!("katla-metal4-meta-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("manifest.json");
        fs::write(&path, b"partial {").unwrap();
        assert_eq!(
            read_manifest(&path, &metadata()).1,
            Some(ArchiveRejection::Corrupt)
        );
        let mut stale = metadata();
        stale.schema_version -= 1;
        atomic_write(
            &path,
            &serde_json::to_vec(&Manifest {
                metadata: stale,
                archives: BTreeMap::new(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_manifest(&path, &metadata()).1,
            Some(ArchiveRejection::MetadataMismatch)
        );
        atomic_write(
            &path,
            &serde_json::to_vec(&Manifest {
                metadata: metadata(),
                archives: BTreeMap::new(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(read_manifest(&path, &metadata()).1, None);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn test_shader_hash_determinism_and_source_invalidation() {
        assert_eq!(hash_bytes(b"source"), hash_bytes(b"source"));
        assert_ne!(hash_bytes(b"source"), hash_bytes(b"changed source"));
    }
    const TEST_WGSL: &str = "@vertex fn vs_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f { return vec4f(0.0,0.0,0.0,1.0); } @fragment fn fs_main()->@location(0) vec4f { return vec4f(1.0,0.0,0.0,1.0); }";
    fn graphics_descriptor(
        device: &ProtocolObject<dyn MTLDevice>,
        source: &str,
    ) -> Retained<MTLRenderPipelineDescriptor> {
        let shader = super::super::shader::compile_wgsl_to_metal(
            device,
            source,
            &["vs_main", "fs_main"],
            super::super::shader::ShaderProfile::Graphics,
        )
        .unwrap();
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&shader.module.entry_points["vs_main"]));
        descriptor.setFragmentFunction(Some(&shader.module.entry_points["fs_main"]));
        unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) }
            .setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);
        descriptor
    }
    #[test]
    fn test_render_key_determinism_and_option_invalidation() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let descriptor = graphics_descriptor(&device, TEST_WGSL);
        let original = render_descriptor(&descriptor, "depth=false;cull=none")
            .unwrap()
            .1;
        assert_eq!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=none")
                .unwrap()
                .1
        );
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=true;cull=none")
                .unwrap()
                .1
        );
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=back")
                .unwrap()
                .1
        );
        let changed_source = graphics_descriptor(
            &device,
            &TEST_WGSL.replace("vec4f(1.0,0.0,0.0,1.0)", "vec4f(0.0,1.0,0.0,1.0)"),
        );
        assert_ne!(
            original,
            render_descriptor(&changed_source, "depth=false;cull=none")
                .unwrap()
                .1
        );
        descriptor.setRasterSampleCount(4);
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=none")
                .unwrap()
                .1
        );
        descriptor.setRasterSampleCount(1);
        let color = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
        color.setBlendingEnabled(true);
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=none")
                .unwrap()
                .1
        );
        color.setBlendingEnabled(false);
        color.setPixelFormat(MTLPixelFormat::RGBA16Float);
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=none")
                .unwrap()
                .1
        );
        color.setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);
        let vertex = MTLVertexDescriptor::new();
        unsafe { vertex.attributes().objectAtIndexedSubscript(0) }
            .setFormat(MTLVertexFormat::Float3);
        unsafe {
            vertex.layouts().objectAtIndexedSubscript(0).setStride(12);
        }
        descriptor.setVertexDescriptor(Some(&vertex));
        assert_ne!(
            original,
            render_descriptor(&descriptor, "depth=false;cull=none")
                .unwrap()
                .1
        );
    }
    #[test]
    fn test_native_archive_warm_hit_and_corrupt_recovery() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let directory =
            std::env::temp_dir().join(format!("katla-metal4-native-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        eprintln!("NATIVE_DESCRIPTOR");
        let descriptor = graphics_descriptor(&device, TEST_WGSL);
        eprintln!("NATIVE_COLD");
        let cache = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        cache
            .create_render_pipeline(&descriptor, "depth=false")
            .unwrap();
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().pipelines_registered, 1, "{:?}", cache.stats());
        eprintln!("NATIVE_WARM");
        let warm = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        warm.create_render_pipeline(&descriptor, "depth=false")
            .unwrap();
        assert_eq!(warm.stats().hits, 1, "{:?}", warm.stats());
        let (_, key) = render_descriptor(&descriptor, "depth=false").unwrap();
        eprintln!("NATIVE_CORRUPT");
        fs::write(directory.join(format!("{key}.mtl4")), b"partial").unwrap();
        warm.manifest
            .lock()
            .unwrap()
            .archives
            .insert(key.clone(), hash_bytes(b"partial"));
        warm.create_render_pipeline(&descriptor, "depth=false")
            .unwrap();
        assert_eq!(warm.stats().misses, 1);
        assert_ne!(
            fs::read(directory.join(format!("{key}.mtl4"))).unwrap(),
            b"partial"
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn test_native_compute_archive_warm_hit_and_workgroup_invalidation() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let directory =
            std::env::temp_dir().join(format!("katla-metal4-compute-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let shader = super::super::shader::compile_wgsl_to_metal(
            &device,
            "@compute @workgroup_size(1) fn cs_main() {}",
            &["cs_main"],
            super::super::shader::ShaderProfile::Graphics,
        )
        .unwrap();
        let function = &shader.module.entry_points["cs_main"];
        let cold = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        cold.create_compute_pipeline(function, [1, 1, 1]).unwrap();
        let warm = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        warm.create_compute_pipeline(function, [1, 1, 1]).unwrap();
        assert_eq!(warm.stats().hits, 1, "{:?}", warm.stats());
        warm.create_compute_pipeline(function, [2, 1, 1]).unwrap();
        assert_eq!(warm.stats().misses, 1);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn test_truncated_archive_container_rejection() {
        assert!(!valid_archive_container(b"partial"));
        assert!(!valid_archive_container(&[
            0xcb, 0xfe, 0xba, 0xbe, 0, 0, 0, 1
        ]));
        let mut bytes = vec![0; 32];
        bytes[..8].copy_from_slice(&[0xcb, 0xfe, 0xba, 0xbe, 0, 0, 0, 1]);
        bytes[16..20].copy_from_slice(&28u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&8u32.to_be_bytes());
        assert!(!valid_archive_container(&bytes));
        bytes[20..24].copy_from_slice(&4u32.to_be_bytes());
        assert!(valid_archive_container(&bytes));
    }
    #[test]
    #[ignore = "Explicit Apple Silicon native latency measurement"]
    fn test_metal4_pipeline_latency_benchmark() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let directory =
            std::env::temp_dir().join(format!("katla-metal4-benchmark-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let started = Instant::now();
        let descriptor = graphics_descriptor(&device, TEST_WGSL);
        let cold = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        cold.create_render_pipeline(&descriptor, "depth=false")
            .unwrap();
        let cold_time = started.elapsed();
        let started = Instant::now();
        let warm = MetalPipelineArchive::open_or_create_in(&device, &directory).unwrap();
        let descriptor = graphics_descriptor(&device, TEST_WGSL);
        warm.create_render_pipeline(&descriptor, "depth=false")
            .unwrap();
        let warm_time = started.elapsed();
        let started = Instant::now();
        let reloaded = graphics_descriptor(
            &device,
            &TEST_WGSL.replace("vec4f(1.0,0.0,0.0,1.0)", "vec4f(0.0,1.0,0.0,1.0)"),
        );
        warm.create_render_pipeline(&reloaded, "depth=false")
            .unwrap();
        println!(
            "METAL4_PIPELINE_BENCH gpu={} cold_us={} warm_us={} reload_us={} warm_hits={}",
            device.name(),
            cold_time.as_micros(),
            warm_time.as_micros(),
            started.elapsed().as_micros(),
            warm.stats().hits
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
