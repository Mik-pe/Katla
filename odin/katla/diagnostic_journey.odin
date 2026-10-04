//! Explicit synthetic-input CLI journeys exercise retained routing and capture the accepted full editor output.
package main
import app "../app"
import editor_app "../app/editor"
import ui "../ui"
import ecs "../ecs"
import "core:fmt"
import "core:path/filepath"

CLI_Check :: struct {name:string,passed:bool}
CLI_Journey :: struct {directory:string,checks:[dynamic]CLI_Check,material_before,material_after:app.Surface_Material,has_material:bool,entity:ecs.Entity_Id,history_before:int,interaction:bool,material_drag_key:u64,material_drag_visible,material_drag_captured,material_drag_outside:bool}
@(private="package")
cli_input :: proc(journey:^CLI_Journey,shell:^editor_app.Shell,frame:int,interaction:bool)->[dynamic]ui.Input_Event {
    events:=make([dynamic]ui.Input_Event)
    target:u64=0; button:=ui.Pointer_Button.Left; release:=true
    switch frame {
    case 0: ui.dock_open(&shell.dock,ui.Tab_Id(editor_app.Panel.Hierarchy))
    case 15:
        for _,node in shell.ctx.nodes { if node.mounted && node.descriptor.action==u64(editor_app.Action.Select) && node.descriptor.text!="Ground" && node.bounds.y>=node.clip.y && node.bounds.y+node.bounds.height<=node.clip.y+node.clip.height { target=node.id.key; break } }
    case 35:
        for _,node in shell.ctx.nodes { if node.mounted && node.descriptor.kind==.Tree_Row { append(&events,ui.Scroll{position={node.bounds.x+10,node.bounds.y+10},delta={0,-8}}); break } }
    case 55: ui.dock_open(&shell.dock,ui.Tab_Id(editor_app.Panel.Assets))
    case 75: ui.dock_open(&shell.dock,ui.Tab_Id(editor_app.Panel.Preferences))
    case 95,105:
        target=editor_app.key(60,"theme")
    case 96,106:
        if node,ok:=shell.ctx.nodes[editor_app.key(60,"theme")]; ok {
            choice:=2 if frame==96 else 1
            point:=ui.Vec2{node.bounds.x+20,node.bounds.y+node.bounds.height+(f32(choice)+.5)*shell.ctx.theme.row_height}
            append(&events,ui.Pointer_Down{position=point,button=.Left},ui.Pointer_Up{position=point,button=.Left})
        }
    case 115: if interaction { ui.dock_open(&shell.dock,ui.Tab_Id(editor_app.Panel.Inspector));ui.dock_open(&shell.dock,ui.Tab_Id(editor_app.Panel.Hierarchy)) }
    case 119:
        if interaction { if node,ok:=shell.ctx.nodes[editor_app.key(50,"expanded")]; ok && node.mounted && !node.descriptor.expanded { target=node.id.key } }
    case 120:
        if interaction { if panel,ok:=shell.ctx.nodes[editor_app.key(30,"panel")]; ok && panel.mounted {
            for _,node in shell.ctx.nodes { if node.mounted && node.descriptor.action==u64(editor_app.Action.Material) && node.descriptor.payload==4 {
                center:=ui.Vec2{panel.bounds.x+panel.bounds.width/2,panel.bounds.y+panel.bounds.height/2}
                append(&events,ui.Scroll{position=center,delta={0,node.bounds.y+node.bounds.height/2-center.y}});break
            } }
        } }
    case 125:
        if interaction && shell.state.selection.has_primary { journey.entity=shell.state.selection.primary;journey.history_before=len(shell.state.owner.agent.session.actions); material,present:=ecs.get_component(&shell.state.owner.world,journey.entity,app.Surface_Material); journey.material_before=material; journey.has_material=present; for _,node in shell.ctx.nodes { if node.mounted && node.descriptor.action==u64(editor_app.Action.Material) && node.descriptor.payload==4 && node.bounds.y>=node.clip.y && node.bounds.y+node.bounds.height<=node.clip.y+node.clip.height && node.clip.width>0 { target=node.id.key; journey.material_drag_key=target;journey.material_drag_visible=true;release=false;break } } }
    case 130:
        if interaction { if node,ok:=shell.ctx.nodes[journey.material_drag_key]; ok && shell.ctx.captured.key==journey.material_drag_key { journey.material_drag_captured=true;append(&events,ui.Pointer_Move{position={node.bounds.x+node.bounds.width+48,node.bounds.y+node.bounds.height/2}}) } }
    case 135:
        if interaction { if node,ok:=shell.ctx.nodes[journey.material_drag_key]; ok && shell.ctx.captured.key==journey.material_drag_key { journey.material_drag_outside=true;append(&events,ui.Pointer_Up{position={node.bounds.x+node.bounds.width+48,node.bounds.y+node.bounds.height/2},button=.Left}) } }
    case 145,155:
        if interaction { modifiers:ui.Modifiers={.Super} when ODIN_OS==.Darwin else {.Control}; if frame==155 { modifiers|={.Shift} }; append(&events,ui.Key_Down{key=.Z,modifiers=modifiers}) }
    }
    if target!=0 { if node,ok:=shell.ctx.nodes[target]; ok { point:=ui.Vec2{node.bounds.x+node.bounds.width/2,node.bounds.y+node.bounds.height/2}; append(&events,ui.Pointer_Down{position=point,button=button}); if release { append(&events,ui.Pointer_Up{position=point,button=button}) } } }
    return events
}
@(private="package")
cli_after_frame :: proc(journey:^CLI_Journey,shell:^editor_app.Shell,gpu:^editor_app.GPU_Owner($R),frame:int,width,height:u32,interaction:bool)->bool {
    name:=""
    switch frame {
    case 10:name="01_default"
    case 30:name="02_entity_selected"; append(&journey.checks,CLI_Check{"Hierarchy click selects a current entity",shell.state.selection.has_primary})
    case 50:name="03_hierarchy_scrolled"
    case 70:name="04_asset_browser"
    case 99:name="05_preferences_light"; append(&journey.checks,CLI_Check{"Light theme changes retained paint",shell.ctx.theme.canvas[0]>.8})
    case 110:name="06_preferences_dark"; append(&journey.checks,CLI_Check{"Dark theme changes retained paint",shell.ctx.theme.canvas[0]<.3})
    case 140:
        if interaction { name="07_material_drag"; material,present:=ecs.get_component(&shell.state.owner.world,journey.entity,app.Surface_Material); journey.material_after=material; append(&journey.checks,CLI_Check{"Captured material drag changes one shared command",journey.material_drag_visible && journey.material_drag_captured && journey.material_drag_outside && journey.has_material && present && material!=journey.material_before && !shell.material.active && len(shell.state.owner.agent.session.actions)==journey.history_before+1}) }
    case 150:
        if interaction { name="08_material_undo"; material,present:=ecs.get_component(&shell.state.owner.world,journey.entity,app.Surface_Material); append(&journey.checks,CLI_Check{"Undo restores exact authored material",journey.has_material && journey.material_after!=journey.material_before && present && material==journey.material_before}) }
    case 160:
        if interaction { name="09_material_redo"; material,present:=ecs.get_component(&shell.state.owner.world,journey.entity,app.Surface_Material); append(&journey.checks,CLI_Check{"Redo restores exact changed material",journey.has_material && journey.material_after!=journey.material_before && present && material==journey.material_after}) }
    }
    if name=="" { return true }
    path,_:=filepath.join({journey.directory,fmt.tprintf("%s.png",name)}); defer delete(path)
    return diagnostic_image(gpu,width,height,path,true)
}
@(private="package")
cli_summary :: proc(journey:^CLI_Journey)->bool {
    path,_:=filepath.join({journey.directory,"receipt.json"}); defer delete(path)
    if !diagnostic_write(struct {input_boundary:string,checks:[]CLI_Check}{"Synthetic retained input with actual full-editor native GPU readback; this does not establish OS event or accessibility behavior",journey.checks[:]},path) { return false }
    if len(journey.checks)<(6 if journey.interaction else 3) { fmt.eprintln("CLI journey ended before all UI checks");return false }
    for check in journey.checks { if !check.passed { fmt.eprintln("Interaction check failed:",check.name); return false } }
    return true
}
