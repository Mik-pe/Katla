#define STB_IMAGE_STATIC
#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#define STBI_ONLY_JPEG
#define STBI_ONLY_BMP
#define STBI_NO_STDIO
#define STBI_NO_HDR
#define STBI_NO_LINEAR
#include "stb_image.h"
#include "tiff_memory.c"
#include "tiff_precision.c"

int katla_image_precision_from_memory(const unsigned char *encoded, int length) {
    if (katla_tiff_signature(encoded,length)) return katla_tiff_precision(encoded,length);
    return stbi_is_16_bit_from_memory(encoded,length) ? 2 : 1;
}

unsigned short *katla_image_load_16_from_memory(const unsigned char *encoded,int length,int *width,int *height,int *channels,int desired_channels) {
    if (katla_tiff_signature(encoded,length)) return desired_channels==4 ? (unsigned short*)katla_tiff_decode_precise(encoded,length,width,height,channels,2) : NULL;
    return stbi_load_16_from_memory(encoded,length,width,height,channels,desired_channels);
}

float *katla_image_load_float_from_memory(const unsigned char *encoded,int length,int *width,int *height,int *channels,int desired_channels) {
    if (!katla_tiff_signature(encoded,length) || desired_channels!=4) return NULL;
    return (float*)katla_tiff_decode_precise(encoded,length,width,height,channels,4);
}

int katla_image_info_from_memory(const unsigned char *encoded, int length, int *width, int *height, int *channels) {
    if (katla_tiff_signature(encoded,length)) return katla_tiff_info(encoded,length,width,height,channels);
    return stbi_info_from_memory(encoded, length, width, height, channels);
}

unsigned char *katla_image_load_from_memory(const unsigned char *encoded, int length, int *width, int *height, int *channels, int desired_channels) {
    if (katla_tiff_signature(encoded,length)) return desired_channels==4 ? katla_tiff_decode(encoded,length,width,height,channels) : NULL;
    return stbi_load_from_memory(encoded, length, width, height, channels, desired_channels);
}

void katla_image_free(void *pixels) {
    stbi_image_free(pixels);
}

void katla_image_set_flip_vertically_on_load_thread(int flip) {
    stbi_set_flip_vertically_on_load_thread(flip);
}

int katla_image_zlib_decode_buffer(char *output, int output_length, const char *encoded, int encoded_length) {
    return stbi_zlib_decode_buffer(output, output_length, encoded, encoded_length);
}
