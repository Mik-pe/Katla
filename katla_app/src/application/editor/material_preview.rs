//! Studio-lit PBR factor thumbnails shared by the material palette and inspector.

use crate::ui::UIRenderer;
use katla_agent::material::{MaterialPreset, MaterialValues};
use katla_gfx::{AnyRenderer, GpuRenderer, RendererError, TextureDescriptor, TextureHandle};
use katla_math::Color;
use katla_ui::TextureId;

const SIZE: u32 = 192;

#[derive(Default)]
pub(crate) struct MaterialPreviews {
    presets: [Option<TextureHandle>; 6],
    current: Option<(MaterialValues, TextureHandle)>,
}

impl MaterialPreviews {
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut AnyRenderer,
        ui: &mut UIRenderer,
        values: Option<MaterialValues>,
    ) -> Result<(), RendererError> {
        for (index, preset) in MaterialPreset::ALL.iter().enumerate() {
            if self.presets[index].is_none() {
                self.presets[index] = Some(upload(gfx, ui, preset.values())?);
            }
        }
        if let Some(values) = values
            && self.current.as_ref().is_none_or(|(old, _)| *old != values)
        {
            values.validate().map_err(RendererError::InvalidOperation)?;
            let texture = if let Some((_, texture)) = self.current {
                gfx.update_texture(texture, &render_sphere(values, SIZE))?;
                texture
            } else {
                upload(gfx, ui, values)?
            };
            self.current = Some((values, texture));
        }
        Ok(())
    }

    pub(crate) fn presets(&self) -> [Option<TextureId>; 6] {
        self.presets.map(|handle| handle.map(texture_id))
    }

    pub(crate) fn current(&self) -> Option<TextureId> {
        self.current.map(|(_, handle)| texture_id(handle))
    }
}

fn texture_id(handle: TextureHandle) -> TextureId {
    TextureId::from_handle(handle.index(), handle.generation())
}

fn upload(
    gfx: &mut AnyRenderer,
    ui: &mut UIRenderer,
    values: MaterialValues,
) -> Result<TextureHandle, RendererError> {
    values.validate().map_err(RendererError::InvalidOperation)?;
    let pixels = render_sphere(values, SIZE);
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
/// This previews factors; imported mesh textures remain attached to the actual object.
fn render_sphere(values: MaterialValues, size: u32) -> Vec<u8> {
    let albedo = Color::new(
        values.base_color[0],
        values.base_color[1],
        values.base_color[2],
        1.0,
    )
    .to_linear();
    let base = [albedo.r, albedo.g, albedo.b];
    let roughness = values.roughness.max(0.045);
    let alpha2 = roughness.powi(4);
    let light = normalize([-0.55, 0.75, 1.0]);
    let half = normalize([light[0], light[1], light[2] + 1.0]);
    let k = (roughness + 1.0).powi(2) / 8.0;
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
            let normal = normalize([nx, ny, z]);
            let nl = dot(normal, light).max(0.0);
            let nh = dot(normal, half).max(0.0);
            let vh = half[2];
            let d = alpha2 / (std::f32::consts::PI * (nh * nh * (alpha2 - 1.0) + 1.0).powi(2));
            let g = (z / (z * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
            let reflected = [2.0 * nx * z, 2.0 * ny * z, 2.0 * z * z - 1.0];
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
                let f0 = 0.04 * (1.0 - values.metallic) + base[channel] * values.metallic;
                let f = f0 + (1.0 - f0) * (1.0 - vh).powi(5);
                let specular = d * g * f / (4.0 * z * nl).max(0.001);
                let diffuse =
                    base[channel] * (1.0 - values.metallic) * (0.18 * values.ao + 0.85 * nl);
                let reflected = studio * (f0 + (1.0 - f0) * (1.0 - z).powi(5)) * values.ao;
                1.0 - (-(diffuse + 1.5 * specular * nl + reflected) * 1.4).exp()
            });
            let srgb = Color::new(rgb[0], rgb[1], rgb[2], 1.0).to_srgb();
            let coverage = ((1.0 - radius2.sqrt()) * size as f32 / 2.3 + 0.5).clamp(0.0, 1.0);
            let index = ((y * size + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&[
                (srgb.r * 255.0).round() as u8,
                (srgb.g * 255.0).round() as u8,
                (srgb.b * 255.0).round() as u8,
                (coverage * values.base_color[3] * 255.0).round() as u8,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_preview_tracks_authored_factors_and_transparency() {
        let values = MaterialPreset::Ceramic.values();
        let glossy = render_sphere(values, SIZE);
        let matte = render_sphere(
            MaterialValues {
                roughness: 1.0,
                ..values
            },
            SIZE,
        );
        let metal = render_sphere(
            MaterialValues {
                metallic: 1.0,
                ..values
            },
            SIZE,
        );
        let red = render_sphere(
            MaterialValues {
                base_color: [1.0, 0.0, 0.0, 1.0],
                ..values
            },
            SIZE,
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
