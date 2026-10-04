//! Bounded cloning retains parsed scalar tags without serializing internal representations.
package ron

import "core:encoding/json"
import "core:strings"

/// Deep-clones owned parsed values; the caller destroys the result with the selected allocator.
clone_value :: proc(value:json.Value,allocator:=context.allocator)->(json.Value,Error) {
    context.allocator=allocator; nodes:=0; bytes:=0; return clone_value_inner(value,0,&nodes,&bytes)
}
@(private="package")
clone_value_inner :: proc(value:json.Value,depth:int,nodes,bytes:^int)->(json.Value,Error) {
    nodes^+=1; if depth>128 || nodes^>8_000_000 || bytes^>64*1024*1024 { return nil,{.Limit,0} }
    #partial switch tree in value {
    case string:
        bytes^+=len(tree); if bytes^>64*1024*1024 { return nil,{.Limit,0} }; return strings.clone(tree),{}
    case json.Object:
        object:=make(json.Object,context.allocator); success:=false; defer { if !success { json.destroy_value(object) } }
        for key,item in tree {
            bytes^+=len(key); child,error:=clone_value_inner(item,depth+1,nodes,bytes); if error.kind!=.None { return nil,error }
            object[strings.clone(key)]=child
        }
        success=true; return object,{}
    case json.Array:
        array:=make(json.Array,0,context.allocator); success:=false; defer { if !success { json.destroy_value(array) } }
        for child in tree { cloned,error:=clone_value_inner(child,depth+1,nodes,bytes); if error.kind!=.None { return nil,error }; append(&array,cloned) }
        success=true; return array,{}
    }
    return value,{}
}
