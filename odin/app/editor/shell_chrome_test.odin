#+test
package editor_app
import app ".."
import ecs "../../ecs"
import prefs "../preferences"
import ui "../../ui"
import render "../render"
import "core:testing"

@(test)
test_workspace_assets_start_and_icon_actions_keep_compact_bounds :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell)
    preferences:=prefs.defaults(); defer prefs.destroy(&preferences); shell.preferences=&preferences
    assets_leaf:=shell.dock.nodes[dock_leaf(&shell,.Assets)]
    testing.expect(t,assets_leaf.tabs[assets_leaf.active]==ui.Tab_Id(Panel.Assets))
    for scale in ([3]f32{1,1.5,2}) {
        preferences.font_scale=scale
        for size in ([3]ui.Vec2{{640,480},{1200,800},{1440,1000}}) {
            root:=shell_build(&shell,size); _,result:=ui.frame(&ctx,root,{},size); testing.expect_value(t,result.error,ui.Frame_Error.None)
            play:=ctx.nodes[key(1,"Play",u64(Action.Play))]
            testing.expect(t,play.descriptor.icon!=0 && play.descriptor.icon_font==render.UI_FONT_ICONS && play.bounds.width<=32*scale+.01 && play.bounds.height<=30*scale+.01 && play.bounds.x+play.bounds.width<=size.x)
            file:=ctx.nodes[key(1,"File",u64(Action.Menu_File))]; shell.menu=.Menu_File
            root=shell_build(&shell,size); _,result=ui.frame(&ctx,root,{},size); testing.expect_value(t,result.error,ui.Frame_Error.None)
            popup:=ctx.nodes[key(1,"menu")]
            testing.expect(t,popup.bounds.x>=0 && popup.bounds.y>=0 && popup.bounds.x+popup.bounds.width<=size.x && popup.bounds.y+popup.bounds.height<=size.y)
            testing.expect(t,popup.bounds.y>=file.bounds.y+file.bounds.height)
            shell.menu=.None; ui.actions_clear(&ctx)
        }
    }
}

@(test)
test_transform_rows_retain_every_axis_inside_the_inspector :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,app.Scene_Transform{local={position={1,2,3},rotation={0,0,0,1},scale={1,1,1}}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,entity)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell)
    preferences:=prefs.defaults(); defer prefs.destroy(&preferences); shell.preferences=&preferences
    for scale in ([2]f32{1,1.5}) {
        preferences.font_scale=scale
        root:=shell_build(&shell,{1440,1000}); _,result:=ui.frame(&ctx,root,{}, {1440,1000}); testing.expect_value(t,result.error,ui.Frame_Error.None)
        panel:=ctx.nodes[key(30,"panel")]; count:=0
        for binding in shell.bindings {
            if binding.component!="SceneTransform" { continue }
            node:=ctx.nodes[key(key(34,binding.component),binding.field.path,u64(entity))]
            testing.expect(t,node.bounds.width>0 && node.bounds.x>=panel.bounds.x && node.bounds.x+node.bounds.width<=panel.bounds.x+panel.bounds.width && node.clip.width>=node.bounds.width-.01)
            count+=1
        }
        testing.expect_value(t,count,10); ui.actions_clear(&ctx)
    }
}
