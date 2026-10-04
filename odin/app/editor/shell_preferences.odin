//! Persistent preferences and actual device meters share the application's native services.
package editor_app
import ui "../../ui"
import app ".."
import "core:fmt"

THEME_NAMES := [15]string{"rcp","dark","light","nord","tokyo_night","dracula","gruvbox","one_dark","material_palenight","ayu_dark","github_dark","monokai","rose_pine","kanagawa","solarized_dark"}
@(private="package")
number_control :: proc(shell:^Shell,scope:u64,label:string,value,minimum,maximum,step:f32,payload:u64,action:Action)->ui.Descriptor {
    control_key:=key(scope,label); state:=ui.state(shell.ctx,control_key,0,value)
    if shell.ctx.focused.key!=control_key && shell.ctx.captured.key!=control_key { ui.state_set(shell.ctx,state,value) }
    return {key=control_key,kind=.Slider,text=label,state=state,minimum=minimum,maximum=maximum,step=step,payload=payload,action=u64(action),layout={height=ui.pixels(32),width=ui.percent(1)}}
}
@(private="package")
toggle_control :: proc(shell:^Shell,label:string,value:bool,payload:u64)->ui.Descriptor {
    control_key:=key(60,label); state:=ui.state(shell.ctx,control_key,0,value)
    if shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,state,value) }
    return {key=control_key,kind=.Checkbox,text=label,state=state,payload=payload,action=u64(Action.Pref_Toggle),layout={height=ui.pixels(30),width=ui.percent(1)}}
}
@(private="package")
shell_preferences :: proc(shell:^Shell)->ui.Descriptor {
    items:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(items)
    value:=shell.preferences
    if value==nil { append(&items,text(60,"Preferences are unavailable")) }
    else {
        append(&items,text(60,"Appearance"))
        theme_index:f32=0; for name,i in THEME_NAMES { if name==value.theme { theme_index=f32(i); break } }
        theme_key:=key(60,"theme"); append(&items,ui.Descriptor{key=theme_key,kind=.Combo,text="Theme",state=ui.state(shell.ctx,theme_key,0,theme_index),options=THEME_NAMES[:],action=u64(Action.Pref_Theme),layout={height=ui.pixels(32),width=ui.percent(1)}})
        append(&items,number_control(shell,60,"Font scale",value.font_scale,.5,3,.05,1,.Pref_Number),text(60,"Viewport"))
        append(&items,toggle_control(shell,"Show grid",value.show_grid,1),toggle_control(shell,"Show stats",value.show_stats,2),toggle_control(shell,"Physics debug",value.show_physics_debug,3),toggle_control(shell,"Reverb debug",value.show_reverb_debug,4),toggle_control(shell,"Snap to grid",value.editor.snap_to_grid,5))
        append(&items,number_control(shell,60,"Camera speed",value.editor.camera_speed,1,200,1,2,.Pref_Number),number_control(shell,60,"Grid size",value.editor.grid_size,.01,100,.01,3,.Pref_Number),text(60,"External conversation"))
        for item in ([2]struct{key_name,label,value:string}{{"socket","Socket path",value.external_chat.socket},{"thread","Existing thread ID",value.external_chat.thread_id}}) {
            field_key:=key(60,item.key_name); append(&items,ui.Descriptor{key=field_key,kind=.Text_Input,text=item.label,placeholder=item.label,state=ui.state(shell.ctx,field_key,0,item.value),action=u64(Action.Pref_Connection),layout={height=ui.pixels(32),width=ui.percent(1)}})
        }
        append(&items,button("Save preferences",.Pref_Save))
    }
    return {key=key(60,"panel"),kind=.Scroll_Area,children=nodes(shell,{ui.Descriptor{key=key(60,"content"),kind=.Column,layout={padding={12,12,12,12},gap={0,8},width=ui.percent(1)},children=nodes(shell,items[:])}})}
}
@(private="package")
shell_mixer :: proc(shell:^Shell)->ui.Descriptor {
    items:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(items)
    if shell.audio==nil { append(&items,text(70,"Audio output is unavailable")) }
    else {
        snapshot:=app.audio_service_snapshot(shell.audio)
        device:=fmt.aprintf("%s · %d Hz · %d active voices",snapshot.device.name,snapshot.device.sample_rate,snapshot.active_voices); append(&shell.texts,device); append(&items,text(70,device))
        if snapshot.error!=.None { error_text:=fmt.aprintf("Audio: %v",snapshot.error); append(&shell.texts,error_text); append(&items,text(70,error_text)) }
        if shell.preferences!=nil {
            values:=[4]f32{shell.preferences.audio.master_volume,shell.preferences.audio.sfx_volume,shell.preferences.audio.music_volume,shell.preferences.audio.ambient_volume}
            labels:=[4]string{"Master","SFX","Music","Ambient"}
            levels:=[4]f32{snapshot.levels.master.peak,snapshot.levels.sfx.peak,snapshot.levels.music.peak,snapshot.levels.ambient.peak}
            for label,i in labels {
                append(&items,number_control(shell,70,label,values[i],0,1,.01,u64(i+4),.Mixer_Volume))
                append(&items,ui.Descriptor{key=key(71,label),kind=.Progress,value=clamp(levels[i],0,1),layout={height=ui.pixels(8),width=ui.percent(1)}})
            }
        }
    }
    return {key=key(70,"panel"),kind=.Scroll_Area,children=nodes(shell,{ui.Descriptor{key=key(70,"content"),kind=.Column,layout={padding={12,12,12,12},gap={0,6},width=ui.percent(1)},children=nodes(shell,items[:])}})}
}
