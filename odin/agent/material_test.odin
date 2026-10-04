#+test
package agent

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:fmt"
import "core:encoding/json"

@(test)
test_material_transport_lossless_ids_presets_and_null_patches :: proc(t:^testing.T) {
    decoded,err:=decode_material(transmute([]byte)string(`{"action":"set","entity_ids":["9007199254740993","18446744073709551615"],"preset":"oak","metallic":null,"roughness":0.25}`))
    defer decoded_material_destroy(&decoded)
    testing.expect_value(t,err,Call_Error.None)
    if err!=.None { return }
    set:=decoded.operation.(Material_Set)
    testing.expect(t,u64(set.entities[0])==9007199254740993 && u64(set.entities[1])==max(u64))
    testing.expect(t,set.has_preset && set.preset==.Oak && set.fields=={.Roughness} && set.values.roughness==0.25)
    for preset in Material_Preset { testing.expect(t,material_values_valid(material_preset_values(preset))) }
}

@(test)
test_material_malformed_calls_reject_before_mailbox_mutation :: proc(t:^testing.T) {
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    for args in ([]string{
        `{}`, `{"action":"presets","entity_id":"0"}`, `{"action":"inspect","entity_id":9007199254740993}`,
        `{"action":"set","entity_ids":[]}`, `{"action":"set","entity_ids":["0"]}`,
        `{"action":"set","entity_ids":["1","01"],"roughness":0.2}`,
        `{"action":"set","entity_ids":["18446744073709551616"],"roughness":0.2}`,
        `{"action":"set","entity_ids":["0"],"roughnes":0.2}`,
        `{"action":"set","entity_ids":["0"],"preset":"wood"}`,
        `{"action":"set","entity_ids":["0"],"metallic":null}`,
        `{"action":"set","entity_ids":["0"],"base_color":[1,1,1]}`,
        `{"action":"set","entity_ids":["0"],"base_color":[1,1,1,1.1]}`,
        `{"action":"set","entity_ids":["0"],"roughness":-0.01}`,
        `{"action":"set","entity_ids":["0"],"ao":1e100}`,
    }) {
        ticket,submission_error:=submit_call(&h,{"invalid","material",transmute([]byte)args})
        testing.expect_value(t,submission_error,Call_Error.Invalid_Arguments); testing.expect_value(t,ticket,u64(0))
    }
    testing.expect(t,len(h.requests)==0 && len(h.session.actions)==0)
}

@(test)
test_material_without_application_returns_explicit_boundary :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:editor.Component_Registry; editor.editor_registry_init(&reg); defer editor.editor_registry_destroy(&reg)
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    ticket,submission_error:=submit_call(&h,{"presets","material",transmute([]byte)string(`{"action":"presets"}`)})
    testing.expect_value(t,submission_error,Call_Error.None); testing.expect(t,ticket>0)
    testing.expect_value(t,editor.agent_tick(&h,&w,&reg),1)
    response,ok:=editor.agent_take_result(&h); defer editor.agent_response_destroy(&response)
    testing.expect(t,ok && response.result.error==.Application_Owned && w.live_count==0)
}
@(test)
test_material_transport_batch_limit :: proc(t:^testing.T) {
    ids:[257]string
    for &id,i in ids { id=fmt.aprintf("%d",i) }
    defer { for id in ids { delete(id) } }
    for size in ([2]int{256,257}) {
        data,_:=json.marshal(struct { action:string, entity_ids:[]string, preset:string }{"set",ids[:size],"oak"})
        decoded,err:=decode_material(data)
        testing.expect_value(t,err,Call_Error.None if size==256 else Call_Error.Invalid_Arguments)
        decoded_material_destroy(&decoded); delete(data)
    }
}
