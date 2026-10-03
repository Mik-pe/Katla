use objc2::runtime::ProtocolObject;

use crate::error::RendererError;
use crate::handle::MaterialHandle;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::pipeline_variant::PipelineVariantKey;
use crate::texture::ImageFormat;

use super::metal_renderer::{MetalMaterial, MetalMaterialReplacement, MetalRenderer};
use super::shader;

/// Weak ownership lets material removal and submission completion retire native state.
pub(crate) type MetalGraphicsCache = std::collections::HashMap<
    crate::renderer::pipeline_variant::GraphicsCompilationKey,
    std::sync::Weak<super::pipeline::MetalGraphicsPipeline>,
>;

impl MetalRenderer {
    pub(crate) fn compile_material_impl(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        descriptor.validate()?;
        self.bindless_manager.initialize(&self.context.device)?;
        if !descriptor.specialization.is_empty() {
            return Err(RendererError::UnsupportedFeature(
                "specialization constants are not yet supported by the Metal backend".to_string(),
            ));
        }
        if descriptor.stages.is_compute() {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines are not yet supported by Metal compile_material".to_string(),
            ));
        }

        let declared_format =
            PipelineVariantKey::resolve(descriptor, ImageFormat::Auto).color_format();
        let source = crate::renderer::shader_source::ShaderSource::load(std::path::Path::new(
            &descriptor.shader_path,
        ))
        .map_err(|error| RendererError::InvalidOperation(error.to_string()))?;
        let wgsl_source = &source.code;
        let compiler = self.context.pipeline_compiler()?;
        let mut variants = std::collections::HashMap::new();
        let formats =
            if descriptor.color_attachment && descriptor.color_format != ImageFormat::R32Uint {
                vec![
                    ImageFormat::R8G8B8A8Srgb,
                    ImageFormat::R8G8B8A8Unorm,
                    ImageFormat::B8G8R8A8Srgb,
                    ImageFormat::R8Unorm,
                    ImageFormat::Rg8Unorm,
                    ImageFormat::R32Sfloat,
                    ImageFormat::R16G16B16A16Sfloat,
                ]
            } else {
                vec![declared_format]
            };
        for format in formats {
            let key = PipelineVariantKey::resolve(descriptor, format);
            let pipeline = Self::build_pipeline_for_key(&compiler, descriptor, &key, wgsl_source)?;
            variants.insert(key, pipeline);
        }

        let instanced = if descriptor.is_ui_layout() {
            let instanced_descriptor = descriptor
                .clone()
                .with_graphics_entries("vs_instanced", "fs_instanced");
            let key = PipelineVariantKey::resolve(&instanced_descriptor, ImageFormat::B8G8R8A8Srgb);
            let pipeline =
                Self::build_pipeline_for_key(&compiler, &instanced_descriptor, &key, wgsl_source)?;
            variants.insert(key, pipeline.clone());
            Some(pipeline)
        } else {
            None
        };

        let interface = crate::renderer::graphics_interface::GraphicsInterface::reflect(
            wgsl_source,
            &descriptor.stages,
        )
        .map_err(RendererError::InvalidOperation)?;
        let handle = self.materials.insert(MetalMaterial {
            interface,
            dependencies: source.dependencies,
            descriptor: descriptor.clone(),
            variants,
            pending_reload: None,
            textures: crate::renderer::registry::MaterialTextures::default(),
        });

        if let Some(instanced) = instanced {
            for ui in &mut self.ui_renderers {
                ui.set_instanced_pipeline(instanced.as_ref().clone());
            }
        }

        Ok(handle)
    }

    /// Compile one pipeline variant of a material for its canonical key.
    ///
    /// Attachment formats come from the key (the same shared derivation the
    /// Vulkan backend resolves), so both backends build identical logical
    /// variants for the same configuration.
    fn build_pipeline_for_key(
        context: &super::context::MetalPipelineCompiler,
        descriptor: &PipelineDescriptor,
        key: &PipelineVariantKey,
        wgsl_source: &str,
    ) -> Result<std::sync::Arc<super::pipeline::MetalGraphicsPipeline>, RendererError> {
        let compilation_key =
            crate::renderer::pipeline_variant::GraphicsCompilationKey::new(key, wgsl_source);
        let mut cache = context.graphics_cache.lock().map_err(|_| {
            RendererError::InvalidOperation("Graphics pipeline cache lock poisoned".into())
        })?;
        if let Some(pipeline) = cache
            .get(&compilation_key)
            .and_then(std::sync::Weak::upgrade)
        {
            return Ok(pipeline);
        }
        cache.retain(|_, pipeline| pipeline.strong_count() != 0);
        use crate::pipeline::CullMode;
        use crate::renderer::pipeline_descriptor::{BlendMode, PipelineStages};
        let PipelineStages::Graphics {
            vertex_entry,
            fragment_entry,
        } = &descriptor.stages
        else {
            return Err(RendererError::UnsupportedFeature(
                "Compute pipelines have no graphics variants".into(),
            ));
        };
        let entries = std::iter::once(vertex_entry.as_str())
            .chain(fragment_entry.as_deref())
            .collect::<Vec<_>>();
        let compiled = shader::compile_wgsl_to_metal(
            &context.device,
            wgsl_source,
            &entries,
            shader::ShaderProfile::Graphics,
        )?;
        let vertex_function = compiled
            .module
            .entry_points
            .get(vertex_entry)
            .ok_or_else(|| RendererError::InvalidOperation("Vertex entry point missing".into()))?;
        let fragment_function = fragment_entry
            .as_ref()
            .map(|entry| {
                compiled.module.entry_points.get(entry).ok_or_else(|| {
                    RendererError::InvalidOperation("Fragment entry point missing".into())
                })
            })
            .transpose()?;
        let colors = if descriptor.color_attachment {
            vec![super::format::to_mtl_pixel_format(key.color_format())]
        } else {
            Vec::new()
        };
        let layout = if descriptor.is_ui_layout() && vertex_entry == "vs_instanced" {
            super::context::ui_instanced_vertex_descriptor()
        } else {
            vertex_descriptor(&descriptor.vertex)
        };
        let pipeline = std::sync::Arc::new(context.create_graphics_pipeline(
            crate::metal::context::GraphicsPipelineConfig {
                vertex_function,
                fragment_function:
                    fragment_function.map(|function| {
                        &**function as &ProtocolObject<dyn objc2_metal::MTLFunction>
                    }),
                color_formats: &colors,
                depth_format: key.depth_format().map(super::format::to_mtl_pixel_format),
                depth_write_enabled: descriptor.depth.write,
                depth_compare: descriptor.depth.compare,
                cull_mode: match descriptor.cull {
                    CullMode::None => objc2_metal::MTLCullMode::None,
                    CullMode::Front => objc2_metal::MTLCullMode::Front,
                    CullMode::Back => objc2_metal::MTLCullMode::Back,
                    CullMode::FrontAndBack => {
                        return Err(RendererError::UnsupportedFeature(
                            "Metal does not support front-and-back culling".into(),
                        ));
                    }
                },
                front_face: objc2_metal::MTLWinding::Clockwise,
                vertex_descriptor: &layout,
                alpha_blended: descriptor.blend == BlendMode::AlphaBlend,
                portable: Some(key.descriptor()),
            },
        )?);
        cache.insert(compilation_key, std::sync::Arc::downgrade(&pipeline));
        Ok(pipeline)
    }

    /// Verify startup warmup completed before acquiring or encoding a frame.
    pub(crate) fn ensure_material_variant_impl(
        &mut self,
        material: MaterialHandle,
        requested_format: ImageFormat,
    ) -> Result<(), RendererError> {
        self.poll_material_reloads_impl();
        let mat = self.materials.get(material).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Material handle {material:?} not found"))
        })?;
        let key = PipelineVariantKey::resolve(&mat.descriptor, requested_format);
        if mat.variants.contains_key(&key) {
            return Ok(());
        }
        Err(RendererError::InvalidOperation(format!(
            "Material {material:?} variant {requested_format:?} was not registered during startup warmup"
        )))
    }

    pub(crate) fn poll_material_reloads_impl(&mut self) {
        let handles: Vec<_> = self
            .materials
            .iter_enumerated()
            .map(|(handle, _)| handle)
            .collect();
        for handle in handles {
            let Some(material) = self.materials.get_mut(handle) else {
                continue;
            };
            let Some(receiver) = material.pending_reload.as_ref() else {
                continue;
            };
            match receiver.try_recv() {
                Ok(Ok(replacement)) => {
                    let variants = replacement.variants;
                    if material.descriptor.is_ui_layout() {
                        use crate::renderer::pipeline_descriptor::PipelineStages;
                        if let Some((_, instanced)) = variants.iter().find(|(key, _)| matches!(&key.descriptor().stages, PipelineStages::Graphics { vertex_entry, .. } if vertex_entry == "vs_instanced")) {
                            for ui in &mut self.ui_renderers { ui.set_instanced_pipeline(instanced.as_ref().clone()); }
                        }
                    }
                    material.interface = replacement.interface;
                    material.dependencies = replacement.dependencies;
                    material.variants = variants;
                    material.pending_reload = None;
                    log::info!("pipeline_cache event=reload_swap material={handle:?}");
                }
                Ok(Err(error)) => {
                    material.pending_reload = None;
                    log::warn!(
                        "pipeline_cache event=reload_failed material={handle:?} reason={error}"
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    material.pending_reload = None;
                    log::warn!(
                        "pipeline_cache event=reload_failed material={handle:?} reason=worker_disconnected"
                    );
                }
            }
        }
    }

    /// Look up the compiled variant pipeline for a material at a color
    /// format. Encoding runs on `&self`, so every pass's variants must have
    /// been ensured before encoding begins.
    pub(crate) fn material_pipeline(
        &self,
        material: MaterialHandle,
        requested_format: ImageFormat,
    ) -> Result<super::pipeline::MetalGraphicsPipeline, RendererError> {
        let mat = self.materials.get(material).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Material handle {material:?} not found"))
        })?;
        let key = PipelineVariantKey::resolve(&mat.descriptor, requested_format);
        mat.variants
            .get(&key)
            .map(|pipeline| pipeline.as_ref().clone())
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!(
                    "Material {material:?} has no pipeline variant for color {:?} / depth {:?}",
                    key.color_format(),
                    key.depth_format()
                ))
            })
    }

    pub(crate) fn set_material_textures_impl(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    ) {
        if let Some(mat) = self.materials.get_mut(material) {
            mat.textures = textures;
        }
    }

    /// Resolve a material's typed texture bindings to argument-table
    /// indices. Missing handles use the descriptor-safe fallback slot.
    pub(crate) fn resolve_material_texture_slots_impl(&self, material: MaterialHandle) -> [u32; 4] {
        let textures = self
            .materials
            .get(material)
            .map(|mat| mat.textures)
            .unwrap_or_default();
        [
            self.get_bindless_slot_impl(textures.albedo).unwrap_or(0),
            self.get_bindless_slot_impl(textures.normal).unwrap_or(0),
            self.get_bindless_slot_impl(textures.metallic_roughness)
                .unwrap_or(0),
            self.get_bindless_slot_impl(textures.occlusion).unwrap_or(0),
        ]
    }

    pub(crate) fn destroy_material_impl(&mut self, handle: MaterialHandle) {
        self.materials.remove(handle);
    }

    /// Compile replacements on a worker and retain the previous pipelines
    /// until every registered variant has compiled successfully.
    pub(crate) fn recompile_materials_for_shader_impl(
        &mut self,
        changed_path: &std::path::Path,
    ) -> usize {
        let identity = crate::renderer::shader_source::path_identity(changed_path);
        let handles: Vec<_> = self
            .materials
            .iter_enumerated()
            .filter_map(|(handle, mat)| mat.dependencies.contains(&identity).then_some(handle))
            .collect();
        let count = handles.len();
        for handle in handles {
            let Some(material) = self.materials.get_mut(handle) else {
                continue;
            };
            let compiler = match self.context.pipeline_compiler() {
                Ok(compiler) => compiler,
                Err(error) => {
                    log::warn!(
                        "pipeline_cache event=reload_failed material={handle:?} reason={error}"
                    );
                    continue;
                }
            };
            let descriptor = material.descriptor.clone();
            let source = match crate::renderer::shader_source::ShaderSource::load(
                std::path::Path::new(&descriptor.shader_path),
            ) {
                Ok(source) => source,
                Err(error) => {
                    material.pending_reload = None;
                    log::warn!(
                        "pipeline_cache event=reload_failed material={handle:?} reason={error}"
                    );
                    continue;
                }
            };
            let keys: Vec<_> = material.variants.keys().cloned().collect();
            let (sender, receiver) = std::sync::mpsc::channel();
            material.pending_reload = Some(receiver);
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let result = (|| -> Result<_, RendererError> {
                    let mut replacements = std::collections::HashMap::new();
                    for key in keys {
                        let pipeline = Self::build_pipeline_for_key(
                            &compiler,
                            key.descriptor(),
                            &key,
                            &source.code,
                        )?;
                        replacements.insert(key, pipeline);
                    }
                    let interface =
                        crate::renderer::graphics_interface::GraphicsInterface::reflect(
                            &source.code,
                            &descriptor.stages,
                        )
                        .map_err(RendererError::InvalidOperation)?;
                    Ok(MetalMaterialReplacement {
                        dependencies: source.dependencies,
                        interface,
                        variants: replacements,
                    })
                })()
                .map_err(|error| error.to_string());
                log::info!(
                    "pipeline_cache event=reload_complete shader={} success={} elapsed_us={}",
                    descriptor.shader_path,
                    result.is_ok(),
                    started.elapsed().as_micros()
                );
                let _ = sender.send(result);
            });
        }
        count
    }
}

