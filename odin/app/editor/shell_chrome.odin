//! Compact menus and semantic icon controls keep scene authoring ahead of editor chrome.
package editor_app

import ui "../../ui"
import editor "../../editor"
import doc "../document"
import render "../render"
import icons "../../icons"

@(private="package")
quiet_button :: proc(name:string,action:Action,disabled:bool=false)->ui.Descriptor {
    result:=button(name,action,disabled); result.button_style=.Quiet; return result
}
@(private="package")
icon_button :: proc(name:string,action:Action,icon:rune,disabled:bool=false)->ui.Descriptor {
    result:=quiet_button(name,action,disabled); result.kind=.Icon_Button; result.icon=icon; result.icon_font=render.UI_FONT_ICONS
    result.layout.width=ui.pixels(32); result.font_size=14; return result
}
@(private="package")
shell_toolbar :: proc(shell:^Shell,size:ui.Vec2)->ui.Descriptor {
    owner:=shell.state.owner; scale:=shell_scale(shell); width:=size.x/scale
    title:="Katla"
    if shell.document!=nil { title=doc.title(shell.document); append(&shell.texts,title) }
    undo:=icon_button("Undo",.Undo,icons.UNDO,owner.mode!=.Editing || !editor.agent_can_undo(&owner.agent.session)); undo.hidden=width<620
    redo:=icon_button("Redo",.Redo,icons.REDO,owner.mode!=.Editing || !editor.agent_can_redo(&owner.agent.session)); redo.hidden=width<620
    play:=icon_button("Play",.Play,icons.PLAY,owner.mode!=.Editing); play.button_style=.Primary
    pause:=icon_button("Resume" if owner.mode==.Paused else "Pause",.Pause,icons.PLAY if owner.mode==.Paused else icons.PAUSE,owner.mode==.Editing)
    stop:=icon_button("Stop",.Stop,icons.STOP,owner.mode==.Editing)
    controls:=nodes(shell,{
        quiet_button("File",.Menu_File),quiet_button("Edit",.Menu_Edit),quiet_button("View",.Menu_View),
        ui.Descriptor{key=key(1,"title"),kind=.Text,text=title,text_max_width=max(1,width-420),hidden=width<500,layout={grow=1,height=ui.pixels(16),align=.Center,margin={0,16,0,12}},has_foreground=true,foreground=shell.ctx.theme.muted},
        ui.Descriptor{key=key(1,"history"),kind=.Row,layout={gap={2,0},margin={0,12,0,0}},children=nodes(shell,{undo,redo})},
        ui.Descriptor{key=key(1,"playback"),kind=.Row,layout={gap={2,0}},children=nodes(shell,{play,pause,stop})},
    })
    for &control in controls[:3] { control.selected=shell.menu==Action(control.action) }
    return {key=key(1,"toolbar"),kind=.Row,has_fixed_bounds=true,fixed_bounds={12*scale,8*scale,max(0,size.x-24*scale),30*scale},layout={gap={2,0},align=.Center},children=controls}
}
@(private="package")
menu_separator :: proc(identity:u64)->ui.Descriptor { return {key=key(4,"separator",identity),kind=.Separator,layout={height=ui.pixels(1),width=ui.percent(1),margin={4,4,4,4}}} }
@(private="package")
menu_item :: proc(name:string,action:Action,shortcut:string="",disabled:bool=false)->ui.Descriptor {
    result:=quiet_button(name,action,disabled); result.key=key(4,name,u64(action)); result.kind=.Menu_Item; result.shortcut=shortcut
    result.layout={width=ui.percent(1),height=ui.pixels(28)}; return result
}
@(private="package")
shell_menu :: proc(shell:^Shell)->ui.Descriptor {
    items:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(items)
    editing:=shell.state.owner.mode==.Editing
    anchor:="File"; x,y:f32=12,42
    #partial switch shell.menu {
    case .Menu_File:
        new_key,open_key,save_key,save_as_key,quit_key:="Ctrl+N","Ctrl+O","Ctrl+S","Ctrl+Shift+S","Ctrl+Q"
        when ODIN_OS==.Darwin { new_key="⌘N"; open_key="⌘O"; save_key="⌘S"; save_as_key="⇧⌘S"; quit_key="⌘Q" }
        append(&items,menu_item("New Scene",.New,new_key,!editing),menu_item("Open Scene…",.Open,open_key,!editing),menu_separator(1),menu_item("Save",.Save,save_key,!editing),menu_item("Save As…",.Save_As,save_as_key,!editing),menu_separator(2),menu_item("Quit",.Quit,quit_key,!editing))
    case .Menu_Edit:
        anchor="Edit"
        undo_key,redo_key:="Ctrl+Z","Ctrl+Shift+Z"
        when ODIN_OS==.Darwin { undo_key="⌘Z"; redo_key="⇧⌘Z" }
        append(&items,menu_item("Undo",.Undo,undo_key,!editing || !editor.agent_can_undo(&shell.state.owner.agent.session)),menu_item("Redo",.Redo,redo_key,!editing || !editor.agent_can_redo(&shell.state.owner.agent.session)))
    case .Menu_Create:
        anchor="Create"
        for name,i in ([6]string{"Cube","Sphere","Plane","Cylinder","Cone","Torus"}) { item:=menu_item(name,.Create_Primitive,"",!editing); item.payload=u64(i); append(&items,item) }
    case .Menu_Hierarchy:
        x=shell.ctx.pointer.x; y=shell.ctx.pointer.y
        append(&items,menu_item("Duplicate",.Duplicate_Entity,"",!editing),menu_item("Delete",.Delete_Entity,"",!editing))
    case .Menu_View:
        anchor="View"
        for label,index in ([4]string{"Single viewport","Two columns","Two rows","Four views"}) { item:=menu_item(label,.Layout); item.payload=u64(index); item.selected=Viewport_Layout(index)==shell.viewports.layout; append(&items,item) }
        append(&items,menu_separator(3))
        for tab in shell.tabs { item:=menu_item(tab.label,.Panel_Open); item.payload=u64(tab.tab); append(&items,item) }
    case:
    }
    scale:=shell_scale(shell)
    if shell.menu!=.Menu_Hierarchy { if bounds,ok:=ui.bounds(shell.ctx,key(1,anchor,u64(shell.menu))); ok { x=bounds.x; y=bounds.y+bounds.height+4*scale } }
    height:f32=12
    for item in items { height+=item.layout.height.value+item.layout.margin.top+item.layout.margin.bottom }
    width:=min(240*scale,max(0,shell.ctx.logical_size.x-16*scale)); height=min(height*scale,max(0,shell.ctx.logical_size.y-16*scale))
    x=clamp(x,8*scale,max(8*scale,shell.ctx.logical_size.x-width-8*scale)); y=clamp(y,8*scale,max(8*scale,shell.ctx.logical_size.y-height-8*scale))
    list:=ui.Descriptor{key=key(1,"menu-items"),kind=.Column,layout={width=ui.percent(1),padding={6,6,6,6}},children=nodes(shell,items[:])}
    scroll:=ui.Descriptor{key=key(1,"menu-scroll"),kind=.Scroll_Area,layout={width=ui.percent(1),height=ui.percent(1)},children=nodes(shell,{list})}
    return {key=key(1,"menu"),kind=.Context_Menu,action=u64(shell.menu),layer=.Popup,has_fixed_bounds=true,fixed_bounds={x,y,width,height},children=nodes(shell,{scroll})}
}
