//! Drained widget intents finish on the scene owner before any native frame acquisition.
package editor_app

import document "../document"
import ecs "../../ecs"
import ui "../../ui"
import "core:encoding/json"

@(private="package")
binding :: proc(shell:^Shell,payload:u64)->^Field_Binding { if payload==0 || payload>u64(len(shell.bindings)) { return nil }; return &shell.bindings[payload-1] }
@(private="package")
field_change :: proc(shell:^Shell,item:^Field_Binding,value:json.Value) {
    if item==nil || !shell.inspector.has_entity { return }
    encoded,error:=json.marshal(value,allocator=shell.allocator); if error!=nil { return }; defer delete(encoded,shell.allocator)
    shell.state.last_error=inspector_set(shell.state,shell.inspector.entity,item.component,item.field.path,encoded)
}
@(private="package")
click :: proc(shell:^Shell,event:ui.Click_Action) {
    action:=Action(event.action)
    if action!=.None && action!=.Material { if error:=shell_finish_gestures(shell); error!=.None { shell.state.last_error=error; return } }
    if shell_material_texture_click(shell,event) { return }
    if shell_material_asset_click(shell,event) { return }
    if shell_script_click(shell,event) { return }
    if shell_particle_click(shell,event) { return }
    if shell_gizmo_click(shell,event) { return }
    if shell_code_click(shell,event) { return }
    if shell_console_click(shell,event) { return }
    if shell_timeline_click(shell,event) { return }
    if shell_host_click(shell,event) { return }
    if shell_service_click(shell,event) { return }
    switch action {
    case .Menu_File,.Menu_Edit,.Menu_View,.Menu_Create,.Menu_Hierarchy: shell.menu=.None if shell.menu==action else action; return
    case .Select:
        mode:=Selection_Mode.Replace
        if .Shift in event.modifiers { mode=.Range } else if .Super in event.modifiers || .Control in event.modifiers { mode=.Toggle }
        selection_set(shell.state,ecs.Entity_Id(event.payload),mode)
        if event.button==.Right { shell.menu=.Menu_Hierarchy; return }
        if event.clicks>=2 { shell_focus(shell) }
    case .Create_Primitive: shell_create(shell,int(event.payload))
    case .Delete_Entity: shell_selection_command(shell,.Destroy)
    case .Duplicate_Entity: shell_selection_command(shell,.Duplicate)
    case .Undo: history_apply(shell.state,false)
    case .Redo: history_apply(shell.state,true)
    case .New: if shell.document!=nil { document.request(shell.document,{kind=.New}) }
    case .Open: if shell.document!=nil { document.choose_path(shell.document,false) }
    case .Save: if shell.document!=nil { document.save(shell.document) }
    case .Save_As: if shell.document!=nil { document.choose_path(shell.document,true) }
    case .Quit: shell_request_close(shell)
    case .Play,.Pause,.Stop:
        name:="play"
        if action==.Stop { name="stop" } else if action==.Pause { name="resume" if shell.state.owner.mode==.Paused else "pause" }
        data,error:=json.marshal(struct{action:string}{name},allocator=shell.allocator)
        if error==nil { execute(shell.state,{kind=.Application,tool_name="simulation",value=data}); delete(data,shell.allocator) }
    case .Document_Submit:
        value,valid:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(40,"path"),0,""),string)
        if valid && shell.document!=nil { document.submit_path(shell.document,value); selection_refresh(shell.state) }
    case .Document_Cancel: if shell.document!=nil { document.respond(shell.document,.Cancel) }
    case .Document_Save: if shell.document!=nil { document.respond(shell.document,.Save) }
    case .Document_Discard: if shell.document!=nil { document.respond(shell.document,.Discard); selection_refresh(shell.state) }
    case .Document_Overwrite: if shell.document!=nil { document.respond(shell.document,.Overwrite) }
    case .Remove_Component:
        if shell.inspector.has_entity && event.payload>0 && event.payload<=u64(len(shell.inspector.components)) {
            execute(shell.state,{kind=.Remove_Component,entity=shell.inspector.entity,component=shell.inspector.components[event.payload-1].name})
        }
    case .Layout: if event.payload<=3 { shell.viewports.layout=Viewport_Layout(event.payload) }
    case .Panel_Open: if event.payload>=1 && event.payload<=11 { ui.dock_open(&shell.dock,ui.Tab_Id(event.payload)) }
    case .None,.Search,.Field,.Add_Component,.Viewport,.Document_Path,.Expand,.Material,.Material_Preset,.Material_Alpha,.Material_Double,.Material_Expand,.Material_Save,.Material_Apply,.Material_Asset_Path,.Material_Texture_Expand,.Material_Role,.Material_Neutral,.Material_Original,.Material_Browser,.Material_Assign,.Material_UV,.Material_Source_Choice,.Material_Source_Text,.Material_Sampling,.Material_Filter,
        .Asset_Back,.Asset_Forward,.Asset_Breadcrumb,.Asset_Reveal,.Asset_Search,.Asset_Select,.Asset_Parent,.Asset_Refresh,.Asset_Open,.Asset_Root,.Asset_New_Folder,.Asset_Folder_Name,.Asset_Folder_Create,.Asset_Delete,.Asset_Delete_Confirm,.Asset_Cancel,
        .Pref_Number,.Pref_Toggle,.Pref_Theme,.Pref_Connection,.Pref_Save,.Mixer_Volume,
        .Host_Connect,.Host_Disconnect,.Host_Interrupt,.Host_Send,.Host_Prompt,
        .Timeline_Play,.Timeline_Pause,.Timeline_Resume,.Timeline_Stop,.Timeline_Clip,.Timeline_Seek,.Timeline_Speed,.Timeline_Loop,.Timeline_Fade,.Timeline_Fade_Time,.Console_Clear,.Console_Search,.Console_Level,.Code_Text,.Code_Save,.Code_Close,.Code_Select,.Code_Confirm_Save,.Code_Discard,.Code_Cancel,.Gizmo_Mode,.Gizmo_Space,.Gizmo_Snap,.Particle_Burst,.Particle_Active,.Particle_Restart,.Particle_Reset_All,.Script_Open,.Script_Reload,.Script_Path,.Script_Variable:
    }
    shell.menu=.None
}
/// Processes the original input order, so a released gesture commits before a following selection or menu action.
shell_actions :: proc(shell:^Shell) {
    events:=ui.actions_drain_all(shell.ctx,shell.allocator); defer delete(events,shell.allocator)
    defer ui.actions_clear(shell.ctx)
    for action in events {
        switch event in action {
        case ui.Click_Action: click(shell,event)
        case ui.Expand_Action: if Action(event.action)==.Select { shell.state.expanded[ecs.Entity_Id(event.payload)]=event.expanded } else if Action(event.action)==.Material_Expand || Action(event.action)==.Material_Texture_Expand { ui.state_set(shell.ctx,{node=event.node,slot=0},event.expanded) }
        case ui.Text_Action:
            value:=ui.action_text(shell.ctx,event)
            if shell_script_text(shell,event,value) { continue }
            if shell_code_text(shell,event,value) { continue }
            if shell_console_text(shell,event,value) { continue }
            if shell_service_text(shell,event,value) { continue }
            #partial switch Action(event.action) {
            case .Search: search_set(shell.state,value)
            case .Field: if event.submitted { field_change(shell,binding(shell,event.payload),value) }
            case .Document_Path: if event.submitted && shell.document!=nil { document.submit_path(shell.document,value); selection_refresh(shell.state) }
            case:
            }
        case ui.Number_Action:
            if Action(event.action)==.Script_Variable { shell_script_variable(shell,event.payload,f64(event.value)); continue }
            if shell_timeline_number(shell,event) { continue }
            if shell_service_number(shell,event) { continue }
            if Action(event.action)==.Material { shell_material_change(shell,event); continue }
            if Action(event.action)==.Material_Sampling { shell_material_sampling_number(shell,event); continue }
            if Action(event.action)==.Field { shell_field_number(shell,event) }
        case ui.Toggle_Action: if Action(event.action)==.Material_Double { shell_material_property(shell,"double_sided",event.value);continue };if Action(event.action)==.Script_Variable { shell_script_variable(shell,event.payload,event.value); continue }; if !shell_timeline_toggle(shell,event) && !shell_service_toggle(shell,event) && Action(event.action)==.Field { field_change(shell,binding(shell,event.payload),event.value) }
        case ui.Selection_Action:
            if event.index<0 || shell_material_filter(shell,event) || shell_code_choice(shell,event) || shell_timeline_choice(shell,event) || shell_service_choice(shell,event) { continue }
            #partial switch Action(event.action) {
            case .Material_Preset: shell_material_preset(shell,event.index)
            case .Material_Alpha: if event.index<3 { modes:=[3]string{"opaque","mask","blend"};shell_material_property(shell,"alpha_mode",modes[event.index]) }
            case .Add_Component:
                if shell.inspector.has_entity && event.index<len(shell.inspector.available) { execute(shell.state,{kind=.Add_Component,entity=shell.inspector.entity,component=shell.inspector.available[event.index]}) }
            case .Field:
                item:=binding(shell,event.payload)
                if item!=nil && event.index<len(item.field.variants) {
                    previous,error:=json.parse(item.field.value,parse_integers=true,allocator=shell.allocator)
                    if error==.None {
                        if _,numeric:=previous.(json.Integer);numeric && event.index<len(item.field.variant_values) { field_change(shell,item,json.Integer(item.field.variant_values[event.index])) }
                        else { field_change(shell,item,item.field.variants[event.index]) }
                        json.destroy_value(previous)
                    }
                }
            case:
            }
        case ui.Dismiss_Action:
            if Action(event.action)==.Document_Cancel { if shell.document!=nil { document.respond(shell.document,.Cancel) } }
            else if Action(event.action)==.Code_Cancel { shell_code_click(shell,{action=u64(Action.Code_Cancel)}) }
            else if Action(event.action)==.Asset_Cancel { shell_service_click(shell,{action=u64(Action.Asset_Cancel)}) }
            else { shell.menu=.None }
        case ui.Dock_Action: if ui.dock_apply(&shell.dock,event)==.None { shell_dock_save(shell) }
        case ui.Pointer_Action: shell_asset_pointer(shell,event); shell_hierarchy_pointer(shell,event); shell_viewport_action(shell,event)
        case ui.Key_Action: shell_shortcut(shell,event)
        case ui.Scroll_Action:
        }
    }
}
@(private="package")
shell_shortcut :: proc(shell:^Shell,event:ui.Key_Action) {
    if event.repeat || shell.document!=nil && shell.document.dialog!=.None { return }
    if shell_code_shortcut(shell,event) { return }
    primary:=.Super in event.modifiers when ODIN_OS==.Darwin else .Control in event.modifiers
    shift:=.Shift in event.modifiers
    action:=Action.None
    if primary {
        #partial switch event.key {
        case .S: action=.Save_As if shift else .Save
        case .O: action=.Open
        case .N: action=.New
        case .Z: action=.Redo if shift else .Undo
        case .Y: action=.Redo
        case .D: shell_selection_command(shell,.Duplicate)
        }
    } else if event.key==.Escape { if shell.gizmo.gesture.active { shell.state.last_error=gizmo_cancel(&shell.gizmo) } else if shell.field_gesture.active || shell.material.active || shell.sampling_gesture.scene.active { shell.state.last_error=shell_cancel_gesture(shell) } else { selection_clear(shell.state) } }
    else if shell.state.owner.mode==.Editing && event.key in (bit_set[ui.Key]{.W,.E,.R}) { shell_gizmo_key(shell,event.key) }
    else if event.key==.F { shell_focus(shell) }
    else if event.key==.Delete || event.key==.Backspace { shell_selection_command(shell,.Destroy) }
    if action!=.None { click(shell,{action=u64(action)}) }
}