fn vertex_descriptor(
    layout: &crate::vertex::VertexLayout,
) -> objc2::rc::Retained<objc2_metal::MTLVertexDescriptor> {
    use crate::vertex::VertexAttributeFormat::*;
    let native = objc2_metal::MTLVertexDescriptor::new();
    let mut offset = 0;
    for field in layout.attributes() {
        let format = field.format;
        let attribute = unsafe {
            native
                .attributes()
                .objectAtIndexedSubscript(field.location as usize)
        };
        attribute.setFormat(match format {
            Float => objc2_metal::MTLVertexFormat::Float,
            Float2 => objc2_metal::MTLVertexFormat::Float2,
            Float3 => objc2_metal::MTLVertexFormat::Float3,
            Float4 => objc2_metal::MTLVertexFormat::Float4,
            UByte4 => objc2_metal::MTLVertexFormat::UChar4,
            UByte4Norm => objc2_metal::MTLVertexFormat::UChar4Normalized,
            UShort4 => objc2_metal::MTLVertexFormat::UShort4,
            UShort4Norm => objc2_metal::MTLVertexFormat::UShort4Normalized,
            Int => objc2_metal::MTLVertexFormat::Int,
            UInt => objc2_metal::MTLVertexFormat::UInt,
        });
        unsafe {
            attribute.setOffset(offset);
            attribute.setBufferIndex(10);
        }
        offset += format.size_bytes();
    }
    if !layout.is_empty() {
        let binding = unsafe { native.layouts().objectAtIndexedSubscript(10) };
        unsafe {
            binding.setStride(layout.stride());
            binding.setStepFunction(objc2_metal::MTLVertexStepFunction::PerVertex);
            binding.setStepRate(1);
        }
    }
    native
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::MTLDevice;
    use std::time::{Duration, Instant};

    const SOURCE: &str = "@vertex fn vs_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f { return vec4f(0.0,0.0,0.0,1.0); } @fragment fn fs_main()->@location(0) vec4f { return vec4f(1.0,0.0,0.0,1.0); }";

    fn wait_for_reload(renderer: &mut MetalRenderer, handle: MaterialHandle) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while renderer
            .materials
            .get(handle)
            .unwrap()
            .pending_reload
            .is_some()
        {
            renderer.poll_material_reloads_impl();
            assert!(Instant::now() < deadline, "reload worker did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn test_async_reload_retains_previous_pipeline_on_failure_and_swaps_success() {
        let path =
            std::env::temp_dir().join(format!("katla-metal4-reload-{}.wgsl", std::process::id()));
        std::fs::write(&path, SOURCE).unwrap();
        let context = super::super::context::MetalContext::init_headless().unwrap();
        let mut renderer = MetalRenderer::new(context).unwrap();
        let descriptor = PipelineDescriptor::simple(path.to_string_lossy());
        let handle = renderer.compile_material_impl(&descriptor).unwrap();
        let original = renderer
            .material_pipeline(handle, ImageFormat::B8G8R8A8Srgb)
            .unwrap();
        let original_count = renderer.materials.get(handle).unwrap().variants.len();
        let misses = renderer
            .context
            .pipeline_archive
            .as_ref()
            .unwrap()
            .stats()
            .misses;
        renderer
            .ensure_material_variant_impl(handle, ImageFormat::R16G16B16A16Sfloat)
            .unwrap();
        assert_eq!(
            renderer
                .context
                .pipeline_archive
                .as_ref()
                .unwrap()
                .stats()
                .misses,
            misses,
            "frame preparation started a compile"
        );
        std::fs::write(&path, "invalid shader").unwrap();
        assert_eq!(renderer.recompile_materials_for_shader_impl(&path), 1);
        assert_eq!(
            renderer.materials.get(handle).unwrap().variants.len(),
            original_count
        );
        wait_for_reload(&mut renderer, handle);
        let retained = renderer
            .material_pipeline(handle, ImageFormat::B8G8R8A8Srgb)
            .unwrap();
        assert!(std::ptr::eq(
            &*original.pipeline_state,
            &*retained.pipeline_state
        ));
        std::fs::write(
            &path,
            SOURCE.replace("vec4f(1.0,0.0,0.0,1.0)", "vec4f(0.0,1.0,0.0,1.0)"),
        )
        .unwrap();
        let reload_started = Instant::now();
        assert_eq!(renderer.recompile_materials_for_shader_impl(&path), 1);
        wait_for_reload(&mut renderer, handle);
        println!(
            "METAL4_MATERIAL_RELOAD gpu={} variants={} elapsed_us={}",
            renderer.context.device.name(),
            original_count,
            reload_started.elapsed().as_micros()
        );
        let replacement = renderer
            .material_pipeline(handle, ImageFormat::B8G8R8A8Srgb)
            .unwrap();
        assert!(!std::ptr::eq(
            &*original.pipeline_state,
            &*replacement.pipeline_state
        ));
        assert_eq!(
            renderer.materials.get(handle).unwrap().variants.len(),
            original_count
        );
        std::fs::remove_file(path).unwrap();
    }
}
