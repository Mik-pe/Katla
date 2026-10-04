package editor_app
import app ".."
import agent "../../agent"
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"
import km "../../math"
import prefs "../preferences"
import "core:testing"
import "core:strings"
import "core:fmt"

@(test)
test_material_extended_controls_group_multiselection_and_preserve_images :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);app.scene_components_register(&owner)
    first:=ecs.create_entity(&owner.world);second:=ecs.create_entity(&owner.world)
    for id in ([2]ecs.Entity_Id{first,second}) {
        ecs.add_component(&owner.world,id,app.Surface_Material{linear_color=km.Color{.5,.4,.3,1},has_tint=true,roughness=.5,ao=1})
        ecs.add_component(&owner.world,id,app.Texture_Assignments{normal={kind=.Neutral}})
    }
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,first);selection_set(&state,second,.Toggle)
    ctx:ui.Context;ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    shell:Shell;shell_init(&shell,&state,&ctx,nil);defer shell_destroy(&shell)
    shell.inspector,_=inspector_read(&state)
    shell_material_change(&shell,{payload=7,value=2,started=true})
    shell_material_change(&shell,{payload=7,value=4,finished=true})
    testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    testing.expect_value(t,len(owner.agent.session.actions),1)
    for id in ([2]ecs.Entity_Id{first,second}) { material,_:=ecs.get_component(&owner.world,id,app.Surface_Material);testing.expect(t,material.surface.emissive_factor==([3]f32{4,0,0}) && material.surface.normal_scale==1);images,_:=ecs.get_component(&owner.world,id,app.Texture_Assignments);testing.expect(t,images.normal.kind==.Neutral) }
    shell_material_change(&shell,{payload=10,value= -1.5,started=true,finished=true})
    shell_material_property(&shell,"alpha_mode",string("mask"));shell_material_property(&shell,"double_sided",true)
    testing.expect_value(t,len(owner.agent.session.actions),4)
    material,_:=ecs.get_component(&owner.world,second,app.Surface_Material)
    values:=app.material_values(material);testing.expect(t,values.normal_scale== -1.5 && values.alpha_mode==agent.Material_Alpha_Mode.Mask && values.double_sided)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    material,_=ecs.get_component(&owner.world,first,app.Surface_Material);testing.expect(t,!material.surface.double_sided && material.surface.alpha_mode==.Mask)
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    material,_=ecs.get_component(&owner.world,first,app.Surface_Material);testing.expect(t,material.surface.double_sided)
}

