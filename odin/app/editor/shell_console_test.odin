package editor_app
import app ".."
import ecs "../../ecs"
import script "../../script"
import editor "../../editor"
import km "../../math"
import "core:strings"
import "core:testing"

@(test)
test_console_owns_drained_runtime_logs_bounded_history_and_clear_preserves_undo :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,app.Scene_Transform{local=km.TRANSFORM_IDENTITY})
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    shell:=Shell{state=&state,allocator=owner.world.allocator}; console_init(&shell.console,owner.world.allocator); defer console_destroy(&shell.console)
    logs:=make([dynamic]script.Log,owner.world.allocator); defer delete(logs)
    append(&logs,script.Log{0,.Warn,strings.clone("Varning från faktisk runtime",owner.world.allocator)})
    ecs.insert_resource(&owner.world,app.Script_Native_Runtime{logs=logs,allocator=owner.world.allocator})
    testing.expect_value(t,execute(&state,{kind=.Set_Field,entity=entity,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[1,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}),editor.Scene_Error.None)
    shell_console_poll(&shell)
    testing.expect(t,len(shell.console.rows)==2 && shell.console.rows[0].level==.Warn && strings.contains(shell.console.rows[0].message,"Varning"))
    shell_console_poll(&shell); testing.expect_value(t,len(shell.console.rows),2)
    chunk:=strings.repeat("å",600000,owner.world.allocator); defer delete(chunk,owner.world.allocator)
    for _ in 0..<6 { console_append(&shell.console,.Info,chunk) }
    testing.expect(t,shell.console.bytes<=4<<20 && len(shell.console.rows)==4)
    console_clear(&shell.console); testing.expect(t,len(shell.console.rows)==0 && editor.agent_can_undo(&owner.agent.session))
    shell_console_poll(&shell); testing.expect_value(t,len(shell.console.rows),0)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
}
