//! Service actions preserve backend ownership while dock contents mount and unmount.
package editor_app
import assets "../assets"
import prefs "../preferences"
import app ".."
import ui "../../ui"
import "core:fmt"

@(private="package")
shell_service_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    if shell_asset_navigation_click(shell,event) { return true }
    action:=Action(event.action); browser:=shell.browser
    #partial switch action {
    case .Asset_Select:
        if browser!=nil && event.payload>0 && event.payload<=u64(len(browser.entries)) {
            mode:=assets.Selection_Mode.Replace; if .Shift in event.modifiers { mode=.Range } else if .Super in event.modifiers || .Control in event.modifiers { mode=.Toggle }
            assets.select_mode(browser,browser.entries[event.payload-1].path,mode)
            if event.clicks>=2 { shell_asset_open(shell,int(event.payload-1)) }
        }
    case .Asset_Parent: if browser!=nil { error:=assets.parent(browser); if error!=.None { message(shell,"Cannot open the parent folder") } }
    case .Asset_Refresh: if browser!=nil { error:=assets.refresh(browser); if error!=.None { message(shell,"Asset refresh failed") } }
    case .Asset_Open: shell_asset_open(shell)
    case .Asset_New_Folder: shell.asset_folder_dialog=true
    case .Asset_Folder_Create:
        if browser!=nil { name,valid:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(52,"name"),0,""),string); if valid { error:=assets.create_folder(browser,name); if error==.None { shell.asset_folder_dialog=false } else { message(shell,"Folder creation failed; choose a valid unused name") } } }
    case .Asset_Delete: if browser!=nil { assets.delete_request(browser) }
    case .Asset_Delete_Confirm: if browser!=nil { result:=assets.delete_confirm(browser); value:=fmt.aprintf("Deleted %d assets: %v",result.removed,result.error); defer delete(value,shell.allocator); message(shell,value) }
    case .Asset_Cancel: shell.asset_folder_dialog=false; if browser!=nil { assets.delete_cancel(browser) }
    case .Pref_Save:
        if shell.preferences!=nil && shell.preference_store!=nil { published,error:=prefs.save(shell.preference_store,shell.preferences); message(shell,"Preferences saved" if published && error==.None else "Preferences could not be saved") }
    case: return false
    }
    return true
}
@(private="package")
shell_service_text :: proc(shell:^Shell,event:ui.Text_Action,value:string)->bool {
    #partial switch Action(event.action) {
    case .Asset_Search: if shell.browser!=nil { assets.search_set(shell.browser,value) }; return true
    case .Asset_Folder_Name: if event.submitted { shell_service_click(shell,{action=u64(Action.Asset_Folder_Create)}) }; return true
    case .Pref_Connection:
        if shell.preferences!=nil {
            socket,ok_socket:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(60,"socket"),0,""),string)
            thread,ok_thread:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(60,"thread"),0,""),string)
            if ok_socket && ok_thread { prefs.set_connection(shell.preferences,socket,thread) }
        }; return true
    }; return false
}
@(private="package")
shell_service_number :: proc(shell:^Shell,event:ui.Number_Action)->bool {
    action:=Action(event.action); value:=shell.preferences; if action!=.Pref_Number && action!=.Mixer_Volume { return false }
    if value==nil { return true }
    switch event.payload {
    case 1:value.font_scale=event.value
    case 2:value.editor.camera_speed=event.value
    case 3:value.editor.grid_size=event.value
    case 4:value.audio.master_volume=event.value
    case 5:value.audio.sfx_volume=event.value
    case 6:value.audio.music_volume=event.value
    case 7:value.audio.ambient_volume=event.value
    }
    prefs.validate(value)
    if shell.audio!=nil && event.payload>=4 { app.audio_service_settings(shell.audio,value.audio.master_volume,value.audio.sfx_volume,value.audio.music_volume,value.audio.ambient_volume) }
    return true
}
@(private="package")
shell_service_toggle :: proc(shell:^Shell,event:ui.Toggle_Action)->bool {
    if Action(event.action)!=.Pref_Toggle { return false }; value:=shell.preferences; if value==nil { return true }
    switch event.payload {
    case 1:value.show_grid=event.value
    case 2:value.show_stats=event.value
    case 3:value.show_physics_debug=event.value
    case 4:value.show_reverb_debug=event.value
    case 5:value.editor.snap_to_grid=event.value
    }
    return true
}
@(private="package")
shell_service_choice :: proc(shell:^Shell,event:ui.Selection_Action)->bool {
    #partial switch Action(event.action) {
    case .Asset_Root:
        if shell.browser!=nil && event.index<2 { error:=assets.navigate_root(shell.browser,app.Mesh_Path_Root(event.index),""); if error!=.None { message(shell,"Asset root unavailable") } }; return true
    case .Pref_Theme: if shell.preferences!=nil && event.index<len(THEME_NAMES) { prefs.set_theme(shell.preferences,THEME_NAMES[event.index]) }; return true
    }; return false
}
