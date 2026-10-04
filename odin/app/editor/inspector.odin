//! Reflected inspector views own encoded values while retaining registry metadata only by reference.
package editor_app

import ecs "../../ecs"
import editor "../../editor"
import ron "../../encoding/ron"
import "core:encoding/json"
import "core:mem"
import "core:fmt"
import "core:strings"
import "core:strconv"
import "core:slice"
import "core:reflect"
import "base:runtime"

Inspector_Field :: struct {
    path,label:string,
    value:[]byte,
    kind:editor.Field_Kind,
    constraints:editor.Field_Constraints,
    variants:[]string,
    variant_values:[]i64,
    depth:int,
}
Inspector_Component :: struct { name:string,fields:[dynamic]Inspector_Field,removable:bool }
Inspector :: struct { entity:ecs.Entity_Id,has_entity:bool,components:[dynamic]Inspector_Component,available:[dynamic]string,allocator:mem.Allocator }

/// Builds only the selected entity's registered values; infrastructure fields marked skip never appear.
inspector_read :: proc(state:^State)->(Inspector,editor.Scene_Error) {
    result:=Inspector{components=make([dynamic]Inspector_Component,state.allocator),available=make([dynamic]string,state.allocator),allocator=state.allocator}
    if !state.selection.has_primary { return result,.None }
    entity:=state.selection.primary
    if !selectable(state,entity) { return result,.Entity_Not_Found }
    result.entity=entity; result.has_entity=true
    context.allocator=state.allocator
    names:=editor.editor_type_names(&state.owner.registry); defer delete(names)
    for name in names {
        entry:=state.owner.registry.entries[name]
        if ecs.component_address(&state.owner.world,entity,entry.T)==nil {
            if entry.inspector_add { append(&result.available,strings.clone(name,state.allocator)) }
            continue
        }
        visible:=false; for field in entry.fields { if !field.constraints.skip { visible=true; break } }
        if !visible && !entry.inspector_remove { continue }
        data,error:=editor.editor_component_json(&state.owner.world,entity,entry)
        if error!=.None { inspector_destroy(&result); return {},error }
        tree,parse_error:=ron.parse_json(data,state.allocator); delete(data,state.owner.world.allocator)
        if parse_error.kind!=.None { inspector_destroy(&result); return {},.Decode_Failed }
        object,valid:=tree.(json.Object)
        if !valid { json.destroy_value(tree); inspector_destroy(&result); return {},.Decode_Failed }
        component:=Inspector_Component{name=strings.clone(name,state.allocator),fields=make([dynamic]Inspector_Field,state.allocator),removable=entry.inspector_remove}
        for metadata in entry.fields {
            if metadata.constraints.skip { continue }
            value,present:=object[metadata.name]; if !present { continue }
            inspector_collect(&component.fields,value,pointer_token(metadata.name,state.allocator),metadata.display_name,metadata,0,state.allocator)
        }
        json.destroy_value(tree)
        if len(component.fields)==0 && !component.removable { delete(component.name,state.allocator); delete(component.fields) }
        else { append(&result.components,component) }
    }
    return result,.None
}
@(private="package")
replace_owned :: proc(value,old,new_value:string,allocator:mem.Allocator)->string {
    result,allocated:=strings.replace_all(value,old,new_value,allocator)
    if !allocated { return strings.clone(result,allocator) }
    return result
}
@(private="package")
pointer_token :: proc(name:string,allocator:mem.Allocator)->string {
    escaped:=replace_owned(name,"~","~0",allocator); defer delete(escaped,allocator)
    final:=replace_owned(escaped,"/","~1",allocator); defer delete(final,allocator)
    return strings.concatenate({"/",final},allocator)
}
@(private="package")
inspector_collect :: proc(fields:^[dynamic]Inspector_Field,value:json.Value,path,label:string,metadata:editor.Field_Info,depth:int,allocator:mem.Allocator) {
    defer delete(path,allocator)
    if depth>32 || metadata.constraints.skip { return }
    if number,valid:=ron.uint_read(value); valid && (number>16_777_216 || metadata.T==ecs.Entity_Id) {
        text:=fmt.aprintf("%d",number,allocator=allocator); defer delete(text,allocator)
        encoded,error:=json.marshal(text,allocator=allocator); if error!=nil { return }
        append(fields,Inspector_Field{path=strings.clone(path,allocator),label=strings.clone(label,allocator),value=encoded,kind=.Int,constraints=metadata.constraints,depth=depth}); return
    }
    switch node in value {
    case json.Object:
        keys:=make([dynamic]string,allocator); defer delete(keys)
        for key in node { append(&keys,key) }; slice.sort(keys[:])
        for key in keys {
            token:=pointer_token(key,allocator)
            child_path:=strings.concatenate({path,token},allocator); delete(token,allocator)
            child_metadata:=metadata; child_label:=key
            if metadata.T!=nil { for field in reflect.struct_fields_zipped(metadata.T) { if field.name==key { child_metadata=editor.editor_field_metadata(field); child_label=child_metadata.display_name; break } } }
            inspector_collect(fields,node[key],child_path,child_label,child_metadata,depth+1,allocator)
        }
    case json.Array:
        if metadata.kind==.Color {
            encoded,error:=json.marshal(value,allocator=allocator); if error!=nil { return }
            append(fields,Inspector_Field{path=strings.clone(path,allocator),label=strings.clone(label,allocator),value=encoded,kind=.Color,constraints=metadata.constraints,depth=depth}); return
        }
        for item,index in node {
            index_text:=fmt.aprintf("%d",index,allocator=allocator)
            token:=pointer_token(index_text,allocator); child_path:=strings.concatenate({path,token},allocator); delete(token,allocator)
            child_metadata:=metadata
            if metadata.T!=nil {
                #partial switch info in reflect.type_info_base(type_info_of(metadata.T)).variant {
                case runtime.Type_Info_Array: child_metadata.T=info.elem.id
                case runtime.Type_Info_Dynamic_Array: child_metadata.T=info.elem.id
                case runtime.Type_Info_Slice: child_metadata.T=info.elem.id
                }
            }
            inspector_collect(fields,item,child_path,index_text,child_metadata,depth+1,allocator); delete(index_text,allocator)
        }
    case json.Integer,json.Float,json.Boolean,json.String,json.Null:
        encoded,error:=json.marshal(value,allocator=allocator)
        if error!=nil { return }
        kind:=metadata.kind
        if depth>0 {
            switch _ in value {
            case json.Integer: kind=.Int
            case json.Float: kind=.Float
            case json.Boolean: kind=.Bool
            case json.String: kind=.String
            case json.Null: kind=.Unknown
            case json.Object,json.Array:
            }
        }
        variants:=metadata.variants
        variant_values:[]i64
        if metadata.T!=nil {
            #partial switch info in reflect.type_info_base(type_info_of(metadata.T)).variant {
            case runtime.Type_Info_Float: kind=.Float
            case runtime.Type_Info_Integer: kind=.Int
            case runtime.Type_Info_Boolean: kind=.Bool
            case runtime.Type_Info_Enum: kind=.Enum; variants=info.names; variant_values=transmute([]i64)info.values
            }
        }
        append(fields,Inspector_Field{path=strings.clone(path,allocator),label=strings.clone(label,allocator),value=encoded,kind=kind,constraints=metadata.constraints,variants=variants,variant_values=variant_values,depth=depth})
    }
}
/// Releases view allocations with the allocator captured when the snapshot was built.
inspector_destroy :: proc(snapshot:^Inspector) {
    for component in snapshot.components {
        delete(component.name,snapshot.allocator)
        for field in component.fields { delete(field.path,snapshot.allocator); delete(field.label,snapshot.allocator); delete(field.value,snapshot.allocator) }
        delete(component.fields)
    }
    for name in snapshot.available { delete(name,snapshot.allocator) }
    delete(snapshot.available); delete(snapshot.components); snapshot^={}
}
/// Merges one nested control value into its actual top-level field before canonical validation/undo.
inspector_operation :: proc(state:^State,entity:ecs.Entity_Id,component,path:string,value:[]byte)->(editor.Scene_Op,editor.Scene_Error) {
    if !selectable(state,entity) { return {},.Entity_Not_Found }
    if len(path)<2 || path[0]!='/' { return {},.Field_Not_Found }
    entry:=state.owner.registry.entries[component]; if entry==nil { return {},.Component_Not_Found }
    context.allocator=state.allocator
    data,error:=editor.editor_component_json(&state.owner.world,entity,entry); if error!=.None { return {},error }; defer delete(data,state.owner.world.allocator)
    tree,parse_error:=ron.parse_json(data,state.allocator); if parse_error.kind!=.None { return {},.Decode_Failed }; defer json.destroy_value(tree)
    parts:=strings.split(path[1:],"/",state.allocator); defer delete(parts,state.allocator)
    decoded:=make([dynamic]string,state.allocator); defer { for part in decoded { delete(part,state.allocator) }; delete(decoded) }
    for part in parts {
        slash:=replace_owned(part,"~1","/",state.allocator)
        token:=replace_owned(slash,"~0","~",state.allocator); delete(slash,state.allocator); append(&decoded,token)
    }
    next,value_error:=ron.parse_json(value,state.allocator); if value_error.kind!=.None { return {},.Invalid_Field_Value }
    if text,is_text:=next.(string); is_text {
        T:=inspector_path_type(entry.T,decoded[:])
        if T!=nil {
            #partial switch info in reflect.type_info_base(type_info_of(T)).variant {
            case runtime.Type_Info_Integer:
                if !info.signed {
                    number,valid:=ron.decimal_u64(text); if !valid { json.destroy_value(next); return {},.Invalid_Field_Value }
                    json.destroy_value(next); next=ron.uint_value(number,state.allocator)
                } else {
                    negative:=strings.has_prefix(text,"-"); digits:=text; if negative { digits=text[1:] }
                    number,valid:=ron.decimal_u64(digits)
                    bound:=u64(max(i64)); if negative { bound+=1 }
                    if !valid || number>bound { json.destroy_value(next); return {},.Invalid_Field_Value }
                    signed:=i64(number); if negative { if number==bound { signed=min(i64) } else { signed=-signed } }
                    json.destroy_value(next); next=json.Integer(signed)
                }
            }
        }
    }
    if !inspector_replace(&tree,decoded[:],next) { json.destroy_value(next); return {},.Field_Not_Found }
    object,valid:=tree.(json.Object); if !valid { return {},.Invalid_Field_Value }
    merged,marshal_error:=ron.write_json(object[decoded[0]],state.allocator); if marshal_error.kind!=.None { return {},.Invalid_Field_Value }
    return {kind=.Set_Field,entity=entity,component=strings.clone(component,state.allocator),field=strings.clone(decoded[0],state.allocator),value=merged},.None
}
/// Executes a prepared nested edit through the same native participant and undo session as tools.
inspector_set :: proc(state:^State,entity:ecs.Entity_Id,component,path:string,value:[]byte)->editor.Scene_Error {
    operation,error:=inspector_operation(state,entity,component,path,value)
    if error!=.None { return error }; defer inspector_operation_destroy(&operation,state.allocator)
    return execute(state,operation)
}
/// Releases the exact strings and value owned by inspector_operation.
inspector_operation_destroy :: proc(operation:^editor.Scene_Op,allocator:=context.allocator) {
    delete(operation.component,allocator); delete(operation.field,allocator); delete(operation.value,allocator); operation^={}
}
@(private="package")
inspector_path_type :: proc(T:typeid,path:[]string)->typeid {
    current:=T
    for token in path {
        found:=false
        #partial switch info in reflect.type_info_base(type_info_of(current)).variant {
        case runtime.Type_Info_Struct:
            for field in reflect.struct_fields_zipped(current) { if field.name==token { current=field.type.id; found=true; break } }
        case runtime.Type_Info_Array: current=info.elem.id; found=true
        case runtime.Type_Info_Slice: current=info.elem.id; found=true
        case runtime.Type_Info_Dynamic_Array: current=info.elem.id; found=true
        }
        if !found { return nil }
    }
    return current
}
@(private="package")
inspector_replace :: proc(current:^json.Value,path:[]string,replacement:json.Value)->bool {
    if len(path)==0 { json.destroy_value(current^); current^=replacement; return true }
    switch node in current^ {
    case json.Object:
        child,present:=node[path[0]]; if !present { return false }
        if !inspector_replace(&child,path[1:],replacement) { return false }
        fields:=node; fields[path[0]]=child; return true
    case json.Array:
        index,valid:=strconv.parse_int(path[0]); if !valid || index<0 || index>=len(node) { return false }
        return inspector_replace(&node[index],path[1:],replacement)
    case json.Integer,json.Float,json.Boolean,json.String,json.Null: return false
    }
    return false
}
