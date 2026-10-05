//! One retained editor shell mounts active dock contents around the shared authored scene.
package editor_app

import doc "../document"
import assets "../assets"
import prefs "../preferences"
import app ".."
import render "../render"
import ui "../../ui"
import ecs "../../ecs"
import icons "../../icons"
import "core:mem"

Action :: enum u64 {
    None,Menu_File,Menu_Edit,Menu_View,Menu_Create,Menu_Hierarchy,Create_Primitive,Delete_Entity,Duplicate_Entity,New,Open,Save,Save_As,Quit,Undo,Redo,
    Play,Pause,Stop,Search,Select,Field,Add_Component,Remove_Component,Viewport,
    Document_Path,Document_Submit,Document_Cancel,Document_Save,Document_Discard,Document_Overwrite,
    Layout,Expand,Material,Material_Preset,Material_Alpha,Material_Double,Material_Expand,Material_Save,Material_Apply,Material_Asset_Path,Material_Texture_Expand,Material_Role,Material_Neutral,Material_Original,Material_Browser,Material_Assign,Material_UV,Material_Source_Choice,Material_Source_Text,Material_Sampling,Material_Filter,Panel_Open,
    Asset_Search,Asset_Select,Asset_Parent,Asset_Refresh,Asset_Open,Asset_Root,Asset_New_Folder,Asset_Folder_Name,Asset_Folder_Create,Asset_Delete,Asset_Delete_Confirm,Asset_Cancel,
    Pref_Number,Pref_Toggle,Pref_Theme,Pref_Connection,Pref_Save,Mixer_Volume,
    Host_Connect,Host_Disconnect,Host_Interrupt,Host_Send,Host_Prompt,
    Timeline_Play,Timeline_Pause,Timeline_Resume,Timeline_Stop,Timeline_Clip,Timeline_Seek,Timeline_Speed,Timeline_Loop,Timeline_Fade,Timeline_Fade_Time,Console_Clear,Console_Search,Console_Level,Code_Text,Code_Save,Code_Close,Code_Select,Code_Confirm_Save,Code_Discard,Code_Cancel,Gizmo_Mode,Gizmo_Space,Gizmo_Snap,Particle_Burst,Particle_Active,Particle_Restart,Particle_Reset_All,Script_Open,Script_Reload,Script_Path,Script_Variable,Asset_Back,Asset_Forward,Asset_Breadcrumb,Asset_Reveal,
}
Field_Binding :: struct { component:string,field:^Inspector_Field,color_channel:int,is_color:bool }
/// The font/UI owner and document service are stationary borrowed application dependencies.
Shell :: struct {
    state:^State,ctx:^ui.Context,document:^doc.State,
    dock:ui.Dock_Tree,viewports:Viewport_Grid,panel_size:ui.Vec2,
    inspector:Inspector,bindings:[dynamic]Field_Binding,
    children:[dynamic][]ui.Descriptor,texts:[dynamic]string,option_lists:[dynamic][]string,
    menu:Action,allocator:mem.Allocator,
    tabs:[11]ui.Dock_Tab,
    host:^Host_Panel,browser:^assets.State,preferences:^prefs.Value,preference_store:^prefs.Store,audio:^app.Audio_Service,
    asset_folder_dialog:bool,service_message,last_runtime_diagnostic:string,
    pick_requested,pick_dragged:bool,pick_position,pick_start:ui.Vec2,pick_modifiers:ui.Modifiers,pick_view:int,
    navigation_orbit,navigation_pan:bool,navigation_modifiers:ui.Modifiers,
    field_gesture:app.Scene_Gesture,field_gesture_node:u64,
    hierarchy_drag_start:ui.Vec2,hierarchy_drag_entity:u64,hierarchy_drag_started,hierarchy_drag_active:bool,
    asset_drag_start:ui.Vec2,asset_drag_started,asset_drag_active:bool,asset_drag:assets.Drag_Batch,
    material:app.Material_Gesture,sampling_gesture:app.Material_Sampling_Gesture,sampling_node:u64,material_info:app.Material_Inspector,material_info_ready:bool,material_preview_textures:[5]ui.Texture_Id,material_preview_entity:ecs.Entity_Id,material_preview_has_entity:bool,gizmo_mode:render.Overlay_Mode,console:Console_State,code:Code_Documents,syntax_lists:[dynamic][]ui.Text_Run,code_quit_requested:bool,
    script_panel:Script_Panel,particle_statistics:string,particle_reset_requested:bool,frame_seconds:f32,frame_passes:int,frame_serial:u64,
    gizmo:Gizmo_Controller,gizmo_local:bool,gizmo_hover:render.Overlay_Handle,gizmo_meshes:^[4]render.Overlay_Mesh,gizmo_frames:[4]render.Frame_Data,
}

