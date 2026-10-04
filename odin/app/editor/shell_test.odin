package editor_app
import app ".."
import ui "../../ui"
import ecs "../../ecs"
import km "../../math"
import editor "../../editor"
import prefs "../preferences"
import "core:testing"
import "core:math"
import "core:unicode/utf8"

// Fixture metrics verify shell reconciliation only; native shaped glyphs have separate GPU acceptance.
fixture_measure :: proc(_:rawptr,_:ui.Font_Id,value:string,size,wrap:f32)->ui.Vec2 { return {min(f32(len(value))*size/2,wrap) if wrap>0 else f32(len(value))*size/2,size} }
fixture_caret :: proc(_:rawptr,_:ui.Font_Id,_:string,size,_:f32,offset:int)->ui.Vec2 { return {f32(offset)*size/2,0} }
fixture_hit :: proc(_:rawptr,_:ui.Font_Id,value:string,size,_:f32,point:ui.Vec2)->int { return clamp(int(math.round(point[0]/max(1,size/2))),0,len(value)) }

fixture_grapheme :: proc(_:rawptr,value:string,offset,direction:int)->int {
    if direction<0 { if offset<=0 { return 0 }; _,size:=utf8.decode_last_rune_in_string(value[:min(offset,len(value))]); return max(0,offset-size) }
    if offset>=len(value) { return len(value) }; _,size:=utf8.decode_rune_in_string(value[max(0,offset):]); return min(len(value),offset+size)
}
fixture_navigate :: proc(state:rawptr,_:ui.Font_Id,value:string,_:f32,_:f32,offset,direction:int)->int { return fixture_grapheme(state,value,offset,direction) }

@(test)
test_canonical_shell_docking_reopen_inactive_state_and_modal_keys :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; testing.expect(t,ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme})==.None); defer ui.context_destroy(&ctx)
    shell:Shell; testing.expect(t,shell_init(&shell,&state,&ctx,nil)==.None); defer shell_destroy(&shell)
    value:=prefs.defaults(); defer prefs.destroy(&value); shell.preferences=&value
    root:=shell_build(&shell,{1200,800}); _,frame:=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    testing.expect(t,dock_leaf(&shell,.Assets)!=0 && dock_leaf(&shell,.Mixer)==dock_leaf(&shell,.Assets))
    ui.actions_clear(&ctx)
    ui.dock_open(&shell.dock,ui.Tab_Id(Panel.Preferences))
    root=shell_build(&shell,{1200,800}); _,frame=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    hook:=ui.state(&ctx,key(60,"socket"),0,""); ui.state_set(&ctx,hook,string("/tmp/actual.sock"))
    leaf:=dock_leaf(&shell,.Preferences); ui.dock_apply(&shell.dock,{kind=.Activate,target=leaf,tab=ui.Tab_Id(Panel.Viewport)})
    root=shell_build(&shell,{1200,800}); _,frame=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    retained,valid:=ui.state_get(&ctx,hook,string); testing.expect(t,valid && retained=="/tmp/actual.sock")
    ui.dock_open(&shell.dock,ui.Tab_Id(Panel.Preferences)); root=shell_build(&shell,{1200,800}); _,frame=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    retained,valid=ui.state_get(&ctx,hook,string); testing.expect(t,valid && retained=="/tmp/actual.sock")
    shell.menu=.Menu_View; root=shell_build(&shell,{1200,800}); _,frame=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    ui.actions_clear(&ctx)
}

@(test)
test_field_release_precedes_selection_and_one_shared_multiselect_command :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    first:=ecs.create_entity(&owner.world); second:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,first,app.Scene_Transform{km.Transform{rotation=km.QUAT_IDENTITY,scale={1,1,1}}})
    ecs.add_component(&owner.world,second,app.Scene_Transform{km.Transform{rotation=km.QUAT_IDENTITY,scale={1,1,1}}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); hierarchy_refresh(&state)
    selection_set(&state,first); selection_set(&state,second,.Toggle)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell)
    root:=shell_build(&shell,{1200,800}); _,frame:=ui.frame(&ctx,root,{}, {1200,800}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    payload:u64=0; node:ui.Node_Id
    for item,i in shell.bindings { if item.component=="SceneTransform" && item.field.path=="/local/position/0" { payload=u64(i+1); node={key=key(key(34,item.component),item.field.path,u64(shell.inspector.entity))}; break } }
    testing.expect(t,payload!=0)
    append(&ctx.actions,ui.Number_Action{node=node,action=u64(Action.Field),payload=payload,value=2,started=true})
    append(&ctx.actions,ui.Number_Action{node=node,action=u64(Action.Field),payload=payload,value=5,finished=true})
    append(&ctx.actions,ui.Click_Action{action=u64(Action.Select),payload=u64(first),button=.Left})
    shell_actions(&shell)
    a,_:=ecs.get_component(&owner.world,first,app.Scene_Transform); b,_:=ecs.get_component(&owner.world,second,app.Scene_Transform)
    testing.expect(t,a.local.position[0]==5 && b.local.position[0]==5 && !shell.field_gesture.active && len(state.selection.entries)==1)
    testing.expect_value(t,len(owner.agent.session.actions),1)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    a,_=ecs.get_component(&owner.world,first,app.Scene_Transform); b,_=ecs.get_component(&owner.world,second,app.Scene_Transform); testing.expect(t,a.local.position[0]==0 && b.local.position[0]==0)
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    a,_=ecs.get_component(&owner.world,first,app.Scene_Transform); b,_=ecs.get_component(&owner.world,second,app.Scene_Transform); testing.expect(t,a.local.position[0]==5 && b.local.position[0]==5)
}
