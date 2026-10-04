//! Private PNG/JPEG/BMP/TIFF decoder ABI from repository-pinned source; no installed vendor binaries are required.
package katla_image

import "core:c"

@(private)
LIB :: #config(STB_IMAGE_LIBRARY,"../../../target/odin-stb-image/libkatla_image.a")
when ODIN_OS==.Windows {
    foreign import image { LIB }
} else {
    foreign import image { LIB,"system:z" }
}

@(default_calling_convention="c",link_prefix="katla_image_")
foreign image {
    info_from_memory :: proc(encoded:[^]byte,length:c.int,width,height,channels:^c.int)->c.int ---
    load_from_memory :: proc(encoded:[^]byte,length:c.int,width,height,channels:^c.int,desired_channels:c.int)->[^]byte ---
    @(link_name="katla_image_free")
    image_free :: proc(pixels:rawptr) ---
    set_flip_vertically_on_load_thread :: proc(flip:b32) ---
    zlib_decode_buffer :: proc(output:[^]byte,output_length:c.int,encoded:[^]byte,encoded_length:c.int)->c.int ---
}
