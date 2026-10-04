//! Canonical RON publication keeps asset vector tuples and explicit tagged variants.
package ron

import "core:encoding/json"
import "core:strings"
import "core:slice"
import "core:fmt"

/// Serializes current asset values as deterministic RON; the caller deletes the returned bytes.
write :: proc(value:json.Value,allocator:=context.allocator)->([]byte,Error) {
    context.allocator=allocator
    builder:strings.Builder; strings.builder_init(&builder,allocator); defer strings.builder_destroy(&builder)
    error:=write_value(&builder,value,"",false,0)
    if error.kind!=.None { return nil,error }
    text:=strings.to_string(builder)
    if len(text)>64*1024*1024 { return nil,{.Limit,len(text)} }
    return slice.clone(transmute([]byte)text,allocator),{}
}
@(private="package")
write_identifier :: proc(value:string)->bool {
    if len(value)==0 { return false }
    for character,i in value {
        if character>='a' && character<='z' || character>='A' && character<='Z' || character=='_' { continue }
        if i>0 && character>='0' && character<='9' { continue }; return false
    }
    return true
}
@(private="package")
write_tuple_field :: proc(field:string)->bool {
    for name in ([17]string{"position","rotation","scale","size","color","base_color","velocity","direction","linear_velocity","end_color","end_scale","velocity_direction","velocity_randomness","gravity","normal","uv","half_extents"}) { if field==name { return true } }; return false
}
@(private="package")
write_value :: proc(builder:^strings.Builder,value:json.Value,field:string,tuple:bool,depth:int)->Error {
    if depth>128 { return {.Limit,strings.builder_len(builder^)} }
    if strings.builder_len(builder^)>64*1024*1024 { return {.Limit,strings.builder_len(builder^)} }
    #partial switch data in value {
    case json.Object:
        if number,is_uint:=uint_read(data); is_uint { text:=fmt.aprintf("%d",number); defer delete(text); strings.write_string(builder,text); return {} }
        if name,is_variant:=data[VARIANT_TAG].(string); is_variant {
            if !write_identifier(name) || len(data)>2 { return {.Syntax,strings.builder_len(builder^)} }
            strings.write_string(builder,name)
            if payload,present:=data[PAYLOAD_TAG]; present { return write_value(builder,payload,name,true,depth+1) }
            return {}
        }
        opening,closing:="(",")"; if field=="components" { opening="{"; closing="}" }
        strings.write_string(builder,opening)
        names:=make([dynamic]string,context.allocator); defer delete(names); for name in data { append(&names,name) }; slice.sort(names[:])
        for name in names {
            if write_identifier(name) && field!="components" { strings.write_string(builder,name) }
            else { encoded,err:=json.marshal(name); if err!=nil { return {.Syntax,strings.builder_len(builder^)} }; strings.write_string(builder,string(encoded)); delete(encoded) }
            strings.write_string(builder,":")
            if err:=write_value(builder,data[name],name,write_tuple_field(name),depth+1); err.kind!=.None { return err }
            strings.write_string(builder,",")
        }
        strings.write_string(builder,closing)
    case json.Array:
        opening,closing:="[","]"; if tuple { opening="("; closing=")" }; strings.write_string(builder,opening)
        for element in data {
            child_tuple:=field=="positions" || field=="normals" || field=="uvs" || field=="Box"
            if err:=write_value(builder,element,"",child_tuple,depth+1); err.kind!=.None { return err }; strings.write_string(builder,",")
        }
        strings.write_string(builder,closing)
    case json.Null: strings.write_string(builder,"None")
    case:
        encoded,err:=json.marshal(value); if err!=nil { return {.Syntax,strings.builder_len(builder^)} }; defer delete(encoded)
        strings.write_string(builder,string(encoded))
    }
    return {}
}
