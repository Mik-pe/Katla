//! Full TOML syntax is parsed by the pinned format library; application fields are typed here.
package preferences
import native "../../deps/toml_native"
import "core:c"
import "core:strings"
import "core:unicode/utf8"
import "core:fmt"
import "core:encoding/json"

/// Decodes all Rust preference fields, with defaults for omitted keys and strict known-field types.
decode :: proc(bytes:[]byte,allocator:=context.allocator)->(Value,Error) {
    if len(bytes)>1024*1024 || !utf8.valid_string(string(bytes)) { return {},.Invalid_TOML }
    tree:=native.parse(raw_data(bytes),c.int(len(bytes))); if tree==nil { return {},.Invalid_TOML }; defer native.destroy(tree)
    value:=defaults(allocator); success:=false; defer { if !success { destroy(&value) } }
    for table in ([3]string{"audio","editor","external_chat"}) {
        key:=strings.clone_to_cstring(table,allocator); defer delete(key,allocator)
        text:cstring; length:c.int; number:f64
        kind:=native.find(tree,key,&text,&length,&number); if kind!=.Missing && kind!=.Table { return {},.Invalid_Value }
    }
    for field in ([15]string{"theme","show_grid","show_stats","show_physics_debug","show_reverb_debug","font_scale","editor.snap_to_grid","editor.camera_speed","editor.grid_size","audio.master_volume","audio.sfx_volume","audio.music_volume","audio.ambient_volume","external_chat.socket","external_chat.thread_id"}) {
        key:=strings.clone_to_cstring(field,allocator); defer delete(key,allocator)
        text:cstring; length:c.int; number:f64
        kind:=native.find(tree,key,&text,&length,&number); if kind==.Missing { continue }
        switch field {
        case "theme","external_chat.socket","external_chat.thread_id":
            if kind!=.String || length<0 { return {},.Invalid_Value }
            owned:=strings.clone(string((cast([^]byte)text)[:int(length)]),allocator)
            switch field {
            case "theme": delete(value.theme,allocator); value.theme=owned
            case "external_chat.socket": value.external_chat.socket=owned
            case "external_chat.thread_id": value.external_chat.thread_id=owned
            }
        case "show_grid","show_stats","show_physics_debug","show_reverb_debug","editor.snap_to_grid":
            if kind!=.Boolean { return {},.Invalid_Value }
            setting:=number!=0
            switch field {
            case "show_grid": value.show_grid=setting
            case "show_stats": value.show_stats=setting
            case "show_physics_debug": value.show_physics_debug=setting
            case "show_reverb_debug": value.show_reverb_debug=setting
            case "editor.snap_to_grid": value.editor.snap_to_grid=setting
            }
        case:
            if kind!=.Number { return {},.Invalid_Value }
            setting:=f32(number)
            switch field {
            case "font_scale": value.font_scale=setting
            case "editor.camera_speed": value.editor.camera_speed=setting
            case "editor.grid_size": value.editor.grid_size=setting
            case "audio.master_volume": value.audio.master_volume=setting
            case "audio.sfx_volume": value.audio.sfx_volume=setting
            case "audio.music_volume": value.audio.music_volume=setting
            case "audio.ambient_volume": value.audio.ambient_volume=setting
            }
        }
    }
    validate(&value); success=true; return value,.None
}

/// Produces interoperable TOML with escaped owned UTF-8 connection strings.
encode :: proc(value:^Value,allocator:=context.allocator)->([]byte,Error) {
    context.allocator=allocator
    escaped:[3][]byte; defer { for bytes in escaped { delete(bytes,allocator) } }
    for text,i in ([3]string{value.theme,value.external_chat.socket,value.external_chat.thread_id}) {
        if !utf8.valid_string(text) { return nil,.Invalid_Value }
        bytes,error:=json.marshal(text,allocator=allocator); if error!=nil { return nil,.Invalid_Value }; escaped[i]=bytes
    }
    // JSON's escaped strings are TOML basic strings for validated UTF-8 text.
    output:=fmt.aprintf("theme = %s\nshow_grid = %t\nshow_stats = %t\nshow_physics_debug = %t\nshow_reverb_debug = %t\nfont_scale = %.9g\n\n[editor]\nsnap_to_grid = %t\ncamera_speed = %.9g\ngrid_size = %.9g\n\n[audio]\nmaster_volume = %.9g\nsfx_volume = %.9g\nmusic_volume = %.9g\nambient_volume = %.9g\n\n[external_chat]\nsocket = %s\nthread_id = %s\n",string(escaped[0]),value.show_grid,value.show_stats,value.show_physics_debug,value.show_reverb_debug,value.font_scale,value.editor.snap_to_grid,value.editor.camera_speed,value.editor.grid_size,value.audio.master_volume,value.audio.sfx_volume,value.audio.music_volume,value.audio.ambient_volume,string(escaped[1]),string(escaped[2]))
    return transmute([]byte)output,.None
}
