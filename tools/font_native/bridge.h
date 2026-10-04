#ifndef KATLA_FONT_NATIVE_H
#define KATLA_FONT_NATIVE_H
#include <stddef.h>
#include <stdint.h>
#if defined(_WIN32)
#define KATLA_FONT_API __declspec(dllexport)
#else
#define KATLA_FONT_API __attribute__((visibility("default")))
#endif
#ifdef __cplusplus
extern "C" {
#endif
typedef struct { uint32_t glyph,cluster; float x,y,advance; uint32_t line,font; } KatlaFontGlyph;
typedef struct { uint32_t byte; float x,y; } KatlaFontCaret;
typedef struct { uint32_t width,height; int32_t pitch,left,top; const uint8_t *pixels; } KatlaFontBitmap;
KATLA_FONT_API uint32_t katla_font_abi(void);
KATLA_FONT_API uint32_t katla_font_grapheme(const uint8_t *text,size_t length,uint32_t byte,int32_t direction);
KATLA_FONT_API void *katla_font_create(const char *regular,const char *icons);
KATLA_FONT_API void katla_font_destroy(void *engine);
KATLA_FONT_API uint32_t katla_font_add_fallback(void *engine,const char *path);
KATLA_FONT_API void *katla_font_shape(void *engine,uint32_t font,const uint8_t *text,size_t length,float size,float wrap);
KATLA_FONT_API void katla_font_layout_destroy(void *layout);
KATLA_FONT_API const KatlaFontGlyph *katla_font_glyphs(void *layout,size_t *count);
KATLA_FONT_API const KatlaFontCaret *katla_font_carets(void *layout,size_t *count);
KATLA_FONT_API void katla_font_dimensions(void *layout,float *width,float *height);
KATLA_FONT_API int katla_font_raster(void *engine,uint32_t font,uint32_t glyph,float physical_size,KatlaFontBitmap *out);
#ifdef __cplusplus
}
#endif
#endif
