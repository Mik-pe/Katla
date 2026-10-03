//! Application-owned raster-state variants for authored scene surfaces.

use crate::{AppResult, Renderer, rendering::SurfaceParameters};
use katla_gfx::ShaderStages;
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
        rows: crate::rendering::frame_context::SurfaceRows<'_>,
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
        let mut groups: Vec<(CullMode, katla_gfx::SamplerDescriptor, Vec<u32>)> = Vec::new();
        let mut group_indices = HashMap::new();
        for &index in indices {
            let Some(surface) = rows.parameters.get(index as usize) else {
                continue;
            };
            if !include_blend && surface.transparent() {
                continue;
            }
            let cull = surface.cull_mode();
            let sampler = rows
                .samplers
                .get(index as usize)
                .map_or(katla_gfx::SamplerDescriptor::linear_repeat(), |samplers| {
                    samplers[0]
                });
            let key = (cull, sampler);
            if let Some(&group) = group_indices.get(&key) {
                let (_, _, selected): &mut (CullMode, katla_gfx::SamplerDescriptor, Vec<u32>) =
                    &mut groups[group];
                selected.push(index);
            } else {
                group_indices.insert(key, groups.len());
                groups.push((cull, sampler, vec![index]));
            }
        }
        for (cull, sampler, selected) in groups {
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
                samplers: vec![katla_gfx::renderer::frame_bindings::SamplerBinding {
                    group: 5,
                    binding: 0,
                    sampling: sampler,
                    stages: ShaderStages::FRAGMENT,
                }],
                pipelines: variants,
                constants: Vec::new(),
                draw: PassDraw::ObjectIndices(selected),
                viewport: None,
            });
        }
        if phases.is_empty() {
            phases.push(PassDrawPhase {
                samplers: Vec::new(),
                pipelines: Vec::new(),
                constants: Vec::new(),
                draw: PassDraw::ObjectIndices(Vec::new()),
                viewport: None,
            });
        }
        Ok(phases)
    }
}

/// Group only consecutive sorted objects so transparent compositing order is preserved.
pub(crate) fn geometry_phases(
    indices: &[u32],
    samplers: &[[katla_gfx::SamplerDescriptor; 5]],
) -> Vec<katla_gfx::renderer::frame_bindings::PassDrawPhase> {
    use katla_gfx::renderer::frame_bindings::{PassDraw, PassDrawPhase, SamplerBinding};
    let mut groups: Vec<([katla_gfx::SamplerDescriptor; 5], Vec<u32>)> = Vec::new();
    for &index in indices {
        let policy = samplers
            .get(index as usize)
            .copied()
            .unwrap_or_else(|| crate::rendering::MaterialSampling::default().samplers());
        if let Some((previous, selected)) = groups.last_mut()
            && *previous == policy
        {
            selected.push(index);
        } else {
            groups.push((policy, vec![index]));
        }
    }
    if groups.is_empty() {
        groups.push((
            crate::rendering::MaterialSampling::default().samplers(),
            Vec::new(),
        ));
    }
    groups
        .into_iter()
        .map(|(policy, selected)| PassDrawPhase {
            samplers: policy
                .into_iter()
                .enumerate()
                .map(|(binding, sampling)| SamplerBinding {
                    group: 5,
                    binding: binding as u32,
                    sampling,
                    stages: ShaderStages::FRAGMENT,
                })
                .collect(),
            pipelines: Vec::new(),
            constants: Vec::new(),
            draw: PassDraw::ObjectIndices(selected),
            viewport: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_sampler_phases_preserve_nonconsecutive_sorted_object_order() {
        use katla_gfx::{PassDraw, SamplerDescriptor};
        let a = [SamplerDescriptor::linear_repeat(); 5];
        let b = [SamplerDescriptor::nearest_clamp(); 5];
        let phases = geometry_phases(&[4, 3, 1, 2], &[a, b, a, b, a]);
        let slots: Vec<_> = phases
            .iter()
            .map(|phase| match &phase.draw {
                PassDraw::ObjectIndices(indices) => indices.clone(),
                _ => panic!("object phase"),
            })
            .collect();
        assert_eq!(slots, [vec![4], vec![3, 1], vec![2]]);
        assert_eq!(phases[1].samplers[4].sampling, b[4]);
        assert!(
            matches!(&geometry_phases(&[], &[])[0].draw, PassDraw::ObjectIndices(indices) if indices.is_empty())
        );
    }
}
