package assets

import "core:testing"
import ecs "../../ecs"

@(test)
test_material_asset_exact_targets_and_discriminated_arguments :: proc(t:^testing.T) {
    data:=`{"action":"apply","path":"resources/materials/a.katmat","entity_ids":["0","18446744073709551615"]}`
    decoded,error:=material_asset_decode(transmute([]byte)data)
    if !testing.expect(t,error==.None) { return }; defer material_asset_destroy(&decoded)
    testing.expect(t,decoded.request.entities[0]==ecs.Entity_Id(0) && decoded.request.entities[1]==ecs.Entity_Id(max(u64)))
    for text in ([8]string{
        `{"action":"apply","path":"a.katmat","entity_ids":["18446744073709551616"]}`,
        `{"action":"apply","path":"a.katmat","entity_ids":["01","1"]}`,
        `{"action":"apply","path":"a.katmat","entity_ids":[]}`,
        `{"action":"capture","path":"a.katmat","entity_id":1}`,
        `{"action":"capture","path":"a.katmat","entity_id":"-1"}`,
        `{"action":"read","path":"a.katmat","document":{}}`,
        `{"action":"write","path":"a.katmat","document":null}`,
        `{"action":"describe","path":"a.katmat"}`,
    }) {
        invalid,invalid_error:=material_asset_decode(transmute([]byte)text); testing.expect(t,invalid_error!=.None)
        if invalid_error==.None { material_asset_destroy(&invalid) }
    }
}
