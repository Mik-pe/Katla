#+test
package mcp

import app "../../app"
import ecs "../../ecs"
import "core:testing"
import "core:encoding/json"
import "core:fmt"

@(test)
test_mcp_signed_request_edges_and_unsigned_entity_overflow :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    zero:=ecs.create_entity(&owner.world); testing.expect_value(t,u64(zero),u64(0))
    server:Server; server_init(&server,&owner.agent); defer server_destroy(&server)
    for id in ([]string{"9223372036854775807","-9223372036854775808"}) {
        output:=receive(&server,id,"ping"); tree,valid:=parse_message(output,context.allocator); testing.expect(t,valid)
        if valid { object:=tree.(json.Object); encoded,ok:=request_id(object["id"],context.allocator); testing.expect(t,ok && encoded==id); delete(encoded); json.destroy_value(tree) }; delete(output)
    }
    for id in ([]string{"9223372036854775808","-9223372036854775809","18446744073709551616","18446744073709551617","36893488147419103232"}) {
        output:=receive(&server,id,"ping"); check_error(t,output,-32700); delete(output)
    }
    for id in ([]string{"18446744073709551616","18446744073709551617","36893488147419103232"}) {
        extra:=fmt.aprintf(`,"name":"destroy_entity","arguments":{{"entity_id":"%s"}}`,id)
        output:=receive(&server,`"overflow"`,"tools/call",extra); delete(extra)
        tree,valid:=parse_message(output,context.allocator); testing.expect(t,valid)
        if valid { result:=tree.(json.Object)["result"].(json.Object); testing.expect(t,bool(result["isError"].(json.Boolean))); json.destroy_value(tree) }; delete(output)
        testing.expect(t,owner.agent.outstanding==0 && len(server.pending)==0)
    }
    maximum:=receive(&server,`"18446744073709551615"`,"tools/call",`,"name":"destroy_entity","arguments":{"entity_id":"18446744073709551615"}`)
    testing.expect_value(t,maximum,""); delete(maximum)
    testing.expect_value(t,app.authoring_tick(&owner),1)
    reply:=server_poll(&server,0); tree,valid:=parse_message(reply,context.allocator); testing.expect(t,valid)
    if valid { object:=tree.(json.Object); testing.expect_value(t,object["id"].(string),"18446744073709551615"); result:=object["result"].(json.Object); testing.expect(t,bool(result["isError"].(json.Boolean))); json.destroy_value(tree) }; delete(reply)
    testing.expect(t,ecs.entity_exists(&owner.world,zero) && owner.world.live_count==1 && owner.agent.outstanding==0)
}
