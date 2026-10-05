//! Confined browser paths drive real selection, preview, insertion and confirmed file operations.
package editor_app
import assets "../assets"
import icons "../../icons"
import render "../render"
import app ".."
import ui "../../ui"
import editor "../../editor"
import "core:encoding/json"
import "core:fmt"
import "core:strings"

@(private="package")
ASSET_ROOT_OPTIONS: [2]string ={"Resources","Project"}
@(private="package")
message :: proc(shell:^Shell,value:string) { next:=strings.clone(value,shell.allocator); delete(shell.service_message,shell.allocator); shell.service_message=next }
@(private="package")
shell_assets :: proc(shell:^Shell)->ui.Descriptor {
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    browser:=shell.browser
    if browser==nil { append(&children,text(50,"Asset roots are unavailable")) }
    else {
        root_key:=key(50,"root"); root_state:=ui.state(shell.ctx,root_key,0,f32(browser.root)); if shell.ctx.captured.key!=root_key { ui.state_set(shell.ctx,root_state,f32(browser.root)) }
        root_choice:=ui.Descriptor{key=root_key,kind=.Combo,options=ASSET_ROOT_OPTIONS[:],state=root_state,action=u64(Action.Asset_Root),layout={height=ui.pixels(30),width=ui.pixels(116)}}
        search_key:=key(50,"search")
        search:=ui.Descriptor{key=search_key,kind=.Text_Input,placeholder="Search assets",state=ui.state(shell.ctx,search_key,0,browser.search),action=u64(Action.Asset_Search),layout={height=ui.pixels(30),grow=1}}
        tools:=nodes(shell,{root_choice,icon_button("Back",.Asset_Back,icons.CHEVRON_LEFT,!assets.can_back(browser)),icon_button("Forward",.Asset_Forward,icons.CHEVRON_RIGHT,!assets.can_forward(browser)),icon_button("Parent",.Asset_Parent,icons.ARROW_UP,browser.directory==""),icon_button("Refresh",.Asset_Refresh,icons.REFRESH),icon_button("New Folder",.Asset_New_Folder,icons.FOLDER),icon_button("Delete",.Asset_Delete,icons.TRASH_ALT,len(browser.selected_paths)==0),search})
        append(&children,ui.Descriptor{key=key(50,"tools"),kind=.Row,layout={gap={6,0},wrap=true,no_shrink=true},children=tools})
        append(&children,shell_asset_breadcrumbs(shell))
        entries:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(entries)
        for entry,i in browser.entries {
            selected:=false; for path in browser.selected_paths { if path==entry.path { selected=true; break } }
            row:=ui.Descriptor{key=key(51,entry.path,u64(browser.root)),kind=.Selectable,draggable=entry.kind!=.Folder,text=entry.name,action=u64(Action.Asset_Select),payload=u64(i+1),selected=selected,icon=icons.FOLDER if entry.kind==.Folder else icons.FILE,icon_font=render.UI_FONT_ICONS,layout={height=ui.pixels(30),width=ui.percent(1)}}
            if entry.kind==.Image { row=shell_asset_image(shell,entry,row) }
            append(&entries,row)
        }
        list:=ui.Descriptor{key=key(50,"list"),kind=.Column,layout={width=ui.percent(1)},children=nodes(shell,entries[:])}
        append(&children,ui.Descriptor{key=key(50,"entries"),kind=.Scroll_Area,layout={grow=1,width=ui.percent(1)},children=nodes(shell,{list})})
        append(&children,ui.Descriptor{key=key(50,"selected-tools"),kind=.Row,layout={gap={6,0},wrap=true,no_shrink=true},children=nodes(shell,{quiet_button("Open selected asset",.Asset_Open,browser.selected==""),quiet_button("Reveal in file manager",.Asset_Reveal,browser.selected=="")})})
    }
    return {key=key(50,"panel"),kind=.Column,layout={padding={8,8,8,8},gap={0,6}},children=nodes(shell,children[:])}
}
@(private="package")
shell_asset_image :: proc(shell:^Shell,entry:assets.Entry,row:ui.Descriptor)->ui.Descriptor {
    result:=row; result.text=""; result.icon=0; result.layout.height=ui.pixels(68); result.layout.padding={6,8,6,8}
    visual:=ui.Descriptor{key=key(55,entry.path,u64(shell.browser.root)),kind=.Text,text="Image",layout={width=ui.pixels(56),height=ui.pixels(56)},has_foreground=true,foreground=shell.ctx.theme.muted}
    if entry.thumbnail_texture!=0 && entry.thumbnail_width>0 && entry.thumbnail_height>0 {
        scale:=f32(56)/f32(max(entry.thumbnail_width,entry.thumbnail_height))
        visual.kind=.Image; visual.text=""; visual.texture=ui.Texture_Id(entry.thumbnail_texture); visual.disabled=true
        visual.layout.width=ui.pixels(f32(entry.thumbnail_width)*scale); visual.layout.height=ui.pixels(f32(entry.thumbnail_height)*scale)
    }
    status:="Waiting for preview"
    #partial switch entry.thumbnail {
    case .Loading: status="Loading preview"
    case .Ready: status="Image preview"
    case .Failed: status="Preview unavailable"
    }
    if entry.thumbnail_error && entry.thumbnail_texture!=0 && entry.thumbnail!=.Loading { status="Update failed · showing previous preview" }
    title:=text(56,entry.name); title.key=key(56,entry.path,u64(shell.browser.root));title.text_max_width=max(1,shell_panel_width(shell)-100)
    detail:=text(57,status); detail.key=key(57,entry.path,u64(shell.browser.root)); detail.has_foreground=true; detail.foreground=shell.ctx.theme.muted
    labels:=ui.Descriptor{key=key(58,entry.path,u64(shell.browser.root)),kind=.Column,layout={grow=1,gap={0,2}},children=nodes(shell,{title,detail})}
    content:=ui.Descriptor{key=key(59,entry.path,u64(shell.browser.root)),kind=.Row,layout={width=ui.percent(1),height=ui.percent(1),gap={10,0},align=.Center},children=nodes(shell,{visual,labels})}
    result.children=nodes(shell,{content}); return result
}
@(private="package")
shell_asset_open :: proc(shell:^Shell,requested_index:int=-1) {
    index:=requested_index
    browser:=shell.browser; if browser==nil { return }
    if index<0 { for entry,i in browser.entries { if entry.path==browser.selected { index=i; break } } }
    if index<0 || index>=len(browser.entries) { return }
    entry:=browser.entries[index]
    switch entry.kind {
    case .Folder: error:=assets.navigate(browser,entry.path); if error!=.None { message(shell,"Cannot open this folder") }
    case .Audio:
        if shell.audio!=nil { error:=app.audio_service_preview(shell.audio,{path=entry.path,root=browser.root}); value:=fmt.aprintf("Audio preview: %v",error); defer delete(value,shell.allocator); message(shell,value) }
    case .Model,.Prefab:
        if strings.has_suffix(entry.path,".katprefab") {
            path,valid:=assets.project_path(browser,entry.path); if !valid { message(shell,"This prefab root must be inside the project before scene insertion"); return }; defer delete(path,shell.allocator)
            bytes,error:=json.marshal(struct{action,path:string}{"instantiate",path},allocator=shell.allocator); if error!=nil { return }; defer delete(bytes,shell.allocator)
            execute_batch(shell.state,{editor.Scene_Op{kind=.Application,tool_name="prefab",value=bytes}})
        } else { execute_batch(shell.state,{editor.Scene_Op{kind=.Spawn_Model,path=entry.path,project_asset=browser.root==.Project,scale={1,1,1}}}) }
    case .Script:
        error:=code_document_open(&shell.code,browser.root,entry.path)
        if error==.None { ui.dock_open(&shell.dock,ui.Tab_Id(Panel.Code)); shell_dock_save(shell) } else { message(shell,"Cannot open this script source") }
    case .Material:
        path,valid:=assets.project_path(browser,entry.path);if !valid { message(shell,"Material assets must resolve inside the project");return };defer delete(path,shell.allocator)
        shell_material_asset_apply(shell,path)
    case .Shader,.Image,.Font,.Unknown:
        value:=fmt.aprintf("Selected %s (%d bytes)",entry.path,entry.size); defer delete(value,shell.allocator); message(shell,value)
    }
}
@(private="package")
shell_asset_dialog :: proc(shell:^Shell,size:ui.Vec2)->ui.Descriptor {
    deleting:=len(shell.browser.pending_delete)>0
    content:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(content)
    append(&content,text(52,"Delete selected assets?" if deleting else "Create folder"))
    if deleting {
        append(&content,text(52,"This removes the selected files and folders from disk."))
        for path in shell.browser.pending_delete { item:=text(53,path); item.key=key(53,path); append(&content,item) }
    } else {
        folder_key:=key(52,"name"); append(&content,ui.Descriptor{key=folder_key,kind=.Text_Input,placeholder="Folder name",state=ui.state(shell.ctx,folder_key,0,""),action=u64(Action.Asset_Folder_Name),layout={height=ui.pixels(30),width=ui.percent(1)}})
    }
    append(&content,ui.Descriptor{key=key(52,"buttons"),kind=.Row,layout={gap={8,0}},children=nodes(shell,{button("Delete permanently" if deleting else "Create",.Asset_Delete_Confirm if deleting else .Asset_Folder_Create),button("Cancel",.Asset_Cancel)})})
    height:=min(size[1]-32,f32(len(content))*30+60); width:=min(size[0]-32,480)
    panel:=ui.Descriptor{key=key(52,"content"),kind=.Column,layout={padding={16,16,16,16},gap={0,10}},children=nodes(shell,content[:])}
    return {key=key(52,"dialog"),kind=.Modal,action=u64(Action.Asset_Cancel),has_fixed_bounds=true,fixed_bounds={(size[0]-width)/2,(size[1]-height)/2,width,height},children=nodes(shell,{panel})}
}

