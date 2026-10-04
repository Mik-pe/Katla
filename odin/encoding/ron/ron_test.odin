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

@(test)
test_unsigned_integer_roundtrip_preserves_full_document_range :: proc(t:^testing.T) {
    for text in ([2]string{"9223372036854775808","18446744073709551615"}) {
        tree,err:=parse(text); defer json.destroy_value(tree); testing.expect_value(t,err.kind,Error_Kind.None)
        number,valid:=uint_read(tree); testing.expect(t,valid && number>u64(max(i64)))
        bytes,write_error:=write(tree); defer delete(bytes); testing.expect(t,write_error.kind==.None && string(bytes)==text)
        json_bytes,json_error:=write_json(tree); defer delete(json_bytes); testing.expect(t,json_error.kind==.None && string(json_bytes)==text)
        again,parse_error:=parse_json(json_bytes); defer json.destroy_value(again); second,second_valid:=uint_read(again); testing.expect(t,parse_error.kind==.None && second_valid && number==second)
    }
    for text in ([3]string{"18446744073709551616","-9223372036854775809","01"}) { tree,err:=parse_json(transmute([]byte)text); defer json.destroy_value(tree); testing.expect_value(t,err.kind,Error_Kind.Syntax) }
}
