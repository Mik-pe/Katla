//! Studio-lit PBR thumbnails shared by the material palette and inspector.
use super::preview_maps::PreviewMaps;
use std::sync::Arc;

use crate::ui::UIRenderer;
use katla_agent::material::{MaterialPreset, MaterialValues};
use katla_gfx::{AnyRenderer, GpuRenderer, RendererError, TextureDescriptor, TextureHandle};
use katla_math::Color;
use katla_ui::TextureId;

const SIZE: u32 = 192;

#[derive(Default)]
pub(crate) struct MaterialPreviews {
    presets: [Option<TextureHandle>; 6],
    current: Option<(MaterialValues, TextureHandle, Option<Arc<PreviewMaps>>)>,
}

impl MaterialPreviews {
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut AnyRenderer,
        ui: &mut UIRenderer,
        values: Option<MaterialValues>,
        maps: Option<Arc<PreviewMaps>>,
    ) -> Result<(), RendererError> {
        for (index, preset) in MaterialPreset::ALL.iter().enumerate() {
            if self.presets[index].is_none() {
                self.presets[index] = Some(upload(gfx, ui, preset.values(), None)?);
            }
        }
        if let Some(values) = values
            && self.current.as_ref().is_none_or(|(old, _, old_maps)| {
                *old != values
                    || match (old_maps, &maps) {
                        (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                        (None, None) => false,
                        _ => true,
                    }
            })
        {
            values.validate().map_err(RendererError::InvalidOperation)?;
            let texture = if let Some((_, texture, _)) = &self.current {
                gfx.update_texture(*texture, &render_sphere(values, SIZE, maps.as_deref()))?;
                *texture
            } else {
                upload(gfx, ui, values, maps.as_deref())?
            };
            self.current = Some((values, texture, maps));
        }
        Ok(())
    }

    pub(crate) fn presets(&self) -> [Option<TextureId>; 6] {
        self.presets.map(|handle| handle.map(texture_id))
    }

    pub(crate) fn current(&self) -> Option<TextureId> {
        self.current
            .as_ref()
            .map(|(_, handle, _)| texture_id(*handle))
    }
}

fn texture_id(handle: TextureHandle) -> TextureId {
    TextureId::from_handle(handle.index(), handle.generation())
}

fn upload(
    gfx: &mut AnyRenderer,
    ui: &mut UIRenderer,
    values: MaterialValues,
    maps: Option<&PreviewMaps>,
) -> Result<TextureHandle, RendererError> {
    values.validate().map_err(RendererError::InvalidOperation)?;
    let pixels = render_sphere(values, SIZE, maps);
    let texture = gfx.create_texture(&TextureDescriptor::rgba8_srgb(SIZE, SIZE), &pixels)?;
    let Some(slot) = gfx.get_bindless_slot(texture) else {
        gfx.destroy_texture(texture);
        return Err(RendererError::InvalidOperation(
            "Material preview has no sampled texture slot".into(),
        ));
    };
    ui.register_bindless_slot(texture, slot);
    Ok(texture)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    v.map(|v| v / length)
}

