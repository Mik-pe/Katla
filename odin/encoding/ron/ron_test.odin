#+test
package ron

import "core:testing"
import "core:encoding/json"
import "core:os"

@(test)
test_current_mesh_and_prefab_fixtures_preserve_tuple_and_variant_syntax :: proc(t:^testing.T) {
    for path in ([2]string{"resources/meshes/chair-frame.katmesh","resources/prefabs/chair.katprefab"}) {
        bytes,read_error:=os.read_entire_file(path,context.allocator); testing.expect(t,read_error==nil); if read_error!=nil { continue }; defer delete(bytes)
        value,err:=parse(string(bytes)); defer json.destroy_value(value)
        testing.expect_value(t,err.kind,Error_Kind.None)
        if err.kind!=.None { continue }
        object,ok:=value.(json.Object); testing.expect(t,ok && object["version"].(json.Integer)==1)
        if path=="resources/prefabs/chair.katprefab" {
            scene:=object["scene"].(json.Object); entities:=scene["entities"].(json.Array)
            source:=entities[1].(json.Object)["source"].(json.Object)
            testing.expect_value(t,source["__variant"].(string),"MeshAsset")
            payload:=source["__payload"].(json.Object); reference:=payload["path"].(json.Object)
            testing.expect_value(t,reference["__variant"].(string),"Resource")
        }
    }
}

@(test)
test_duplicate_fields_truncated_input_and_unclosed_comments_fail_without_leaks :: proc(t:^testing.T) {
    for text in ([5]string{`(version:1,version:2)`,`(a:[1,2)`,`/* never closed`,`(a:"unterminated)`,`#![enable(unknown)] (a:1)`}) {
        value,err:=parse(text); defer json.destroy_value(value); testing.expect_value(t,err.kind,Error_Kind.Syntax)
    }
    value,err:=parse(`/* outer /* nested */ comment */ (value:Some("quoted\ntext"),items:[1,2,3,],)`); defer json.destroy_value(value)
    testing.expect_value(t,err.kind,Error_Kind.None)
}
