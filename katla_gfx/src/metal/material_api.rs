use objc2::runtime::ProtocolObject;

use crate::error::RendererError;
use crate::handle::MaterialHandle;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::pipeline_variant::PipelineVariantKey;
use crate::texture::ImageFormat;

use super::metal_renderer::{MetalMaterial, MetalRenderer, read_shader};
use super::shader;

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

        let declared_format = match descriptor.color_format {
            ImageFormat::Auto if !descriptor.is_ui_layout() => ImageFormat::R16G16B16A16Sfloat,
            ImageFormat::Auto => ImageFormat::B8G8R8A8Srgb,
            format => format,
        };
        let wgsl_source = read_shader(&descriptor.shader_path)?;
        let mut variants = std::collections::HashMap::new();
        let formats = if descriptor.color_format != ImageFormat::R32Uint {
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
            let pipeline =
                Self::build_pipeline_for_key(&self.context, descriptor, &key, &wgsl_source)?;
            variants.insert(key, pipeline);
        }

        let instanced = if descriptor.is_ui_layout() {
            let instanced_descriptor = descriptor
                .clone()
                .with_graphics_entries("vs_instanced", "fs_instanced");
            let key = PipelineVariantKey::resolve(&instanced_descriptor, ImageFormat::B8G8R8A8Srgb);
            let pipeline = Self::build_pipeline_for_key(
                &self.context,
                &instanced_descriptor,
                &key,
                &wgsl_source,
            )?;
            variants.insert(key, pipeline.clone());
            Some(pipeline)
        } else {
            None
        };

        let handle = self.materials.insert(MetalMaterial {
            descriptor: descriptor.clone(),
            variants,
            pending_reload: None,
            textures: crate::renderer::registry::MaterialTextures::default(),
        });

        if let Some(instanced) = instanced {
            for ui in &mut self.ui_renderers {
                ui.set_instanced_pipeline(instanced.clone());
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
        context: &super::context::MetalContext,
        descriptor: &PipelineDescriptor,
        key: &PipelineVariantKey,
        wgsl_source: &str,
    ) -> Result<super::pipeline::MetalGraphicsPipeline, RendererError> {
        use crate::pipeline::CullMode;
        use crate::renderer::pipeline_descriptor::PipelineStages;
        use crate::vertex::VertexLayout;

        let PipelineStages::Graphics {
            vertex_entry,
            fragment_entry,
        } = &descriptor.stages
        else {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines have no graphics variants".to_string(),
            ));
        };

        log::debug!(
            "compile_material: shader_path={}, wgsl_size={} bytes",
            descriptor.shader_path,
            wgsl_source.len()
        );
        if wgsl_source.contains("pbr_lighting") {
            log::debug!("compile_material: WGSL contains PBR lighting code");
        }

        let entry_points = vec![vertex_entry.as_str(), fragment_entry.as_str()];

        let profile = if descriptor.is_ui_layout() {
            shader::ShaderProfile::Ui
        } else {
            shader::ShaderProfile::Graphics
        };

        let compiled =
            shader::compile_wgsl_to_metal(&context.device, wgsl_source, &entry_points, profile)?;

        let vertex_fn = compiled
            .module
            .entry_points
            .get(vertex_entry.as_str())
            .ok_or_else(|| {
                RendererError::InvalidOperation("Vertex entry point not found".into())
            })?;

        let fragment_fn = compiled.module.entry_points.get(fragment_entry.as_str());

        // Attachment formats come from the variant key, not from renderer
        // state: the resolved color format plus the shared depth derivation.
        let color_formats = &[super::format::to_mtl_pixel_format(key.color_format())];
        let depth_format = key.depth_format().map(super::format::to_mtl_pixel_format);

        let is_skinned = descriptor.vertex == VertexLayout::pbr_skinned();
        let is_billboard =
            descriptor.vertex == VertexLayout::pbr() && matches!(descriptor.cull, CullMode::None);

        let pipeline = if descriptor.is_ui_layout() {
            let vd = if vertex_entry == "vs_instanced" {
                super::context::ui_instanced_vertex_descriptor()
            } else {
                super::context::ui_vertex_descriptor()
            };
            context.create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                false,
                crate::pipeline::CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                Some(&vd),
                true,
            )?
        } else if is_skinned {
            let vd = super::context::pbr_skinned_vertex_descriptor();
            context.create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                descriptor.depth.write,
                descriptor.depth.compare,
                objc2_metal::MTLCullMode::Back,
                objc2_metal::MTLWinding::Clockwise,
                Some(&vd),
                false,
            )?
        } else if is_billboard {
            context.create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                descriptor.depth.write,
                descriptor.depth.compare,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                None,
                true,
            )?
        } else {
            let vd = super::context::default_pbr_vertex_descriptor();
            context.create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                descriptor.depth.write,
                descriptor.depth.compare,
                objc2_metal::MTLCullMode::Back,
                objc2_metal::MTLWinding::Clockwise,
                Some(&vd),
                false,
            )?
        };

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
                Ok(Ok(variants)) => {
                    if material.descriptor.is_ui_layout() {
                        use crate::renderer::pipeline_descriptor::PipelineStages;
                        if let Some((_, instanced)) = variants.iter().find(|(key, _)| matches!(&key.descriptor().stages, PipelineStages::Graphics { vertex_entry, .. } if vertex_entry == "vs_instanced")) {
                            for ui in &mut self.ui_renderers { ui.set_instanced_pipeline(instanced.clone()); }
                        }
                    }
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
        mat.variants.get(&key).cloned().ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Material {material:?} has no pipeline variant for color {:?} / depth {:?}",
                key.color_format(),
                key.depth_format()
            ))
        })
    }

    /// Whether the handle references a registered material.
    ///
    /// Draw collection skips unknown handles with a warning; every known
    /// handle must already have its requested variant ready.
    pub(crate) fn has_material_impl(&self, material: MaterialHandle) -> bool {
        self.materials.get(material).is_some()
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
    /// indices. `NONE` and stale handles resolve to the role's default
    /// texture slot; this is the only place material texture handles
    /// become shader-visible numbers on Metal.
    pub(crate) fn resolve_material_texture_slots_impl(&self, material: MaterialHandle) -> [u32; 4] {
        use crate::texture::{
            DEFAULT_ALBEDO_SLOT, DEFAULT_MR_SLOT, DEFAULT_NORMAL_SLOT, DEFAULT_OCCLUSION_SLOT,
        };
        let textures = self
            .materials
            .get(material)
            .map(|mat| mat.textures)
            .unwrap_or_default();
        [
            self.get_bindless_slot_impl(textures.albedo)
                .unwrap_or(DEFAULT_ALBEDO_SLOT),
            self.get_bindless_slot_impl(textures.normal)
                .unwrap_or(DEFAULT_NORMAL_SLOT),
            self.get_bindless_slot_impl(textures.metallic_roughness)
                .unwrap_or(DEFAULT_MR_SLOT),
            self.get_bindless_slot_impl(textures.occlusion)
                .unwrap_or(DEFAULT_OCCLUSION_SLOT),
        ]
    }

    pub(crate) fn default_material_impl(&self) -> MaterialHandle {
        self.default_material.unwrap_or_default()
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
        if changed_path
            .extension()
            .is_none_or(|extension| extension != "wgsl")
        {
            return 0;
        }
        let handles: Vec<_> = self
            .materials
            .iter_enumerated()
            .filter_map(|(handle, mat)| (!mat.descriptor.shader_path.is_empty()).then_some(handle))
            .collect();
        let count = handles.len();
        for handle in handles {
            let Some(material) = self.materials.get_mut(handle) else {
                continue;
            };
            let descriptor = material.descriptor.clone();
            let keys: Vec<_> = material.variants.keys().cloned().collect();
            let (sender, receiver) = std::sync::mpsc::channel();
            material.pending_reload = Some(receiver);
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let result = (|| -> Result<_, RendererError> {
                    let context = super::context::MetalContext::init_headless_with_size(1, 1)?;
                    let source = read_shader(&descriptor.shader_path)?;
                    let mut replacements = std::collections::HashMap::new();
                    for key in keys {
                        let pipeline = Self::build_pipeline_for_key(
                            &context,
                            key.descriptor(),
                            &key,
                            &source,
                        )?;
                        replacements.insert(key, pipeline);
                    }
                    Ok(replacements)
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