@(test)
test_material_sampling_gesture_preserves_role_policy_and_readonly_interleaving :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    first:=ecs.create_entity(&owner.world);second:=ecs.create_entity(&owner.world)
    for id in ([2]ecs.Entity_Id{first,second}) { geometry,error:=app.mesh_cube({1,1,1});assert(error==.None);ecs.add_component(&owner.world,id,app.Scene_Mesh{geometry=geometry,source={kind=.Geometry,geometry=transmute([]byte)strings.clone(`{"kind":"cube","size":[1,1,1]}`)}});ecs.add_component(&owner.world,id,app.Surface_Material{roughness=.5,ao=1});ecs.add_component(&owner.world,id,app.Texture_Assignments{normal={kind=.Neutral}}) }
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,first);selection_set(&state,second,.Toggle)
    ctx:ui.Context;ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    shell:Shell;shell_init(&shell,&state,&ctx,nil);defer shell_destroy(&shell);shell.inspector,_=inspector_read(&state)
    owner.before_mutation_state=&shell;owner.before_mutation=shell_before_mutation
    shell_material_texture_click(&shell,{action=u64(Action.Material_Role),payload=1})
    shell_material_sampling_number(&shell,{node={key=987},payload=0,value=.25,started=true})
    testing.expect_value(t,state.last_error,editor.Scene_Error.None);testing.expect(t,shell.sampling_gesture.scene.active)
    observation:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Query_Entities},app.authoring_executor(&owner))
    testing.expect_value(t,observation.result.error,editor.Scene_Error.None)
    shell_material_sampling_number(&shell,{node={key=987},payload=0,value=.75,finished=true})
    testing.expect_value(t,state.last_error,editor.Scene_Error.None);testing.expect(t,!shell.sampling_gesture.scene.active)
    for id in ([2]ecs.Entity_Id{first,second}) { material,_:=ecs.get_component(&owner.world,id,app.Surface_Material);testing.expect(t,material.sampling.normal.uv.offset==([2]f32{.75,0}) && material.sampling.albedo.uv.offset==([2]f32{0,0}));images,_:=ecs.get_component(&owner.world,id,app.Texture_Assignments);testing.expect(t,images.normal.kind==.Neutral) }
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    material,_:=ecs.get_component(&owner.world,first,app.Surface_Material);testing.expect(t,!material.has_sampling)
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    shell_material_sampling_number(&shell,{node={key=988},payload=5,value=8,started=true,finished=true})
    testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    material,_=ecs.get_component(&owner.world,first,app.Surface_Material);testing.expect(t,material.sampling.normal.sampler.max_anisotropy==8 && material.sampling.normal.sampler.min_filter==.Linear && material.sampling.normal.sampler.mag_filter==.Linear)
    shell_material_filter(&shell,{action=u64(Action.Material_Filter),payload=0,index=0})
    testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    material,_=ecs.get_component(&owner.world,second,app.Surface_Material);testing.expect(t,material.sampling.normal.sampler.max_anisotropy==1 && material.sampling.normal.sampler.min_filter==.Nearest)
    preferences:=prefs.defaults();defer prefs.destroy(&preferences);shell.preferences=&preferences
    input_index:=0
    for scale in ([2]f32{1,1.5}) { for width in ([2]f32{180,320}) {
        expected:=f32(-1.25)-f32(input_index)*.25;input_index+=1
        entered:=fmt.aprintf("%g",expected);defer delete(entered)
        shell_frame_destroy(&shell);shell.inspector,_=inspector_read(&state);info,info_error:=app.material_inspector_read(&owner,shell.inspector.entity);testing.expect_value(t,info_error,editor.Scene_Error.None);shell.material_info=info;shell.material_info_ready=info_error==.None
        shell_material_texture_click(&shell,{action=u64(Action.Material_Role),payload=1})
        preferences.font_scale=scale;shell.panel_size={width,300};shell_appearance(&shell);ui.state_set(&ctx,ui.state(&ctx,key(120,"expanded"),0,false),true)
        content:=shell_material_textures(&shell);root:=ui.Descriptor{key=989,kind=.Scroll_Area,layout={width=ui.percent(1),height=ui.percent(1)},children=nodes(&shell,{content})};scale_descriptor(&root,scale)
        _,frame:=ui.frame(&ctx,root,{}, {width,300});testing.expect_value(t,frame.error,ui.Frame_Error.None)
        identity:=key(key(124,"sampling",u64(shell.inspector.entity)*5+u64(shell_material_role(&shell))),"Offset U");node,present:=ctx.nodes[identity];if !testing.expect(t,present) {continue}
        testing.expect(t,node.bounds.width>0 && node.bounds.x>=0 && node.bounds.x+node.bounds.width<=width+.01)
        shell_material_role(&shell)
        _,frame=ui.frame(&ctx,root,{events={ui.Scroll{position={width/2,150},delta={0,node.bounds.y-70}}}},{width,300});testing.expect_value(t,frame.error,ui.Frame_Error.None);shell_actions(&shell)
        node=ctx.nodes[identity];point:=ui.Vec2{node.bounds.x+node.bounds.width/2,node.bounds.y+node.bounds.height/2}
        testing.expect(t,inside(node.clip,point))
        shell_material_role(&shell)
        _,frame=ui.frame(&ctx,root,{events={ui.Pointer_Down{position=point,button=.Left},ui.Pointer_Up{position=point,button=.Left},ui.Key_Down{key=.A,modifiers={.Control}},ui.Text_Commit{text=entered},ui.Key_Down{key=.Enter}}},{width,300});testing.expect_value(t,frame.error,ui.Frame_Error.None);shell_actions(&shell)
        testing.expect_value(t,state.last_error,editor.Scene_Error.None)
        material,_=ecs.get_component(&owner.world,first,app.Surface_Material);testing.expect_value(t,material.sampling.normal.uv.offset[0],expected)
    } }
}