/// Stable control identity depends on semantic names and entity identity, never on row index.
key :: proc(scope:u64,name:string,identity:u64=0)->u64 {
    hash:=u64(1469598103934665603) ~ scope
    for byte in transmute([]byte)name { hash=(hash ~ u64(byte))*1099511628211 }
    hash=(hash ~ identity)*1099511628211
    return hash if hash!=0 else u64(1)
}
shell_init :: proc(shell:^Shell,state:^State,ctx:^ui.Context,document:^doc.State)->ui.Dock_Error {
    shell^={state=state,ctx=ctx,document=document,allocator=state.allocator}
    console_init(&shell.console,state.allocator); code_documents_init(&shell.code,state.owner); shell.syntax_lists=make([dynamic][]ui.Text_Run,state.allocator)
    shell.option_lists=make([dynamic][]string,state.allocator); shell.children=make([dynamic][]ui.Descriptor,state.allocator); shell.texts=make([dynamic]string,state.allocator); shell.bindings=make([dynamic]Field_Binding,state.allocator)
    shell.tabs={{1,"Hierarchy"},{2,"Viewport"},{3,"Inspector"},{4,"Assets"},{5,"Co-Creator"},{6,"Preferences"},{7,"Particles"},{8,"Console"},{9,"Mixer"},{10,"Timeline"},{11,"Code"}}
    viewport_grid_init(&shell.viewports)
    error:=ui.dock_init(&shell.dock,{ui.Tab_Id(Panel.Viewport),ui.Tab_Id(Panel.Hierarchy),ui.Tab_Id(Panel.Inspector),ui.Tab_Id(Panel.Assets),ui.Tab_Id(Panel.Console),ui.Tab_Id(Panel.Mixer)},state.allocator)
    if error!=.None { return error }
    root:=shell.dock.root
    error=ui.dock_apply(&shell.dock,{kind=.Move,source=root,target=root,tab=ui.Tab_Id(Panel.Inspector),zone=.Right})
    if error!=.None { return error }
    center:=dock_leaf(shell,.Viewport)
    error=ui.dock_apply(&shell.dock,{kind=.Move,source=center,target=center,tab=ui.Tab_Id(Panel.Hierarchy),zone=.Left})
    if error!=.None { return error }
    center=dock_leaf(shell,.Viewport)
    error=ui.dock_apply(&shell.dock,{kind=.Move,source=center,target=center,tab=ui.Tab_Id(Panel.Assets),zone=.Bottom})
    if error!=.None { return error }
    bottom:=dock_leaf(shell,.Assets)
    for tab in ([2]Panel{.Console,.Mixer}) { source:=dock_leaf(shell,tab); error=ui.dock_apply(&shell.dock,{kind=.Move,source=source,target=bottom,tab=ui.Tab_Id(tab),zone=.Center,index=3}); if error!=.None { return error } }
    // Ratios are local to their splits: left hierarchy, dominant viewport, right inspector.
    root_node:=shell.dock.nodes[shell.dock.root]; root_node.ratio=0.77
    left:=shell.dock.nodes[root_node.children[0]]; left.ratio=0.21
    center_node:=shell.dock.nodes[left.children[1]]; center_node.ratio=.74
    error=ui.dock_apply(&shell.dock,{kind=.Activate,source=bottom,tab=ui.Tab_Id(Panel.Assets)})
    if error!=.None { return error }
    return .None
}
@(private="package")
dock_leaf :: proc(shell:^Shell,panel:Panel)->ui.Dock_Id { for id,node in shell.dock.nodes { for tab in node.tabs { if tab==ui.Tab_Id(panel) { return id } } }; return 0 }
@(private="package")
shell_frame_destroy :: proc(shell:^Shell) {
    for list in shell.syntax_lists { delete(list,shell.allocator) }; clear(&shell.syntax_lists)
    for list in shell.option_lists { delete(list,shell.allocator) }; clear(&shell.option_lists)
    inspector_destroy(&shell.inspector);app.material_inspector_destroy(&shell.material_info,shell.allocator);shell.material_info_ready=false; clear(&shell.bindings)
    for children in shell.children { delete(children,shell.allocator) }; clear(&shell.children)
    for text in shell.texts { delete(text,shell.allocator) }; clear(&shell.texts)
}
shell_destroy :: proc(shell:^Shell) { if shell.state!=nil && shell.state.owner!=nil && shell.state.owner.before_mutation_state==shell { shell.state.owner.before_mutation=nil; shell.state.owner.before_mutation_state=nil }; if shell.gizmo.gesture.active { gizmo_cancel(&shell.gizmo) }; gizmo_destroy(&shell.gizmo); script_panel_destroy(shell); console_destroy(&shell.console); code_documents_destroy(&shell.code); delete(shell.syntax_lists); assets.drag_batch_destroy(&shell.asset_drag); if shell.field_gesture.active { app.scene_gesture_cancel(shell.state.owner,&shell.field_gesture) }; app.scene_gesture_destroy(&shell.field_gesture); if shell.material.active { app.material_gesture_cancel(shell.state.owner,&shell.material) }; app.material_gesture_destroy(&shell.material); if shell.sampling_gesture.scene.active { app.material_sampling_gesture_cancel(shell.state.owner,&shell.sampling_gesture) }; app.material_sampling_gesture_destroy(&shell.sampling_gesture); shell_frame_destroy(shell); ui.dock_destroy(&shell.dock); delete(shell.option_lists); delete(shell.children); delete(shell.texts); delete(shell.bindings); delete(shell.service_message,shell.allocator); delete(shell.last_runtime_diagnostic,shell.allocator); delete(shell.particle_statistics,shell.allocator); shell^={} }
@(private="package")
nodes :: proc(shell:^Shell,items:[]ui.Descriptor)->[]ui.Descriptor {
    owned:=make([]ui.Descriptor,len(items),shell.allocator); copy(owned,items); append(&shell.children,owned); return owned
}
@(private="package")
button :: proc(name:string,action:Action,disabled:bool=false)->ui.Descriptor { return {key=key(1,name,u64(action)),kind=.Button,text=name,action=u64(action),disabled=disabled,layout={height=ui.pixels(30),padding={0,8,0,8}}} }
@(private="package")
text :: proc(scope:u64,value:string)->ui.Descriptor { return {key=key(scope,value),kind=.Text,text=value,layout={height=ui.pixels(22),width=ui.percent(1)}} }

