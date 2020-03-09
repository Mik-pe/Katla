use crate::gl;
use image::RgbaImage;

pub enum TextureUsage {
    ALBEDO,
    NORMAL,
    METALLIC_ROUGHNESS,
    EMISSION,
}

pub struct Texture {
    id: u32,
    res_x: i32,
    res_y: i32,
    usage: TextureUsage,
}

impl Texture {
    pub fn new(usage: TextureUsage) -> Self {
        Self {
            id: 0,
            res_x: 0,
            res_y: 0,
            usage,
        }
    }

    //TODO: Add error handling here
    //TODO? Have one CPU-setter of data, one GPU-upload of data?
    //TODO: Mipmaps & friends (texture settings)
    pub unsafe fn set_data(&mut self, img: RgbaImage) {
        gl::CreateTextures(gl::TEXTURE_2D, 1, &mut self.id);
        let (res_x, res_y) = img.dimensions();
        self.res_x = res_x as i32;
        self.res_y = res_y as i32;
        gl::TextureStorage2D(self.id, 1, gl::RGBA8, self.res_x, self.res_y);

        gl::TextureSubImage2D(
            self.id,
            0, // level
            0, // xoffset
            0, // yoffset
            1024,
            1024,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            img.into_raw().as_ptr() as *const _,
        );
    }

    pub unsafe fn set_data_gltf(&mut self, img: &gltf::image::Data) {
        gl::CreateTextures(gl::TEXTURE_2D, 1, &mut self.id);
        self.res_x = img.width as i32;
        self.res_y = img.height as i32;
        let max_side = u32::max(img.width, img.height);
        let num_mipmaps = max_side.next_power_of_two().trailing_zeros() + 1;
        let (gl_enum, internal_format) = match img.format {
            gltf::image::Format::R8 => (gl::RED, gl::R8),
            gltf::image::Format::R8G8 => (gl::RG, gl::RG8),
            gltf::image::Format::R8G8B8 => (gl::RGB, gl::RGB8),
            gltf::image::Format::R8G8B8A8 => (gl::RGBA, gl::RGBA8),
            gltf::image::Format::B8G8R8 => (gl::BGR, gl::RGB8),
            gltf::image::Format::B8G8R8A8 => (gl::BGRA, gl::RGBA8),
        };
        gl::TextureStorage2D(
            self.id,
            num_mipmaps as i32,
            internal_format,
            self.res_x,
            self.res_y,
        );

        gl::TextureSubImage2D(
            self.id,
            0, // level
            0, // xoffset
            0, // yoffset
            self.res_x,
            self.res_y,
            gl_enum,
            gl::UNSIGNED_BYTE,
            img.pixels.as_ptr() as *const _,
        );
        gl::GenerateTextureMipmap(self.id);
    }

    pub unsafe fn bind(&self) {
        let texture_unit = match self.usage {
            TextureUsage::ALBEDO => 0,
            TextureUsage::NORMAL => 1,
            TextureUsage::METALLIC_ROUGHNESS => 2,
            TextureUsage::EMISSION => 3,
        };

        gl::BindTextureUnit(texture_unit, self.id);
    }

    pub unsafe fn unbind(&self) {
        let texture_unit = match self.usage {
            TextureUsage::ALBEDO => 0,
            TextureUsage::NORMAL => 1,
            TextureUsage::METALLIC_ROUGHNESS => 2,
            TextureUsage::EMISSION => 3,
        };

        gl::BindTextureUnit(texture_unit, 0);
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id as *const _);
        }
    }
}
