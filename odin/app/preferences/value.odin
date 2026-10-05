//! Application preferences own persistent strings and retain Rust editor defaults and bounds.
package preferences
import "core:mem"
import "core:math"
import "core:strings"

Editor :: struct { snap_to_grid:bool,camera_speed,grid_size:f32 }
Audio :: struct { master_volume,sfx_volume,music_volume,ambient_volume:f32 }
External_Chat :: struct { socket,thread_id:string }
Value :: struct {
    theme:string,
    show_grid,show_stats,show_physics_debug,show_reverb_debug:bool,
    font_scale:f32,
    editor:Editor,
    audio:Audio,
    external_chat:External_Chat,
    allocator:mem.Allocator,
}
Error :: enum { None,Invalid_TOML,Invalid_Value,IO,Invalid_Path }
/// Returns owned default settings, independent of a scene or renderer.
defaults :: proc(allocator:=context.allocator)->Value {
    return {theme=strings.clone("rcp",allocator),show_grid=true,show_stats=false,font_scale=1,editor={true,50,1},audio={1,1,1,1},allocator=allocator}
}
/// Releases every preference-owned string.
destroy :: proc(value:^Value) {
    delete(value.theme,value.allocator); delete(value.external_chat.socket,value.allocator); delete(value.external_chat.thread_id,value.allocator); value^={}
}
/// Theme and connection edits retain storage independently of transient UI text buffers.
set_theme :: proc(value:^Value,text:string) { next:=strings.clone(text,value.allocator); delete(value.theme,value.allocator); value.theme=next; validate(value) }
set_connection :: proc(value:^Value,socket,thread_id:string) {
    next_socket:=strings.clone(socket,value.allocator); next_thread:=strings.clone(thread_id,value.allocator)
    delete(value.external_chat.socket,value.allocator); delete(value.external_chat.thread_id,value.allocator)
    value.external_chat={next_socket,next_thread}
}
@(private="package")
finite_clamp :: proc(number,default_value,lower,upper:f32)->f32 { if math.is_nan(number)||math.is_inf(number) { return default_value }; return clamp(number,lower,upper) }
/// Matches Rust finite fallbacks and numeric bounds before persistent or live consumption.
validate :: proc(value:^Value) {
    accepted:=false
    for name in ([15]string{"rcp","dark","light","nord","tokyo_night","dracula","gruvbox","one_dark","material_palenight","ayu_dark","github_dark","monokai","rose_pine","kanagawa","solarized_dark"}) { if value.theme==name { accepted=true; break } }
    if !accepted { replacement:=strings.clone("rcp",value.allocator); delete(value.theme,value.allocator); value.theme=replacement }
    value.font_scale=finite_clamp(value.font_scale,1,0.5,3)
    value.editor.camera_speed=finite_clamp(value.editor.camera_speed,50,1,200)
    value.editor.grid_size=finite_clamp(value.editor.grid_size,1,0.01,100)
    value.audio.master_volume=finite_clamp(value.audio.master_volume,1,0,1)
    value.audio.sfx_volume=finite_clamp(value.audio.sfx_volume,1,0,1)
    value.audio.music_volume=finite_clamp(value.audio.music_volume,1,0,1)
    value.audio.ambient_volume=finite_clamp(value.audio.ambient_volume,1,0,1)
}
