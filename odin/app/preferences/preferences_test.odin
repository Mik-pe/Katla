#+test
#+build darwin, linux
package preferences
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"

@(test)
test_preferences_rust_toml_syntax_defaults_bounds_and_invalid_types :: proc(t:^testing.T) {
    wire:string=`theme = "nord"
show_grid = false
show_stats = true
font_scale = inf
editor = {snap_to_grid = false, camera_speed = 999, grid_size = -inf}
[audio]
master_volume = -1
sfx_volume = nan
music_volume = 0.25
ambient_volume = 20
[external_chat]
"socket" = '/tmp/existing host.sock'
thread_id = """existing
conversation"""
[unknown]
arr = [1, "two", { nested = true }]
date = 2026-10-04T10:00:00Z
`
    value,error:=decode(transmute([]byte)wire); defer destroy(&value)
    testing.expect_value(t,error,Error.None)
    testing.expect(t,value.theme=="nord" && !value.show_grid && value.show_stats && !value.editor.snap_to_grid)
    testing.expect(t,value.font_scale==1 && value.editor.camera_speed==200 && value.editor.grid_size==1)
    testing.expect(t,value.audio.master_volume==0 && value.audio.sfx_volume==1 && value.audio.music_volume==0.25 && value.audio.ambient_volume==1)
    testing.expect(t,value.external_chat.socket=="/tmp/existing host.sock" && value.external_chat.thread_id=="existing\nconversation")
    encoded,encode_error:=encode(&value); defer delete(encoded); testing.expect_value(t,encode_error,Error.None)
    restored,restore_error:=decode(encoded); defer destroy(&restored); testing.expect_value(t,restore_error,Error.None)
    testing.expect(t,restored.editor==value.editor && restored.audio==value.audio && restored.external_chat.socket==value.external_chat.socket && restored.external_chat.thread_id==value.external_chat.thread_id)
    for invalid in ([5]string{`show_grid = "false"`,`theme = {name="dark"}`,`editor = 10`,`font_scale = 1
font_scale = 2`,`theme = "unterminated`}) {
        rejected,reject_error:=decode(transmute([]byte)invalid); defer destroy(&rejected); testing.expect(t,reject_error!=.None)
    }
    unknown:string=`theme = "missing"`; fallback,fallback_error:=decode(transmute([]byte)unknown); defer destroy(&fallback); testing.expect_value(t,fallback_error,Error.None); testing.expect_value(t,fallback.theme,"rcp")
}

@(test)
test_preferences_actual_atomic_file_and_invalid_dock_preserves_previous_layout :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-prefs-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    store:Store; testing.expect_value(t,store_init(&store,directory),Error.None); defer store_destroy(&store)
    defaults_value,missing_error:=load(&store); defer destroy(&defaults_value); testing.expect_value(t,missing_error,Error.IO); testing.expect_value(t,defaults_value.theme,"rcp")
    value:=defaults(); defer destroy(&value); set_theme(&value,"dracula"); set_connection(&value,"/tmp/åäö \"quoted\" \\socket.sock","old\nthread\tID")
    value.editor={false,75,0.5}; value.audio={0.8,0.6,0.5,0.7}
    published,save_error:=save(&store,&value); testing.expect(t,published && save_error==.None)
    loaded,load_error:=load(&store); defer destroy(&loaded); testing.expect_value(t,load_error,Error.None)
    testing.expect(t,loaded.theme==value.theme && loaded.editor==value.editor && loaded.audio==value.audio && loaded.external_chat.socket==value.external_chat.socket && loaded.external_chat.thread_id==value.external_chat.thread_id)
    validator :: proc(_:rawptr,bytes:[]byte)->bool {
        tree,error:=json.parse(bytes,spec=.JSON); if error!=nil { return false }; defer json.destroy_value(tree)
        object,ok:=tree.(json.Object); if !ok { return false }; name,is_name:=object["type"].(string); return is_name && name=="Empty"
    }
    layout:string=`{"type":"Empty"}`
    dock_published,dock_error:=dock_save(&store,transmute([]byte)layout,nil,validator); testing.expect(t,dock_published && dock_error==.None)
    bad:string=`{"type":"bogus"}`; rejected,reject_error:=dock_save(&store,transmute([]byte)bad,nil,validator); testing.expect(t,!rejected && reject_error==.Invalid_Value)
    actual,read_error:=dock_load(&store); defer delete(actual); testing.expect_value(t,read_error,Error.None); testing.expect_value(t,string(actual),layout)
    path:=strings.concatenate({directory,"/preferences.toml"}); defer delete(path); testing.expect(t,os.remove(path)==nil); testing.expect(t,os.make_directory(path)==nil)
    failed,failed_error:=save(&store,&value); testing.expect(t,!failed && failed_error==.IO)
}