@(private="package")
shell_asset_breadcrumbs :: proc(shell:^Shell)->ui.Descriptor {
    browser:=shell.browser; controls:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(controls)
    root:=quiet_button("Resources" if browser.root==.Resource else "Project",.Asset_Breadcrumb,browser.directory==""); root.key=key(54,"root",u64(browser.root)); root.payload=0; append(&controls,root)
    remaining:=browser.directory; length:=0; index:=0
    for segment in strings.split_iterator(&remaining,"/") {
        if segment=="" { break }; length+=len(segment); index+=1
        control:=quiet_button(segment,.Asset_Breadcrumb,length==len(browser.directory)); control.key=key(54,browser.directory[:length],u64(browser.root)); control.payload=u64(index); append(&controls,control); length+=1
    }
    return {key=key(54,"breadcrumbs"),kind=.Row,layout={gap={4,0},wrap=true,no_shrink=true},children=nodes(shell,controls[:])}
}
@(private="package")
shell_asset_navigation_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    action:=Action(event.action); if action!=.Asset_Back && action!=.Asset_Forward && action!=.Asset_Breadcrumb && action!=.Asset_Reveal { return false }
    browser:=shell.browser; if browser==nil { return true }
    if action==.Asset_Reveal {
        result:=assets.reveal(browser); value:=fmt.aprintf("Reveal: %v · filesystem %v · native %d",result.error,result.filesystem_error,result.native_error,allocator=shell.allocator); defer delete(value,shell.allocator)
        message(shell,"Reveal sent to file manager" if result.error==.None else value); return true
    }
    error:=assets.back(browser) if action==.Asset_Back else assets.forward(browser) if action==.Asset_Forward else assets.breadcrumb(browser,int(event.payload))
    if error!=.None { value:=fmt.aprintf("Folder navigation failed: %v",error,allocator=shell.allocator); defer delete(value,shell.allocator); message(shell,value) }; return true
}
