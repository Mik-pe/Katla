#ifndef KATLA_WINDOW_NATIVE_H
#define KATLA_WINDOW_NATIVE_H
#include <stdint.h>
#if defined(_WIN32)
#define KATLA_API __declspec(dllexport)
#else
#define KATLA_API __attribute__((visibility("default")))
#endif
#ifdef __cplusplus
extern "C" {
#endif
typedef struct { uint32_t width,height; float scale,density; uint32_t visible,focused,closed; } KatlaWindowState;
typedef struct { uint32_t kind,code,modifiers,button,clicks,repeat; float x,y,dx,dy; int32_t start,length; const char *text; } KatlaWindowEvent;
typedef struct { uint32_t kind; void *view,*display; } KatlaWindowSurface;
KATLA_API uint32_t katla_window_abi(void);
KATLA_API void *katla_window_create(const char*,uint32_t,uint32_t,const char*);
KATLA_API void katla_window_destroy(void*);
KATLA_API int32_t katla_window_state(void*,KatlaWindowState*);
KATLA_API int32_t katla_window_poll(void*,KatlaWindowEvent*);
KATLA_API int32_t katla_window_resize(void*,uint32_t,uint32_t);
KATLA_API int32_t katla_window_surface(void*,KatlaWindowSurface*);
KATLA_API int32_t katla_window_ime(void*,uint32_t,int32_t,int32_t,int32_t,int32_t);
KATLA_API const char *katla_window_clipboard_get(void*);
KATLA_API int32_t katla_window_clipboard_set(void*,const char*);
KATLA_API const char *katla_window_error(void);
#ifdef __cplusplus
}
#endif
#endif
