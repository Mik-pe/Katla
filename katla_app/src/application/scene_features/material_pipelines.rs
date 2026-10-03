//! Application-owned raster-state variants for authored scene surfaces.

use crate::{AppResult, Renderer, rendering::SurfaceParameters};
use katla_gfx::{BlendMode, CullMode, GpuRenderer, MaterialHandle};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(crate) enum CoveragePass {
    Depth,
    Picking,
    Shadow { reverse_winding: bool },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RasterState {
    cull: CullMode,
    blend: BlendMode,
    depth_write: bool,
}

#[derive(Default)]
pub(crate) struct MaterialPipelines {
    variants: HashMap<(MaterialHandle, RasterState), MaterialHandle>,
}

impl MaterialPipelines {
    pub(crate) fn resolve(
        &mut self,
        renderer: &mut Renderer,
        source: MaterialHandle,
        surface: SurfaceParameters,
    ) -> AppResult<MaterialHandle> {
        let Some(original) = renderer.material_descriptor(source) else {
            return Err(katla_gfx::RendererError::InvalidOperation(format!(
                "Material {source:?} is stale"
            ))
            .into());
        };
        let state = RasterState {
            cull: surface.cull_mode(),
            blend: if surface.transparent() {
                BlendMode::AlphaBlend
            } else {
                BlendMode::Opaque
            },
            depth_write: original.depth.write && !surface.transparent(),
        };
        self.variant(renderer, source, state)
    }

    fn variant(
        &mut self,
        renderer: &mut Renderer,
        source: MaterialHandle,
        state: RasterState,
    ) -> AppResult<MaterialHandle> {
        let original = renderer.material_descriptor(source).ok_or_else(|| {
            katla_gfx::RendererError::InvalidOperation("Scene material is stale".into())
        })?;
        if original.cull == state.cull
            && original.blend == state.blend
            && original.depth.write == state.depth_write
        {
            return Ok(source);
        }
        let key = (source, state);
        if let Some(handle) = self.variants.get(&key) {
            return Ok(*handle);
        }
        let mut descriptor = original.clone();
        descriptor.cull = state.cull;
        descriptor.blend = state.blend;
        descriptor.depth.write = state.depth_write;
        let handle = renderer.compile_material(&descriptor)?;
        self.variants.insert(key, handle);
        Ok(handle)
    }

    pub(crate) fn prepare_draws(
        &mut self,
        renderer: &mut Renderer,
        draws: &mut katla_gfx::renderer::DrawList,
        surfaces: &[SurfaceParameters],
    ) -> AppResult<()> {
        self.variants.retain(|(source, _), variant| {
            if renderer.material_descriptor(*source).is_some() {
                true
            } else {
                renderer.destroy_material(*variant);
                false
            }
        });
        for draw in draws.iter_mut() {
            let Some(surface) = surfaces.get(draw.base_object_slot() as usize).copied() else {
                continue;
            };
            let source = draw.material;
            let textures = renderer.material_textures(source);
            draw.material = self.resolve(renderer, source, surface)?;
            if let Some(textures) = textures {
                renderer.set_material_textures(draw.material, textures);
            }
        }
        Ok(())
    }
    pub(crate) fn auxiliary_phases(
        &mut self,
        renderer: &mut Renderer,
        pipelines: &[katla_gfx::renderer::frame_bindings::PassPipeline],
        indices: &[u32],
        surfaces: &[SurfaceParameters],
        policy: CoveragePass,
    ) -> AppResult<Vec<katla_gfx::renderer::frame_bindings::PassDrawPhase>> {
        use katla_gfx::renderer::frame_bindings::{PassDraw, PassDrawPhase, PassPipeline};
        let include_blend = matches!(policy, CoveragePass::Picking);
        let reverse_winding = matches!(
            policy,
            CoveragePass::Shadow {
                reverse_winding: true
            }
        );
        let mut phases = Vec::new();
        for cull in [
            katla_gfx::CullMode::Back,
            katla_gfx::CullMode::Front,
            katla_gfx::CullMode::None,
        ] {
            let selected: Vec<_> = indices
                .iter()
                .copied()
                .filter(|&index| {
                    surfaces.get(index as usize).is_some_and(|surface| {
                        surface.cull_mode() == cull && (include_blend || !surface.transparent())
                    })
                })
                .collect();
            if selected.is_empty() {
                continue;
            }
            let mut variants = Vec::new();
            for pipeline in pipelines {
                let descriptor =
                    renderer
                        .material_descriptor(pipeline.material)
                        .ok_or_else(|| {
                            katla_gfx::RendererError::InvalidOperation(
                                "Scene pass material is stale".into(),
                            )
                        })?;
                let cull = if reverse_winding {
                    match cull {
                        CullMode::Front => CullMode::Back,
                        CullMode::Back => CullMode::Front,
                        other => other,
                    }
                } else {
                    cull
                };
                let state = RasterState {
                    cull,
                    blend: descriptor.blend,
                    depth_write: descriptor.depth.write,
                };
                let material = self.variant(renderer, pipeline.material, state)?;
                variants.push(PassPipeline {
                    vertex_layout: pipeline.vertex_layout.clone(),
                    material,
                });
            }
            phases.push(PassDrawPhase {
                pipelines: variants,
                constants: Vec::new(),
                draw: PassDraw::ObjectIndices(selected),
                viewport: None,
            });
        }
        if phases.is_empty() {
            phases.push(PassDrawPhase {
                pipelines: Vec::new(),
                constants: Vec::new(),
                draw: PassDraw::ObjectIndices(Vec::new()),
                viewport: None,
            });
        }
        Ok(phases)
    }
}
