#define STB_IMAGE_STATIC
#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#define STBI_ONLY_JPEG
#define STBI_NO_STDIO
#define STBI_NO_HDR
#define STBI_NO_LINEAR
#include "stb_image.h"

int katla_image_info_from_memory(const unsigned char *encoded, int length, int *width, int *height, int *channels) {
    return stbi_info_from_memory(encoded, length, width, height, channels);
}

unsigned char *katla_image_load_from_memory(const unsigned char *encoded, int length, int *width, int *height, int *channels, int desired_channels) {
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