/// Shade an antialiased sphere using the authored sRGB factors, GGX and studio lights.
/// Imported maps use the same role channels and factor multipliers as model PBR.
fn render_sphere(values: MaterialValues, size: u32, maps: Option<&PreviewMaps>) -> Vec<u8> {
    let albedo = Color::new(
        values.base_color[0],
        values.base_color[1],
        values.base_color[2],
        1.0,
    )
    .to_linear();
    let factor = [albedo.r, albedo.g, albedo.b];
    let light = normalize([-0.55, 0.75, 1.0]);
    let half = normalize([light[0], light[1], light[2] + 1.0]);
    let mut pixels = vec![0; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let nx = ((x as f32 + 0.5) / size as f32 - 0.5) * 2.3;
            let ny = (0.5 - (y as f32 + 0.5) / size as f32) * 2.3;
            let radius2 = nx * nx + ny * ny;
            if radius2 > 1.0 + 2.3 / size as f32 {
                continue;
            }
            let z = (1.0 - radius2).max(0.000001).sqrt();
            let sphere_normal = normalize([nx, ny, z]);
            let uv = [
                0.5 + nx.atan2(z) / std::f32::consts::TAU,
                sphere_normal[1].clamp(-1.0, 1.0).acos() / std::f32::consts::PI,
            ];
            let albedo_map = maps
                .and_then(|m| m.albedo.as_ref())
                .map_or([1.0; 4], |m| m.sample(uv));
            let sampled = Color::new(albedo_map[0], albedo_map[1], albedo_map[2], 1.0).to_linear();
            let base = [
                sampled.r * factor[0],
                sampled.g * factor[1],
                sampled.b * factor[2],
            ];
            let mr = maps
                .and_then(|m| m.metallic_roughness.as_ref())
                .map_or([1.0; 4], |m| m.sample(uv));
            let roughness = (values.roughness * mr[1]).max(0.045);
            let metallic = values.metallic * mr[2];
            let ao = values.ao
                * maps
                    .and_then(|m| m.occlusion.as_ref())
                    .map_or(1.0, |m| m.sample(uv)[0]);
            let emission = maps
                .and_then(|m| m.emission.as_ref())
                .map_or([0.0; 4], |m| m.sample(uv));
            let normal = maps
                .and_then(|m| m.normal.as_ref())
                .map_or(sphere_normal, |m| {
                    let sample = m.sample(uv);
                    let n = [
                        sample[0] * 2.0 - 1.0,
                        sample[1] * 2.0 - 1.0,
                        sample[2] * 2.0 - 1.0,
                    ];
                    let tangent = normalize([z, 0.0, -nx]);
                    let bitangent = [
                        -sphere_normal[1] * tangent[2],
                        sphere_normal[2] * tangent[0] - sphere_normal[0] * tangent[2],
                        sphere_normal[1] * tangent[0],
                    ];
                    let mapped = std::array::from_fn(|i| {
                        tangent[i] * n[0] + bitangent[i] * n[1] + sphere_normal[i] * n[2]
                    });
                    if dot(mapped, mapped) > 0.000001 {
                        normalize(mapped)
                    } else {
                        sphere_normal
                    }
                });
            let z = normal[2].max(0.001);
            let alpha2 = roughness.powi(4);
            let k = (roughness + 1.0).powi(2) / 8.0;
            let nl = dot(normal, light).max(0.0);
            let nh = dot(normal, half).max(0.0);
            let vh = half[2];
            let d = alpha2 / (std::f32::consts::PI * (nh * nh * (alpha2 - 1.0) + 1.0).powi(2));
            let g = (z / (z * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
            let reflected = [2.0 * normal[0] * z, 2.0 * normal[1] * z, 2.0 * z * z - 1.0];
            // Broad rectangular softboxes give rough and polished surfaces
            // distinct reflections without adding fictitious surface textures.
            let blur = 0.09 + roughness * 0.55;
            let studio = 0.08
                + 1.7
                    * (-(reflected[0] + 0.45).powi(2) / (blur * blur)
                        - (reflected[1] - 0.5).powi(4) / 0.7)
                        .exp()
                + 0.7
                    * (-(reflected[0] - 0.65).powi(2) / (blur * blur) - reflected[1].powi(4) / 0.9)
                        .exp();
            let rgb = std::array::from_fn::<_, 3, _>(|channel| {
                let f0 = 0.04 * (1.0 - metallic) + base[channel] * metallic;
                let f = f0 + (1.0 - f0) * (1.0 - vh).powi(5);
                let specular = d * g * f / (4.0 * z * nl).max(0.001);
                let diffuse = base[channel] * (1.0 - metallic) * (0.18 * ao + 0.85 * nl);
                let reflected = studio * (f0 + (1.0 - f0) * (1.0 - z).powi(5)) * ao;
                1.0 - (-(diffuse + 1.5 * specular * nl + reflected + emission[channel]) * 1.4).exp()
            });
            let srgb = Color::new(rgb[0], rgb[1], rgb[2], 1.0).to_srgb();
            let coverage = ((1.0 - radius2.sqrt()) * size as f32 / 2.3 + 0.5).clamp(0.0, 1.0);
            let index = ((y * size + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&[
                (srgb.r * 255.0).round() as u8,
                (srgb.g * 255.0).round() as u8,
                (srgb.b * 255.0).round() as u8,
                (coverage * values.base_color[3] * albedo_map[3] * 255.0).round() as u8,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_imported_maps_change_surface_color_relief_and_alpha() {
        use super::super::preview_maps::PreviewMap;
        let map = |pixel: [u8; 4]| {
            PreviewMap::from_gltf(&gltf::image::Data {
                width: 1,
                height: 1,
                format: gltf::image::Format::R8G8B8A8,
                pixels: pixel.to_vec(),
            })
        };
        let values = MaterialPreset::Ceramic.values();
        let factors = render_sphere(values, SIZE, None);
        let maps = PreviewMaps {
            albedo: map([255, 0, 0, 128]),
            ..Default::default()
        };
        let textured = render_sphere(values, SIZE, Some(&maps));
        let center = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        assert!(textured[center] > textured[center + 1] + 50);
        assert_eq!(textured[center + 3], 128);
        for maps in [
            PreviewMaps {
                normal: map([200, 128, 220, 255]),
                ..Default::default()
            },
            PreviewMaps {
                metallic_roughness: map([255, 40, 255, 255]),
                ..Default::default()
            },
            PreviewMaps {
                occlusion: map([0, 0, 0, 255]),
                ..Default::default()
            },
            PreviewMaps {
                emission: map([0, 255, 0, 255]),
                ..Default::default()
            },
        ] {
            assert_ne!(factors, render_sphere(values, SIZE, Some(&maps)));
        }
    }

    #[test]
    fn test_preview_tracks_authored_factors_and_transparency() {
        let values = MaterialPreset::Ceramic.values();
        let glossy = render_sphere(values, SIZE, None);
        let matte = render_sphere(
            MaterialValues {
                roughness: 1.0,
                ..values
            },
            SIZE,
            None,
        );
        let metal = render_sphere(
            MaterialValues {
                metallic: 1.0,
                ..values
            },
            SIZE,
            None,
        );
        let red = render_sphere(
            MaterialValues {
                base_color: [1.0, 0.0, 0.0, 1.0],
                ..values
            },
            SIZE,
            None,
        );
        assert_ne!(glossy, matte);
        assert_ne!(glossy, metal);
        let center = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        assert!(red[center] > red[center + 1] + 50);
        assert_eq!(glossy[3], 0);
        assert_eq!(glossy[center + 3], 255);
        let transparent = render_sphere(
            MaterialValues {
                base_color: [1.0, 1.0, 1.0, 0.0],
                ..values
            },
            SIZE,
            None,
        );
        assert!(
            transparent
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 0)
        );
    }
}
