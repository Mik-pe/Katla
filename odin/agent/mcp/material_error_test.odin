#+test
package mcp

import app "../../app"
import "core:encoding/json"
import "core:strings"
import "core:testing"

@(test)
test_material_owner_failure_returns_structured_tool_error :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    server:Server; server_init(&server,&owner.agent); defer server_destroy(&server)
    admitted:=receive(&server,`"missing-material"`,"tools/call",`,"name":"material","arguments":{"action":"inspect","entity_id":"42"}`)
    testing.expect(t,admitted=="" && len(server.pending)==1)
    testing.expect_value(t,app.authoring_tick(&owner),1)
    output:=server_poll(&server,0); defer delete(output)
    tree,error:=json.parse(output); testing.expect(t,error==nil); if error!=nil { return }; defer json.destroy_value(tree)
    root:=tree.(json.Object); response:=root["result"].(json.Object); failure:=response["structuredContent"].(json.Object)
    testing.expect(t,root["id"].(string)=="missing-material" && response["isError"].(bool) && !failure["success"].(bool))
    testing.expect(t,strings.contains(failure["message"].(string),"Entity_Not_Found") && owner.world.live_count==0 && owner.agent.outstanding==0)
    blocks:=response["content"].(json.Array)
    testing.expect(t,blocks[0].(json.Object)["text"].(string)==failure["message"].(string))
}
