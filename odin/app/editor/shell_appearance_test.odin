#+test
package editor_app
import app ".."
import prefs "../preferences"
import ui "../../ui"
import "core:testing"

@(test)
test_applied_appearance_scaled_controls_stats_and_floating_viewport_occlusion :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell)
    preferences:=prefs.defaults(); defer prefs.destroy(&preferences); shell.preferences=&preferences
    prefs.set_theme(&preferences,"light"); preferences.font_scale=2
    root:=shell_build(&shell,{1440,1000}); _,frame:=ui.frame(&ctx,root,{}, {1440,1000}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    testing.expect(t,ctx.theme.text[0]<.2 && ctx.theme.canvas[0]>.9 && ctx.theme.font_size==24)
    search:=ctx.nodes[key(10,"search")]; testing.expect(t,search!=nil && search.bounds.height>=60)
    shell_frame_statistics(&shell,.02,37,9); label:=shell_status(&shell,"Editing"); testing.expect(t,label!="Editing")
    preferences.show_stats=false; testing.expect_value(t,shell_status(&shell,"Editing"),"Editing")
    shell.viewports.has_active=false
    shell_viewport_input(&shell,{events={ui.Pointer_Down{position={shell.viewports.slots[0].bounds.x+20,shell.viewports.slots[0].bounds.y+20},button=.Right}}},{consumed_pointer=true,captured_pointer=true})
    testing.expect(t,!shell.viewports.has_active && !shell.navigation_orbit && !shell.navigation_pan)
    ui.actions_clear(&ctx)
}
