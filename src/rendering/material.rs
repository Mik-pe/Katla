// use crate::rendering::{Texture, TextureUsage};

// pub struct Material {
//     albedo: Option<Texture>,
//     met_rough: Option<Texture>,
//     normal: Option<Texture>,
// }

// impl Material {
//     pub fn new(mat: gltf::material::Material, images: &Vec<gltf::image::Data>) -> Self {
//         let mut albedo = None;
//         let mut met_rough = None;
//         let mut normal = None;
//         if let Some(base_color_tex) = mat.pbr_metallic_roughness().base_color_texture() {
//             let mut tex = Texture::new(TextureUsage::ALBEDO);
//             unsafe {
//                 tex.set_data_gltf(&images[base_color_tex.texture().index()]);
//             }
//             albedo = Some(tex);
//         }
//         if albedo.is_none() {
//             //For now lets cheat some
//             if let Some(spec_gloss) = mat.pbr_specular_glossiness() {
//                 if let Some(base_color_tex) = spec_gloss.diffuse_texture() {
//                     let mut tex = Texture::new(TextureUsage::ALBEDO);
//                     unsafe {
//                         tex.set_data_gltf(&images[base_color_tex.texture().index()]);
//                     }
//                     albedo = Some(tex);
//                 }
//             }
//         }
//         if let Some(met_rough_tex) = mat.pbr_metallic_roughness().metallic_roughness_texture() {
//             let mut tex = Texture::new(TextureUsage::METALLIC_ROUGHNESS);
//             unsafe {
//                 tex.set_data_gltf(&images[met_rough_tex.texture().index()]);
//             }
//             met_rough = Some(tex);
//         }
//         if let Some(normal_tex) = mat.normal_texture() {
//             let mut tex = Texture::new(TextureUsage::NORMAL);
//             unsafe {
//                 tex.set_data_gltf(&images[normal_tex.texture().index()]);
//             }
//             normal = Some(tex);
//         }

//         Self {
//             albedo,
//             met_rough,
//             normal,
//         }
//     }

// pub fn bind(&self) {
//     if let Some(tex) = &self.albedo {
//         unsafe {
//             tex.bind();
//         }
//     }
//     if let Some(tex) = &self.met_rough {
//         unsafe {
//             tex.bind();
//         }
//     }
//     if let Some(tex) = &self.normal {
//         unsafe {
//             tex.bind();
//         }
//     }
// }

// pub fn unbind(&self) {
//     if let Some(tex) = &self.albedo {
//         unsafe {
//             tex.unbind();
//         }
//     }
//     if let Some(tex) = &self.met_rough {
//         unsafe {
//             tex.unbind();
//         }
//     }
//     if let Some(tex) = &self.normal {
//         unsafe {
//             tex.unbind();
//         }
//     }
// }
// }
