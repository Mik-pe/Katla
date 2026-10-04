#+test
#+build darwin, linux
package editor_app

import app ".."
import assets "../assets"
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_material_browser_image_drag_capture_and_independent_apply :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-material-shell-*",context.allocator);if !testing.expect(t,error==nil) {return};defer {os.remove_all(directory);delete(directory)}
    root:=strings.concatenate({directory,"/resources"});defer delete(root);testing.expect(t,os.make_directory(root)==nil)
    path:=strings.concatenate({root,"/source.png"});defer delete(path);bytes:=#load("../render/texture_image_fixtures/rgba.png",[]byte);testing.expect(t,os.write_entire_file(path,bytes)==nil)
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);testing.expect(t,app.authoring_services_init(&owner)==.None && app.asset_resources_init(&owner,directory,root)==.None)
    ids:[2]ecs.Entity_Id
    for &id in ids {geometry,geometry_error:=app.mesh_cube({1,1,1});assert(geometry_error==.None);id=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,id,app.Scene_Mesh{geometry=geometry,source={kind=.Geometry,geometry=transmute([]byte)strings.clone(`{"kind":"cube","size":[1,1,1]}`)}});ecs.add_component(&owner.world,id,app.Scene_Transform{local={rotation={0,0,0,1},scale={1,1,1}}});ecs.add_component(&owner.world,id,app.Surface_Material{roughness=.5,ao=1})}
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,ids[0])
    browser:assets.State;assets.init(&browser,&owner);defer assets.destroy(&browser);testing.expect(t,assets.refresh(&browser)==.None)
    ctx:ui.Context;ui.context_init(&ctx,{measure=responsive_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    shell:Shell;shell_init(&shell,&state,&ctx,nil);defer shell_destroy(&shell);shell.browser=&browser;shell.panel_size={320,600};shell.inspector,_=inspector_read(&state)
    info,error_info:=app.material_inspector_read(&owner,ids[0]);shell.material_info=info;testing.expect_value(t,error_info,editor.Scene_Error.None);shell.material_info_ready=error_info==.None
    testing.expect(t,info.uv_available==([2]bool{true,false}))
    ui.state_set(&ctx,ui.state(&ctx,key(120,"expanded"),0,false),true)
    material:=shell_material_textures(&shell);material.layout.width=ui.pixels(320)
    asset:=shell_assets(&shell);asset.layout.width=ui.pixels(240)
    descriptor:=ui.Descriptor{key=777,kind=.Row,layout={width=ui.percent(1),height=ui.percent(1)},children=nodes(&shell,{asset,material})}
    _,frame_error:=ui.frame(&ctx,descriptor,{}, {560,600});testing.expect_value(t,frame_error.error,ui.Frame_Error.None)
    source:=ctx.nodes[key(51,"source.png",0)];target:=ctx.nodes[key(121,"role",0)]
    start:=ui.Vec2{source.bounds.x+source.bounds.width/2,source.bounds.y+source.bounds.height/2};finish:=ui.Vec2{target.bounds.x+target.bounds.width/2,target.bounds.y+target.bounds.height/2}
    _,frame_error=ui.frame(&ctx,descriptor,{events={ui.Pointer_Down{position=start,button=.Left}}},{560,600});testing.expect_value(t,frame_error.error,ui.Frame_Error.None);shell_actions(&shell)
    _,frame_error=ui.frame(&ctx,descriptor,{events={ui.Pointer_Move{position=finish},ui.Pointer_Up{position=finish,button=.Left}}},{560,600});testing.expect_value(t,frame_error.error,ui.Frame_Error.None);shell_actions(&shell)
    testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    assigned,present:=ecs.get_component(&owner.world,ids[0],app.Material_Images);if !testing.expect(t,present && assigned.roles[0].image.width==2 && assigned.roles[0].image.height==2) {return}
    digest:=assigned.roles[0].digest
    material_after,_:=ecs.get_component(&owner.world,ids[0],app.Surface_Material);testing.expect(t,material_after.roughness==.5 && !material_after.has_sampling)
    previous_actions:=len(owner.agent.session.actions)
    covered:=ui.Descriptor{key=778,kind=.Stack,layout={width=ui.percent(1),height=ui.percent(1)},children=nodes(&shell,{descriptor,ui.Descriptor{key=779,kind=.Stack,layer=.Overlay,has_fixed_bounds=true,fixed_bounds=target.bounds}})}
    _,frame_error=ui.frame(&ctx,covered,{}, {560,600});testing.expect_value(t,frame_error.error,ui.Frame_Error.None)
    assets.select(&browser,"source.png");shell.asset_drag=assets.drag_batch(&browser)
    testing.expect(t,!shell_material_drop(&shell,finish));assets.drag_batch_destroy(&shell.asset_drag)
    testing.expect_value(t,len(owner.agent.session.actions),previous_actions)
    shell_material_asset_apply(&shell,"resources/materials/copied.katmat",true);testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    selection_set(&state,ids[1]);shell_material_asset_apply(&shell,"resources/materials/copied.katmat");testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    copied,copied_present:=ecs.get_component(&owner.world,ids[1],app.Material_Images);testing.expect(t,copied_present && copied.roles[0].digest==digest)
    shell_material_property(&shell,"roughness",.2);testing.expect_value(t,state.last_error,editor.Scene_Error.None)
    original,_:=ecs.get_component(&owner.world,ids[0],app.Surface_Material);changed,_:=ecs.get_component(&owner.world,ids[1],app.Surface_Material);testing.expect(t,original.roughness==.5 && changed.roughness==.2)
    testing.expect(t,os.write_entire_file(path,"corrupt replacement")==nil)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None);testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None);testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    copied,_=ecs.get_component(&owner.world,ids[1],app.Material_Images);testing.expect(t,copied.roles[0].digest==digest)
}
