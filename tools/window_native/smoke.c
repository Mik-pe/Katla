#include "bridge.h"
#include <SDL3/SDL.h>
#include <SDL3/SDL_vulkan.h>
#include <assert.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdatomic.h>
static atomic_int allocations;
static void *tracked_malloc(size_t bytes){void *p=malloc(bytes);if(p)atomic_fetch_add(&allocations,1);return p;}
static void *tracked_calloc(size_t count,size_t bytes){void *p=calloc(count,bytes);if(p)atomic_fetch_add(&allocations,1);return p;}
static void tracked_free(void *p){if(p)atomic_fetch_sub(&allocations,1);free(p);}
static void *tracked_realloc(void *p,size_t bytes){if(!bytes){tracked_free(p);return NULL;}if(!p)return tracked_malloc(bytes);return realloc(p,bytes);}
int main(int argc,char **argv) {
    assert(SDL_SetMemoryFunctions(tracked_malloc,tracked_calloc,tracked_realloc,tracked_free));
    assert(argc==2 && katla_window_abi()==1);
    void *window=katla_window_create("Katla portable window acceptance",320,200,argv[1]);
    if(!window){fprintf(stderr,"Native window unavailable: %s\n",katla_window_error());return 77;}
    KatlaWindowState state;assert(katla_window_state(window,&state) && state.width>0 && state.height>0 && state.scale>0 && state.density>0);
    KatlaWindowSurface surface;assert(katla_window_surface(window,&surface) && surface.view && surface.kind>0);
    assert(katla_window_resize(window,400,240));
    for(int frame=0;frame<20;frame++){KatlaWindowEvent event;while(katla_window_poll(window,&event)>0){}SDL_Delay(2);}
    assert(katla_window_state(window,&state) && state.width>=400 && state.height>=240);
    char *previous=SDL_GetClipboardText();assert(previous);
    const char *swedish="Åäö välj dörren 😊";
    assert(katla_window_clipboard_set(window,swedish));assert(strcmp(katla_window_clipboard_get(window),swedish)==0);
    assert(katla_window_clipboard_set(window,previous));SDL_free(previous);
    assert(katla_window_ime(window,1,40,30,120,24));
    void *other=katla_window_create("Katla auxiliary window acceptance",160,120,argv[1]);assert(other);
    int count=0;SDL_Window **native_windows=SDL_GetWindows(&count);assert(native_windows && count==2);
    SDL_WindowID main_id=0,other_id=0;
    for(int i=0;i<count;i++){if(strcmp(SDL_GetWindowTitle(native_windows[i]),"Katla portable window acceptance")==0)main_id=SDL_GetWindowID(native_windows[i]);else other_id=SDL_GetWindowID(native_windows[i]);}
    SDL_free(native_windows);assert(main_id && other_id);
    SDL_Event injected={0};injected.type=SDL_EVENT_TEXT_EDITING;injected.edit.windowID=main_id;injected.edit.text="é😊";injected.edit.start=2;injected.edit.length=1;assert(SDL_PushEvent(&injected));
    injected.type=SDL_EVENT_TEXT_INPUT;injected.text.windowID=other_id;injected.text.text="annan vy";assert(SDL_PushEvent(&injected));
    injected.text.windowID=main_id;injected.text.text=swedish;assert(SDL_PushEvent(&injected));
    int edit=0,commit=0;KatlaWindowEvent event;
    while(katla_window_poll(window,&event)>0){if(event.kind==10){assert(event.start==2 && event.length==1 && strcmp(event.text,"é😊")==0);edit++;}if(event.kind==9){assert(strcmp(event.text,swedish)==0);commit++;}}
    assert(edit==1 && commit==1);
    int other_commits=0;while(katla_window_poll(other,&event)>0){if(event.kind==9){assert(strcmp(event.text,"annan vy")==0);other_commits++;}}
    assert(other_commits==1);katla_window_destroy(other);
    assert(katla_window_state(window,&state));assert(katla_window_ime(window,0,0,0,1,1));katla_window_destroy(window);
    assert(atomic_load(&allocations)==0);
    puts("Native SDL windows create/resize/surface/clipboard/IME-area PASS; independent window lifetime and injected UTF8 queue ownership PASS (not OS IME acceptance).");return 0;
}
