#+test
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"
import "core:testing"

Sparse_Inspector_Choice :: enum i32 { First=-3,Second=9,Third=41 }
Sparse_Inspector_Component :: struct { choice:Sparse_Inspector_Choice }
@(test)
test_numeric_sparse_enum_pointer_choice_preserves_wire_and_shared_undo :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);app.authoring_services_init(&owner)
    editor.editor_register(&owner.world,&owner.registry,"Sparse",Sparse_Inspector_Component{.First})
    entity:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,entity,Sparse_Inspector_Component{.First})
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,entity)
    ctx:ui.Context;ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    shell:Shell;shell_init(&shell,&state,&ctx,nil);defer shell_destroy(&shell)
    shell_build(&shell,{1200,800});root:=shell_inspector(&shell);_,frame:=ui.frame(&ctx,root,{}, {400,800});testing.expect_value(t,frame.error,ui.Frame_Error.None)
    id:=key(key(34,"Sparse"),"/choice",u64(entity));node:=ctx.nodes[id]
    testing.expect(t,node!=nil&&node.descriptor.kind==.Combo);if node==nil {return}
    position:=ui.Vec2{node.bounds.x+20,node.bounds.y+node.bounds.height/2}
    root=shell_inspector(&shell);ui.frame(&ctx,root,{events={ui.Pointer_Down{position=position},ui.Pointer_Up{position=position}}},{400,800});shell_actions(&shell)
    testing.expect_value(t,ctx.popup,node.id)
    position={node.bounds.x+20,node.bounds.y+node.bounds.height+ctx.theme.row_height*1.5}
    root=shell_inspector(&shell);ui.frame(&ctx,root,{events={ui.Pointer_Down{position=position},ui.Pointer_Up{position=position}}},{400,800});shell_actions(&shell)
    testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    value,_:=ecs.get_component(&owner.world,entity,Sparse_Inspector_Component);testing.expect_value(t,value.choice,Sparse_Inspector_Choice.Second)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    value,_=ecs.get_component(&owner.world,entity,Sparse_Inspector_Component);testing.expect_value(t,value.choice,Sparse_Inspector_Choice.First)
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    value,_=ecs.get_component(&owner.world,entity,Sparse_Inspector_Component);testing.expect_value(t,value.choice,Sparse_Inspector_Choice.Second)
}
