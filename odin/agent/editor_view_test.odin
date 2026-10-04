#+test
package agent

import "core:testing"
import ecs "../ecs"

@(test)
test_editor_view_exact_ids_clear_selection_and_finite_camera :: proc(t:^testing.T) {
    for text in ([]string{`{"action":"select","entity_id":"0"}`,`{"action":"focus","entity_id":"18446744073709551615","select":true}`}) {
        request,err:=editor_view_decode(transmute([]byte)text); testing.expect(t,err==.None && request.has_entity)
        testing.expect(t,request.entity==0 || request.entity==ecs.Entity_Id(max(u64)))
    }
    request,err:=editor_view_decode(transmute([]byte)string(`{"action":"select","entity_id":null}`))
    testing.expect(t,err==.None && !request.has_entity)
    request,err=editor_view_decode(transmute([]byte)string(`{"action":"observe"}`)); testing.expect(t,err==.None && request.limit==64)
    for example in ([]struct {text:string,limit:int}{{`{"action":"observe","limit":null}`,64},{`{"action":"observe","limit":0}`,0},{`{"action":"observe","limit":257}`,256},{`{"action":"observe","limit":18446744073709551615}`,256}}) {
        view,error:=editor_view_decode(transmute([]byte)example.text); testing.expect(t,error==.None && view.limit==example.limit)
    }
    for text in ([]string{`{"action":"select","entity_id":"18446744073709551616"}`,`{"action":"select","entity_id":0}`,`{"action":"focus","entity_id":null}`,`{"action":"observe","limit":18446744073709551680}`,`{"action":"set_camera","position":[0,0,0],"target":[0,0,0]}`,`{"action":"set_camera","position":[0,1e999,0],"target":[0,0,1]}`,`{"action":"observe","unexpected":true}`}) {
        _,error:=editor_view_decode(transmute([]byte)text); testing.expect(t,error!=.None,text)
    }
    decoded,call_error:=decode_call({"view-id","editor_view",transmute([]byte)string(`{"action":"select","entity_id":null}`)}); defer decoded_call_destroy(&decoded)
    testing.expect(t,call_error==.None && decoded.operation.kind==.Application && decoded.operation.tool_name=="editor_view")
}
