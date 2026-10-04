//! Lossless JSON publication preserves unsigned integers and converts named asset variants.
package ron

import "core:encoding/json"
import "core:fmt"
import "core:slice"
import "core:strings"

/// Serializes parsed assets as ordinary JSON without rounding unsigned integer values.
write_json :: proc(value:json.Value,allocator:=context.allocator)->([]byte,Error) {
    context.allocator=allocator
    builder:strings.Builder; strings.builder_init(&builder,allocator); defer strings.builder_destroy(&builder)
    if err:=write_json_value(&builder,value,0); err.kind!=.None { return nil,err }
    if strings.builder_len(builder)>64*1024*1024 { return nil,{.Limit,strings.builder_len(builder)} }
    return slice.clone(transmute([]byte)strings.to_string(builder),allocator),{}
}
@(private="package")
write_json_string :: proc(builder:^strings.Builder,text:string)->Error {
    bytes,err:=json.marshal(text); if err!=nil { return {.Syntax,strings.builder_len(builder^)} }; defer delete(bytes); strings.write_string(builder,string(bytes)); return {}
}
@(private="package")
write_json_value :: proc(builder:^strings.Builder,value:json.Value,depth:int)->Error {
    if depth>128 || strings.builder_len(builder^)>64*1024*1024 { return {.Limit,strings.builder_len(builder^)} }
    #partial switch tree in value {
    case json.Object:
        if number,is_uint:=uint_read(tree); is_uint { text:=fmt.aprintf("%d",number); defer delete(text); strings.write_string(builder,text); return {} }
        if name,is_variant:=tree["__variant"].(string); is_variant {
            if len(tree)>2 { return {.Syntax,strings.builder_len(builder^)} }
            payload,present:=tree["__payload"]; if !present { return write_json_string(builder,name) }
            if values,is_values:=payload.(json.Array); is_values && len(values)==1 { payload=values[0] }
            strings.write_string(builder,"{"); if err:=write_json_string(builder,name); err.kind!=.None { return err }; strings.write_string(builder,":")
            if err:=write_json_value(builder,payload,depth+1); err.kind!=.None { return err }; strings.write_string(builder,"}"); return {}
        }
        strings.write_string(builder,"{"); names:=make([dynamic]string); defer delete(names); for name in tree { append(&names,name) }; slice.sort(names[:])
        for name,index in names { if index>0 { strings.write_string(builder,",") }; if err:=write_json_string(builder,name); err.kind!=.None { return err }; strings.write_string(builder,":"); if err:=write_json_value(builder,tree[name],depth+1); err.kind!=.None { return err } }; strings.write_string(builder,"}")
    case json.Array:
        strings.write_string(builder,"["); for child,index in tree { if index>0 { strings.write_string(builder,",") }; if err:=write_json_value(builder,child,depth+1); err.kind!=.None { return err } }; strings.write_string(builder,"]")
    case:
        bytes,err:=json.marshal(value); if err!=nil { return {.Syntax,strings.builder_len(builder^)} }; defer delete(bytes); strings.write_string(builder,string(bytes))
    }
    return {}
}
