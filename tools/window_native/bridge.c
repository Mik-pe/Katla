#include "bridge.h"
#include <SDL3/SDL.h>
#include <SDL3/SDL_vulkan.h>
#include <string.h>
#include <stdlib.h>

typedef struct EventNode { KatlaWindowEvent event; char *text; struct EventNode *next; } EventNode;
typedef struct WindowOwner { SDL_Window *window; EventNode *head,*tail,*current; char *clipboard; uint32_t buttons; int failed; struct WindowOwner *next; } WindowOwner;
static WindowOwner *windows;
static uint32_t mods(SDL_Keymod flags) {
    return ((flags&SDL_KMOD_SHIFT)?1u:0u)|((flags&SDL_KMOD_CTRL)?2u:0u)|((flags&SDL_KMOD_ALT)?4u:0u)|((flags&SDL_KMOD_GUI)?8u:0u);
}
static void free_event(EventNode *event) { if(event){ SDL_free(event->text); SDL_free(event); } }
static void append_event(WindowOwner *owner,const KatlaWindowEvent *event) {
    EventNode *node=SDL_calloc(1,sizeof(*node)); if(!node){owner->failed=1;return;}
    node->event=*event;
    if(event->text){node->text=SDL_strdup(event->text);if(!node->text){free_event(node);owner->failed=1;return;}node->event.text=node->text;}
    if(owner->tail)owner->tail->next=node;else owner->head=node;
    owner->tail=node;
}
static void pump(void) {
    SDL_Event event;
    while(SDL_PollEvent(&event)) {
        SDL_WindowID id=0; KatlaWindowEvent value={0};
        switch(event.type) {
        case SDL_EVENT_QUIT: value.kind=1;break;
        case SDL_EVENT_WINDOW_CLOSE_REQUESTED: id=event.window.windowID;value.kind=1;break;
        case SDL_EVENT_WINDOW_FOCUS_GAINED: case SDL_EVENT_WINDOW_FOCUS_LOST:
            id=event.window.windowID;value.kind=2;value.code=event.type==SDL_EVENT_WINDOW_FOCUS_GAINED;break;
        case SDL_EVENT_KEY_DOWN: case SDL_EVENT_KEY_UP:
            id=event.key.windowID;value.kind=event.type==SDL_EVENT_KEY_DOWN?3:4;value.code=event.key.scancode;value.modifiers=mods(event.key.mod);value.repeat=event.key.repeat;break;
        case SDL_EVENT_MOUSE_MOTION:
            id=event.motion.windowID;value.kind=5;value.x=event.motion.x;value.y=event.motion.y;value.dx=event.motion.xrel;value.dy=event.motion.yrel;break;
        case SDL_EVENT_MOUSE_BUTTON_DOWN: case SDL_EVENT_MOUSE_BUTTON_UP:
            id=event.button.windowID;value.kind=event.type==SDL_EVENT_MOUSE_BUTTON_DOWN?6:7;value.x=event.button.x;value.y=event.button.y;value.button=event.button.button;value.clicks=event.button.clicks;value.modifiers=mods(SDL_GetModState());break;
        case SDL_EVENT_MOUSE_WHEEL:
            id=event.wheel.windowID;value.kind=8;value.x=event.wheel.mouse_x;value.y=event.wheel.mouse_y;value.dx=event.wheel.x;value.dy=event.wheel.y;
            if(event.wheel.direction==SDL_MOUSEWHEEL_FLIPPED){value.dx=-value.dx;value.dy=-value.dy;}break;
        case SDL_EVENT_TEXT_INPUT:
            id=event.text.windowID;value.kind=9;value.text=event.text.text;break;
        case SDL_EVENT_TEXT_EDITING:
            id=event.edit.windowID;value.kind=10;value.text=event.edit.text;value.start=event.edit.start;value.length=event.edit.length;break;
        default:continue;
        }
        for(WindowOwner *owner=windows;owner;owner=owner->next) {
            if(id && SDL_GetWindowID(owner->window)!=id)continue;
            if(value.kind==6 && value.button<=31){owner->buttons|=1u<<value.button;SDL_CaptureMouse(true);}
            if(value.kind==7 && value.button<=31){owner->buttons&=~(1u<<value.button);if(!owner->buttons)SDL_CaptureMouse(false);}
            if(value.kind==2 && !value.code){owner->buttons=0;SDL_CaptureMouse(false);}
            append_event(owner,&value);
        }
    }
}
uint32_t katla_window_abi(void) { return SDL_GetVersion()==SDL_VERSION?1u:0u; }
void *katla_window_create(const char *title,uint32_t width,uint32_t height,const char *loader) {
    if(!title || !width || !height || width>16384 || height>16384 || !SDL_IsMainThread())return NULL;
    if(!SDL_InitSubSystem(SDL_INIT_VIDEO))return NULL;
    if(!SDL_Vulkan_LoadLibrary(loader&&*loader?loader:NULL)){SDL_QuitSubSystem(SDL_INIT_VIDEO);return NULL;}
    WindowOwner *owner=SDL_calloc(1,sizeof(*owner)); if(!owner){SDL_Vulkan_UnloadLibrary();SDL_QuitSubSystem(SDL_INIT_VIDEO);return NULL;}
    float content_scale=SDL_GetDisplayContentScale(SDL_GetPrimaryDisplay());if(content_scale<=0)content_scale=1;
    owner->window=SDL_CreateWindow(title,(int)((float)width*content_scale),(int)((float)height*content_scale),SDL_WINDOW_RESIZABLE|SDL_WINDOW_HIGH_PIXEL_DENSITY|SDL_WINDOW_VULKAN);
    if(!owner->window){SDL_free(owner);SDL_Vulkan_UnloadLibrary();SDL_QuitSubSystem(SDL_INIT_VIDEO);return NULL;}
    owner->next=windows;windows=owner;return owner;
}
void katla_window_destroy(void *value) {
    WindowOwner *owner=value;if(!owner)return;
    WindowOwner **cursor=&windows;while(*cursor && *cursor!=owner)cursor=&(*cursor)->next;if(*cursor)*cursor=owner->next;
    SDL_StopTextInput(owner->window);SDL_DestroyWindow(owner->window);
    while(owner->head){EventNode *event=owner->head;owner->head=event->next;free_event(event);}free_event(owner->current);SDL_free(owner->clipboard);SDL_free(owner);
    SDL_Vulkan_UnloadLibrary();SDL_QuitSubSystem(SDL_INIT_VIDEO);if(!windows)SDL_Quit();
}
int32_t katla_window_state(void *value,KatlaWindowState *state) {
    WindowOwner *owner=value;if(!owner || !state)return 0;int width,height;
    if(!SDL_GetWindowSizeInPixels(owner->window,&width,&height))return 0;
    SDL_WindowFlags flags=SDL_GetWindowFlags(owner->window);
    *state=(KatlaWindowState){(uint32_t)(width>0?width:0),(uint32_t)(height>0?height:0),SDL_GetWindowDisplayScale(owner->window),SDL_GetWindowPixelDensity(owner->window),!(flags&(SDL_WINDOW_HIDDEN|SDL_WINDOW_MINIMIZED)),SDL_GetKeyboardFocus()==owner->window,0};return 1;
}
int32_t katla_window_poll(void *value,KatlaWindowEvent *event) {
    WindowOwner *owner=value;if(!owner || !event)return -1;
    free_event(owner->current);owner->current=NULL;pump();if(owner->failed)return -1;
    if(!owner->head)return 0;owner->current=owner->head;owner->head=owner->head->next;if(!owner->head)owner->tail=NULL;
    *event=owner->current->event;return 1;
}
int32_t katla_window_resize(void *value,uint32_t width,uint32_t height) {
    WindowOwner *owner=value;if(!owner || !width || !height || width>16384 || height>16384)return 0;
    float density=SDL_GetWindowPixelDensity(owner->window),scale=SDL_GetWindowDisplayScale(owner->window);if(density<=0 || scale<=0)return 0;
    return SDL_SetWindowSize(owner->window,(int)((float)width*scale/density),(int)((float)height*scale/density));
}
int32_t katla_window_surface(void *value,KatlaWindowSurface *surface) {
    WindowOwner *owner=value;if(!owner || !surface)return 0;SDL_PropertiesID props=SDL_GetWindowProperties(owner->window);*surface=(KatlaWindowSurface){0};
    surface->view=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_WIN32_HWND_POINTER,NULL);
    if(surface->view){surface->kind=4;surface->display=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_WIN32_INSTANCE_POINTER,NULL);return surface->display!=NULL;}
    surface->view=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER,NULL);
    if(surface->view){surface->kind=3;surface->display=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER,NULL);return surface->display!=NULL;}
    uint64_t xwindow=(uint64_t)SDL_GetNumberProperty(props,SDL_PROP_WINDOW_X11_WINDOW_NUMBER,0);
    if(xwindow){surface->kind=2;surface->view=(void*)(uintptr_t)xwindow;surface->display=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_X11_DISPLAY_POINTER,NULL);return surface->display!=NULL;}
    surface->view=SDL_GetPointerProperty(props,SDL_PROP_WINDOW_COCOA_WINDOW_POINTER,NULL);if(surface->view){surface->kind=1;return 1;}return 0;
}
int32_t katla_window_ime(void *value,uint32_t active,int32_t x,int32_t y,int32_t width,int32_t height) {
    WindowOwner *owner=value;if(!owner)return 0;
    if(!active)return !SDL_TextInputActive(owner->window)||SDL_StopTextInput(owner->window);
    SDL_Rect rect={x,y,width>0?width:1,height>0?height:1};
    if(!SDL_SetTextInputArea(owner->window,&rect,0))return 0;
    return SDL_TextInputActive(owner->window)||SDL_StartTextInput(owner->window);
}
const char *katla_window_clipboard_get(void *value) {
    WindowOwner *owner=value;if(!owner)return NULL;char *next=SDL_GetClipboardText();if(!next)return NULL;SDL_free(owner->clipboard);owner->clipboard=next;return next;
}
int32_t katla_window_clipboard_set(void *value,const char *text) { return value && text && SDL_SetClipboardText(text); }
const char *katla_window_error(void) { return SDL_GetError(); }