/// Builds borrowed descriptors; call ui.frame and shell_actions before the next shell_build.
shell_build :: proc(shell:^Shell,size:ui.Vec2)->ui.Descriptor {
    shell_frame_destroy(shell); shell_appearance(shell); hierarchy_refresh(shell.state)
    inspector,error:=inspector_read(shell.state); shell.inspector=inspector; if error!=.None { shell.state.last_error=error }
    if inspector.has_entity { info,info_error:=app.material_inspector_read(shell.state.owner,inspector.entity);if info_error==.None {shell.material_info=info;shell.material_info_ready=true} }
    toolbar:=shell_toolbar(shell,size)
    scale:=shell_scale(shell)
    dock_bounds:=ui.Rect{6*scale,48*scale,max(0,size[0]-12*scale),max(0,size[1]-72*scale)}
    bounds:=ui.dock_bounds(&shell.dock,dock_bounds,tab_height=shell.ctx.theme.row_height,allocator=shell.allocator); defer delete(bounds,shell.allocator)
    dock:=shell_dock_descriptor(shell,bounds,0,dock_bounds)
    for tab in shell.tabs { if !panel_active(bounds,tab.tab) { ui.retain(shell.ctx,panel_key(Panel(tab.tab))) } }
    status:="Editing"; if shell.state.owner.mode==.Playing { status="Playing" } else if shell.state.owner.mode==.Paused { status="Paused" }
    label:=text(3,shell_status(shell,status)); label.has_foreground=true; label.foreground=shell.ctx.theme.muted; label.font_size=11; label.has_fixed_bounds=true; label.fixed_bounds={12*scale,max(0,size[1]-18*scale),max(0,size[0]-24*scale),18*scale}
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children); append(&children,toolbar,dock,label)
    for floating in shell.dock.floating { append(&children,shell_dock_descriptor(shell,bounds,floating.root,floating.bounds)) }
    if shell.code.dialog!=.None { append(&children,shell_code_dialog(shell,size)) }
    if shell.menu!=.None { append(&children,shell_menu(shell)) }
    if shell.document!=nil && shell.document.dialog!=.None { append(&children,shell_document(shell,size)) }
    if shell.browser!=nil && (shell.asset_folder_dialog || len(shell.browser.pending_delete)>0) { append(&children,shell_asset_dialog(shell,size)) }
    root:=ui.Descriptor{key=key(1,"root"),kind=.Stack,children=nodes(shell,children[:]),has_background=true,background=shell.ctx.theme.canvas}
    scale_descriptor(&root,scale); return root
}
@(private="package")
shell_dock_descriptor :: proc(shell:^Shell,bounds:[]ui.Dock_Bounds,root:ui.Dock_Id,rect:ui.Rect)->ui.Descriptor {
    panels:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(panels)
    for region in bounds {
        if !region.has_active || region.floating_root!=root { continue }
        shell.panel_size={region.content.width,region.content.height}
        panel:ui.Descriptor
        switch Panel(region.active) {
        case .Hierarchy: panel=shell_hierarchy(shell)
        case .Inspector: panel=shell_inspector(shell)
        case .Viewport: panel=shell_viewports(shell,region.content)
        case .Assets: panel=shell_assets(shell)
        case .Preferences: panel=shell_preferences(shell)
        case .Mixer: panel=shell_mixer(shell)
        case .Console: panel=shell_console(shell)
        case .Co_Creator: panel=shell_host(shell)
        case .Timeline: panel=shell_timeline(shell)
        case .Code: panel=shell_code(shell)
        case .Particles: panel=shell_particles(shell)
        }
        panel.has_fixed_bounds=true; panel.fixed_bounds=region.content; panel.clip_children=true; panel.has_background=true; panel.background=shell.ctx.theme.panel
        append(&panels,panel)
    }
    return {key=key(2,"dock",u64(root)),kind=.Dock_Space,dock=&shell.dock,dock_root=root,dock_tabs=shell.tabs[:],layer=.Overlay if root!=0 else .Content,children=nodes(shell,panels[:]),has_fixed_bounds=true,fixed_bounds=rect}
}
@(private="package")
shell_panel_width :: proc(shell:^Shell)->f32 { return max(1,(shell.panel_size.x if shell.panel_size.x>0 else shell.ctx.logical_size.x if shell.ctx.logical_size.x>0 else 400)/shell_scale(shell)) }
@(private="package")
shell_hierarchy :: proc(shell:^Shell)->ui.Descriptor {
    search_key:=key(10,"search")
    search:=ui.Descriptor{key=search_key,kind=.Text_Input,placeholder="Search entities",state=ui.state(shell.ctx,search_key,0,shell.state.search),action=u64(Action.Search),layout={height=ui.pixels(30),width=ui.percent(1)}}
    rows:=make([]ui.Descriptor,len(shell.state.rows),shell.allocator); defer delete(rows,shell.allocator)
    for row,i in shell.state.rows {
        glyph:=icons.CUBE
        if ecs.get_component_mut(&shell.state.owner.world,row.entity,app.Scene_Point_Light)!=nil || ecs.get_component_mut(&shell.state.owner.world,row.entity,app.Scene_Directional_Light)!=nil { glyph=icons.LIGHTBULB }
        else if ecs.get_component_mut(&shell.state.owner.world,row.entity,app.Particle_Emitter)!=nil { glyph=icons.MAGIC }
        rows[i]={key=key(10,"entity",u64(row.entity)),kind=.Tree_Row,draggable=true,text=row.name,icon=glyph,icon_font=render.UI_FONT_ICONS,action=u64(Action.Select),payload=u64(row.entity),selected=row.selected,expanded=row.expanded,has_children=row.has_children,layout={height=ui.pixels(28),width=ui.percent(1),padding={0,0,0,f32(row.depth)*14}}}
    }
    scroll:=ui.Descriptor{key=key(10,"rows"),kind=.Scroll_Area,layout={grow=1,width=ui.percent(1)},children=nodes(shell,{ui.Descriptor{key=key(10,"list"),kind=.Column,layout={width=ui.percent(1)},children=nodes(shell,rows)}})}
    return {key=key(10,"panel"),kind=.Column,layout={padding={8,8,8,8},gap={0,8}},children=nodes(shell,{ui.Descriptor{key=key(10,"tools"),kind=.Row,layout={gap={4,0}},children=nodes(shell,{icon_button("Create",.Menu_Create,icons.PLUS,shell.state.owner.mode!=.Editing),icon_button("Duplicate",.Duplicate_Entity,icons.COPY,!shell.state.selection.has_primary || shell.state.owner.mode!=.Editing),icon_button("Delete",.Delete_Entity,icons.TRASH_ALT,!shell.state.selection.has_primary || shell.state.owner.mode!=.Editing)})},search,scroll})}
}
@(private="package")
shell_viewports :: proc(shell:^Shell,bounds:ui.Rect)->ui.Descriptor {
    toolbar_height:=40*shell_scale(shell)
    viewport_grid_layout(&shell.viewports,{bounds.x,bounds.y+toolbar_height,bounds.width,max(0,bounds.height-toolbar_height)})
    children:=make([]ui.Descriptor,viewport_count(shell.viewports.layout),shell.allocator); defer delete(children,shell.allocator)
    for &slot,index in shell.viewports.slots[:len(children)] {
        children[index]={key=key(20,"image",u64(index)),kind=.Image,texture=slot.texture,action=u64(Action.Viewport),payload=u64(index),focusable=true,has_fixed_bounds=true,fixed_bounds=slot.bounds}
    }
    panels:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(panels); append(&panels,..children)
    tools:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(tools)
    for label,index in ([3]string{"Move (W)","Rotate (E)","Scale (R)"}) { glyphs:=[3]rune{icons.CROSSHAIRS,icons.REFRESH,icons.EXPAND}; control:=icon_button(label,.Gizmo_Mode,glyphs[index],shell.state.owner.mode!=.Editing); control.key=key(21,label); control.payload=u64(index); control.selected=render.Overlay_Mode(index)==shell.gizmo_mode; append(&tools,control) }
    space:=quiet_button("Local" if shell.gizmo_local else "World",.Gizmo_Space); space.layout.margin.left=8
    snap:=quiet_button("Snap",.Gizmo_Snap); snap.selected=shell.preferences!=nil && shell.preferences.editor.snap_to_grid
    append(&tools,space,snap)
    append(&panels,ui.Descriptor{key=key(21,"tools"),kind=.Row,has_fixed_bounds=true,fixed_bounds={bounds.x+8*shell_scale(shell),bounds.y+4*shell_scale(shell),max(0,bounds.width-16*shell_scale(shell)),30*shell_scale(shell)},layout={gap={4,0}},children=nodes(shell,tools[:])})
    return {key=key(20,"panel"),kind=.Stack,children=nodes(shell,panels[:])}
}

@(private="package")
panel_active :: proc(bounds:[]ui.Dock_Bounds,tab:ui.Tab_Id)->bool { for item in bounds { if item.has_active && item.active==tab { return true } }; return false }
@(private="package")
panel_key :: proc(panel:Panel)->u64 {
    switch panel {
    case .Hierarchy:return key(10,"panel")
    case .Viewport:return key(20,"panel")
    case .Inspector:return key(30,"panel")
    case .Assets:return key(50,"panel")
    case .Preferences:return key(60,"panel")
    case .Mixer:return key(70,"panel")
    case .Console:return key(80,"panel")
    case .Co_Creator:return key(90,"panel")
    case .Particles:return key(100,"panel")
    case .Timeline:return key(110,"panel")
    case .Code:return key(120,"panel")
    }; return 0
}
/// Services remain application-owned and stationary while panels may be closed or reopened.
shell_services :: proc(shell:^Shell,host:^Host_Panel,browser:^assets.State,preferences:^prefs.Value,store:^prefs.Store,audio:^app.Audio_Service) { shell.host=host; shell.browser=browser; shell.preferences=preferences; shell.preference_store=store; shell.audio=audio }
