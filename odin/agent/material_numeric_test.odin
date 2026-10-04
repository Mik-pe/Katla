#+test
package agent

import "core:testing"

@(test)
test_material_large_numeric_tokens_cannot_wrap_into_valid_factors :: proc(t:^testing.T) {
    for text in ([]string{`{"action":"set","entity_ids":["0"],"metallic":18446744073709551616}`,`{"action":"set","entity_ids":["0"],"roughness":18446744073709551617}`,`{"action":"set","entity_ids":["0"],"base_color":[0,1,18446744073709551616,1]}`,`{"action":"set","entity_ids":["0"],"ao":1e999}`}) {
        decoded,error:=decode_material(transmute([]byte)text)
        testing.expect(t,error!=.None)
        if error==.None { decoded_material_destroy(&decoded) }
    }
    valid:string=`{"action":"set","entity_ids":["18446744073709551615"],"base_color":null,"roughness":1,"metallic":0}`
    decoded,error:=decode_material(transmute([]byte)valid); testing.expect(t,error==.None)
    if error==.None {
        operation:=decoded.operation.(Material_Set)
        testing.expect(t,u64(operation.entities[0])==max(u64) && operation.fields=={.Roughness,.Metallic} && operation.values.roughness==1 && operation.values.metallic==0)
        decoded_material_destroy(&decoded)
    }
}
