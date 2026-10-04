#+test
package editor_app
import app ".."
import ecs "../../ecs"
import ui "../../ui"
import km "../../math"
import editor "../../editor"
import "core:testing"

@(test)
test_particle_panel_mounts_typed_color_and_burst_active_use_actual_routes :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.TRANSFORM_IDENTITY},{app.particle_defaults()}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,entity)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell); ui.dock_open(&shell.dock,ui.Tab_Id(Panel.Particles))
    root:=shell_build(&shell,{1440,900}); _,frame:=ui.frame(&ctx,root,{}, {1440,900}); testing.expect_value(t,frame.error,ui.Frame_Error.None)
    colors:=0; for binding in shell.bindings { if binding.is_color { colors+=1 } }; testing.expect(t,colors>=4)
    shell_particle_click(&shell,{action=u64(Action.Particle_Burst)})
    emitter,_:=ecs.get_component(&owner.world,entity,app.Particle_Emitter); testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==32 && state.last_error==.None)
    shell_particle_click(&shell,{action=u64(Action.Particle_Active)})
    emitter,_=ecs.get_component(&owner.world,entity,app.Particle_Emitter); testing.expect(t,!emitter.descriptor.active && len(emitter.descriptor.burst_queue)==1)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    emitter,_=ecs.get_component(&owner.world,entity,app.Particle_Emitter); testing.expect(t,emitter.descriptor.active && len(emitter.descriptor.burst_queue)==1)
    ui.actions_clear(&ctx)
}
