//! Bounded CPU copies of the PBR maps already decoded for model import.
use std::sync::Arc;

const LIMIT: u32 = 192;

#[derive(Default)]
pub(crate) struct PreviewMaps {
    pub(crate) albedo: Option<PreviewMap>,
    pub(crate) normal: Option<PreviewMap>,
    pub(crate) metallic_roughness: Option<PreviewMap>,
    pub(crate) occlusion: Option<PreviewMap>,
    pub(crate) emission: Option<PreviewMap>,
}
impl PreviewMaps {
    pub(crate) fn shared(self) -> Option<Arc<Self>> {
        (self.albedo.is_some()
            || self.normal.is_some()
            || self.metallic_roughness.is_some()
            || self.occlusion.is_some()
            || self.emission.is_some())
        .then(|| Arc::new(self))
    }
}

pub(crate) struct PreviewMap {
    image: image::RgbaImage,
}
impl PreviewMap {
    pub(crate) fn from_gltf(data: &gltf::image::Data) -> Option<Self> {
        let pixels = match data.format {
            gltf::image::Format::R8G8B8A8 => data.pixels.clone(),
            gltf::image::Format::R8G8B8 => data
                .pixels
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            _ => return None,
        };
        let image = image::RgbaImage::from_raw(data.width, data.height, pixels)?;
        let width = data.width.clamp(1, LIMIT);
        let height = data.height.clamp(1, LIMIT);
        Some(Self {
            image: image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            ),
        })
    }

    pub(crate) fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        let width = self.image.width() as i32;
        let height = self.image.height() as i32;
        let x = uv[0].rem_euclid(1.0) * width as f32 - 0.5;
        let y = uv[1].rem_euclid(1.0) * height as f32 - 0.5;
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let at = |x: i32, y: i32| {
            self.image
                .get_pixel(x.rem_euclid(width) as u32, y.rem_euclid(height) as u32)
                .0
                .map(|v| v as f32 / 255.0)
        };
        let a = at(x0, y0);
        let b = at(x0 + 1, y0);
        let c = at(x0, y0 + 1);
        let d = at(x0 + 1, y0 + 1);
        let tx = x.fract().rem_euclid(1.0);
        let ty = y.fract().rem_euclid(1.0);
        std::array::from_fn(|i| {
            (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty
        })
    }
}
