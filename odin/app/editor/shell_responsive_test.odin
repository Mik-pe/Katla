package editor_app
import app ".."
import ui "../../ui"
import prefs "../preferences"
import document "../document"
import "core:testing"
import "core:math"

responsive_measure :: proc(_:rawptr,_:ui.Font_Id,value:string,size,wrap:f32)->ui.Vec2 { advance:=f32(len(value))*size/2;return {min(advance,wrap) if wrap>0 else advance,max(1,math.ceil(advance/wrap) if wrap>0 else 1)*size} }

@(test)
test_console_small_panels_keep_level_search_and_scrolled_input_bounds :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    state:State;state_init(&state,&owner);defer state_destroy(&state)
    ctx:ui.Context;ui.context_init(&ctx,{measure=responsive_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    shell:Shell;shell_init(&shell,&state,&ctx,nil);defer shell_destroy(&shell)
    preferences:=prefs.defaults();defer prefs.destroy(&preferences);shell.preferences=&preferences
    for scale in ([2]f32{1,1.5}) { for width in ([3]f32{180,240,400}) {
        shell_frame_destroy(&shell);shell_console_text(&shell,{action=u64(Action.Console_Search)},"");ui.state_set(&ctx,ui.state(&ctx,key(80,"search"),0,""),string(""));preferences.font_scale=scale;shell.panel_size={width,400};shell_appearance(&shell)
        root:=shell_console(&shell);scale_descriptor(&root,scale)
        _,result:=ui.frame(&ctx,root,{}, {width,400});testing.expect_value(t,result.error,ui.Frame_Error.None)
        for name in ([5]string{"Error","Warn","Info","Debug","Trace"}) { node,present:=ctx.nodes[key(80,name)];if !testing.expect(t,present) {continue};testing.expect(t,node.bounds.width>0 && node.bounds.x>=0 && node.bounds.x+node.bounds.width<=width+.01) }
        search:=ctx.nodes[key(80,"search")];clear_button:=ctx.nodes[key(1,"Clear",u64(Action.Console_Clear))]
        testing.expect(t,search.bounds.width>0 && search.bounds.x+search.bounds.width<=width+.01 && clear_button.bounds.x+clear_button.bounds.width<=width+.01)
        point:=ui.Vec2{search.bounds.x+search.bounds.width/2,search.bounds.y+search.bounds.height/2}
        _,result=ui.frame(&ctx,root,{events={ui.Pointer_Down{position=point,button=.Left},ui.Pointer_Up{position=point,button=.Left},ui.Text_Commit{text="warning"},ui.Key_Down{key=.Enter}}},{width,400});testing.expect_value(t,result.error,ui.Frame_Error.None)
        shell_actions(&shell);testing.expect_value(t,shell.console.search,string("warning"))
        ui.actions_clear(&ctx)
    } }
}

@(test)
test_document_small_window_retains_margin_and_scrollable_long_message :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    state:State;state_init(&state,&owner);defer state_destroy(&state)
    ctx:ui.Context;ui.context_init(&ctx,{measure=responsive_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme});defer ui.context_destroy(&ctx)
    doc:document.State;document.init(&doc,&owner);defer document.destroy(&doc);doc.dialog=.Error
    shell:Shell;shell_init(&shell,&state,&ctx,&doc);defer shell_destroy(&shell)
    preferences:=prefs.defaults();defer prefs.destroy(&preferences);preferences.font_scale=1.5;shell.preferences=&preferences;shell_appearance(&shell)
    root:=shell_document(&shell,{240,180});scale_descriptor(&root,1.5)
    _,result:=ui.frame(&ctx,root,{}, {240,180});testing.expect_value(t,result.error,ui.Frame_Error.None)
    modal:=ctx.nodes[key(40,"dialog")];scroll:=ctx.nodes[key(40,"scroll")]
    testing.expect(t,modal.bounds.x>=8 && modal.bounds.y>=8 && modal.bounds.x+modal.bounds.width<=232 && modal.bounds.y+modal.bounds.height<=172)
    testing.expect(t,scroll.content.height>scroll.bounds.height)
    ui.actions_clear(&ctx)
}
