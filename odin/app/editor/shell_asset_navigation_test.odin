#+test
#+build darwin, linux
//! Real retained controls route pointer events into accepted navigation and owned history.
package editor_app
import app ".."
import assets "../assets"
import ui "../../ui"
import "core:testing"
import "core:os"
import "core:strings"

@(private="file")
asset_navigation_frame :: proc(t:^testing.T,shell:^Shell,input:[]ui.Input_Event=nil) {
    shell_frame_destroy(shell); panel:=shell_assets(shell)
    _,result:=ui.frame(shell.ctx,panel,{events=input},{1100,650}); testing.expect_value(t,result.error,ui.Frame_Error.None); shell_actions(shell)
}
@(private="file")
asset_navigation_click :: proc(t:^testing.T,shell:^Shell,control:u64) {
    point:ui.Vec2; found:=false
    node,present:=shell.ctx.nodes[control]; if present { point={node.bounds.x+node.bounds.width*.5,node.bounds.y+node.bounds.height*.5}; found=true }
    testing.expect(t,found); if !found { return }
    asset_navigation_frame(t,shell,{ui.Pointer_Down{position=point},ui.Pointer_Up{position=point}})
}
@(test)
test_asset_navigation_pointer_back_forward_breadcrumbs_and_root_choice_history :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-asset-controls-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for folder in ([2]string{"one","one/two"}) { path:=strings.concatenate({resource,"/",folder}); testing.expect(t,os.make_directory(path)==nil); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect(t,app.asset_resources_init(&owner,directory,resource)==.None)
    browser:assets.State; assets.init(&browser,&owner); defer assets.destroy(&browser); testing.expect(t,assets.refresh(&browser)==.None && assets.navigate(&browser,"one/two")==.None)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:=Shell{state=&state,ctx=&ctx,browser=&browser,allocator=owner.world.allocator}; defer shell_destroy(&shell)
    asset_navigation_frame(t,&shell)
    asset_navigation_click(t,&shell,key(54,"one",u64(browser.root))); testing.expect_value(t,browser.directory,"one")
    asset_navigation_click(t,&shell,key(1,"Back",u64(Action.Asset_Back))); testing.expect_value(t,browser.directory,"one/two")
    asset_navigation_click(t,&shell,key(1,"Forward",u64(Action.Asset_Forward))); testing.expect_value(t,browser.directory,"one")
    asset_navigation_click(t,&shell,key(54,"root",u64(browser.root))); testing.expect_value(t,browser.directory,"")
    shell_service_choice(&shell,{action=u64(Action.Asset_Root),index=1}); testing.expect(t,browser.root==.Project)
    asset_navigation_frame(t,&shell); asset_navigation_click(t,&shell,key(1,"Back",u64(Action.Asset_Back))); testing.expect(t,browser.root==.Resource && browser.directory=="")
    asset_navigation_frame(t,&shell); shown,valid:=ui.state_get(&ctx,ui.state(&ctx,key(50,"root"),0,f32(browser.root)),f32); testing.expect(t,valid && shown==0)
    testing.expect(t,len(owner.agent.session.actions)==0)
}
