#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:strings"

@(test)
test_record_applied_action_transfers_undo_once_without_reexecuting :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    session:Agent_Session; agent_session_init(&session); defer agent_session_destroy(&session)
    name:=strings.clone("one applied scene action")
    operation:=Scene_Op{kind=.Spawn,name=name}
    result,undo:=scene_execute(&w,&reg,operation)
    action:=agent_record_action(&session,operation,&result,&undo)
    delete(name)
    testing.expect(t,result.entities==nil && result.data==nil && undo.state==nil)
    testing.expect(t,len(session.actions)==1 && w.live_count==1 && action.id==0 && action.operation.name=="one applied scene action")
    testing.expect_value(t,agent_undo_last(&session,&w,&reg),Scene_Error.None)
    testing.expect(t,w.live_count==0 && len(session.actions)==0 && session.next_id==1)
}
