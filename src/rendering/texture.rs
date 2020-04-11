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
        // gl::CreateBuffers(1, &mut self.id);
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
        let (gl_enum, internal_format, stride) = match img.format {
            gltf::image::Format::R8 => (gl::RED, gl::R8, 1),
            gltf::image::Format::R8G8 => (gl::RG, gl::RG8, 2),
            gltf::image::Format::R8G8B8 => (gl::RGB, gl::RGB8, 3),
            gltf::image::Format::R8G8B8A8 => (gl::RGBA, gl::RGBA8, 4),
            gltf::image::Format::B8G8R8 => (gl::BGR, gl::RGB8, 3),
            gltf::image::Format::B8G8R8A8 => (gl::BGRA, gl::RGBA8, 4),
        };
        gl::TextureStorage2D(
            self.id,
            num_mipmaps as i32,
            internal_format,
            self.res_x,
            self.res_y,
        );

        let total_buffer_size = stride * self.res_x * self.res_y;
        let mut buffer = 0;
        gl::CreateBuffers(1, &mut buffer);
        gl::NamedBufferStorage(
            buffer,
            total_buffer_size as isize,
            std::ptr::null(),
            gl::MAP_WRITE_BIT,
        );
        let pixel_size = img.pixels.len() as usize;
        let buf = gl::MapNamedBufferRange(
            buffer,
            0,
            pixel_size as isize,
            gl::MAP_WRITE_BIT | gl::MAP_FLUSH_EXPLICIT_BIT,
        );
        if !buf.is_null() {
            std::ptr::copy(img.pixels.as_ptr(), buf as *mut _, pixel_size);
            gl::FlushMappedNamedBufferRange(buffer, 0, pixel_size as isize);
            gl::UnmapNamedBuffer(buffer);
        }
        gl::TextureBuffer(self.id, gl_enum, buffer);
        gl::BindBuffer(gl::PIXEL_UNPACK_BUFFER, buffer);
        gl::TextureSubImage2D(
            self.id,
            0, // level
            0, // xoffset
            0, // yoffset
            self.res_x,
            self.res_y,
            gl_enum,
            gl::UNSIGNED_BYTE,
            std::ptr::null() as *const _,
        );
        gl::BindBuffer(gl::PIXEL_UNPACK_BUFFER, 0);
        gl::DeleteBuffers(1, &buffer);
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
            println!("Deleted texture!");
            gl::DeleteTextures(1, &self.id);
        }
    }
}
