#+test
package agent

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:fmt"

@(test)
test_entity_decimal_overflow_never_aliases_live_zero :: proc(t:^testing.T) {
    world:ecs.World; ecs.world_init(&world); defer ecs.world_destroy(&world)
    registry:editor.Component_Registry; editor.editor_registry_init(&registry); defer editor.editor_registry_destroy(&registry)
    zero:=ecs.create_entity(&world); testing.expect_value(t,u64(zero),u64(0))
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox); defer editor.agent_harness_destroy(&mailbox)
    for value in ([]string{"18446744073709551616","18446744073709551617","36893488147419103232","000000000000000000000","99999999999999999999"}) {
        _,valid:=parse_entity_id(value); testing.expect(t,!valid,value)
        arguments:=fmt.aprintf(`{{"entity_id":"%s"}}`,value)
        ticket,error:=submit_call(&mailbox,{"overflow","destroy_entity",transmute([]byte)arguments}); delete(arguments)
        testing.expect(t,error==.Invalid_Arguments && ticket==0 && mailbox.outstanding==0)
        material:=fmt.aprintf(`{{"action":"set","entity_ids":["%s"],"metallic":1}}`,value)
        ticket,error=submit_call(&mailbox,{"overflow-material","material",transmute([]byte)material}); delete(material)
        testing.expect(t,error==.Invalid_Arguments && ticket==0 && mailbox.outstanding==0)
    }
    testing.expect(t,editor.agent_tick(&mailbox,&world,&registry)==0 && ecs.entity_exists(&world,zero) && world.live_count==1)
    maximum,valid:=parse_entity_id("18446744073709551615"); testing.expect(t,valid && u64(maximum)==max(u64))
    decoded,error:=decode_call({"max","destroy_entity",transmute([]byte)string(`{"entity_id":"18446744073709551615"}`)})
    testing.expect(t,error==.None && u64(decoded.operation.entity)==max(u64)); if error==.None { decoded_call_destroy(&decoded) }
}
