//! Optional reflection, reversible scene tools and agent sessions.
package editor

import ecs "../ecs"
import "core:mem"
import "core:reflect"
import "core:strings"
import "core:strconv"
import "core:slice"
import "core:encoding/json"
import "base:runtime"




Field_Kind :: enum { Unknown, Float, Int, Bool, String, Color, Struct, Enum, Vec, Entity_Ref }
/// Carries optional numeric range, drag speed and visibility metadata.
Field_Constraints :: struct { min,max,speed:f32, has_min,has_max,skip:bool }
/// Describes one reflected component field and its inspection constraints.
Field_Info :: struct { name,display_name:string, T:typeid, kind:Field_Kind, constraints:Field_Constraints, variants:[]string }
/// Reports reflected scene mutation or application-boundary failures.
Scene_Error :: enum { None, Entity_Not_Found, Component_Not_Found, Field_Not_Found, Invalid_Field_Value, Application_Owned, Decode_Failed, Invalid_Operation, Protected_Entity, Editing_Required }
/// Owns a registered component default and reflected decoding metadata.
Editor_Entry :: struct {
    name:string, T:typeid, default_value:rawptr, ops:ecs.Value_Ops,
    fields:[dynamic]Field_Info,
    decode:proc([]byte,mem.Allocator)->(rawptr,bool),
}
/// Maps application-selected component names to reflected operations.
Component_Registry :: struct { entries:map[string]^Editor_Entry, allocator:mem.Allocator }
/// Initializes the separately imported editor component registry.
editor_registry_init :: proc(reg:^Component_Registry,allocator:=context.allocator) {
    reg.allocator=allocator; reg.entries=make(map[string]^Editor_Entry,allocator)
}
/// Releases registry defaults and metadata using the captured allocator.
editor_registry_destroy :: proc(reg:^Component_Registry) {
    context.allocator=reg.allocator
    for _,entry in reg.entries {
        if entry.ops.destroy!=nil { entry.ops.destroy(entry.default_value) }
        mem.free(entry.default_value,reg.allocator)
        delete(entry.fields); delete(entry.name,reg.allocator); free(entry,reg.allocator)
    }
    delete(reg.entries); reg^={}
}
@(private="package")
owns_memory :: proc(T:typeid)->bool {
    ti:=reflect.type_info_base(type_info_of(T))
    #partial switch info in ti.variant {
    case runtime.Type_Info_String,runtime.Type_Info_Dynamic_Array,runtime.Type_Info_Map: return true
    case runtime.Type_Info_Array: return owns_memory(info.elem.id)
    case runtime.Type_Info_Struct:
        for i in 0..<int(info.field_count) { if owns_memory(info.types[i].id) { return true } }
    }
    return false
}
/// Replaces the Component derive with RTTI and inspect tags; registry owns default_value.
editor_register :: proc(w:^ecs.World,reg:^Component_Registry,name:string,default_value:$T,ops:=ecs.Value_Ops{}) {
    assert(reg.entries[name]==nil)
    if owns_memory(T) { assert(ops.clone!=nil && ops.destroy!=nil,"owned editor values require ownership hooks") }
    existing,registered:=ecs.component_ops(w,T)
    if !registered { ecs.register_component(w,T,ops) }
    else { assert(existing.destroy==ops.destroy && existing.clone==ops.clone) }
    entry:=new(Editor_Entry,reg.allocator)
    entry.name=strings.clone(name,reg.allocator); entry.T=T; entry.ops=ops
    entry.default_value=allocate(size_of(T),align_of(T),reg.allocator)
    owned:=default_value
    mem.copy(entry.default_value,rawptr(&owned),size_of(T))
    entry.fields=make([dynamic]Field_Info,reg.allocator)
    for f in reflect.struct_fields_zipped(T) {
        metadata:=Field_Info{name=f.name,display_name=f.name,T=f.type.id}
        tag:=reflect.struct_tag_get(f.tag,"inspect")
        metadata.constraints.skip=strings.contains(tag,"skip")
        if display:=reflect.struct_tag_get(f.tag,"display_name"); display!="" { metadata.display_name=display }
        if v:=reflect.struct_tag_get(f.tag,"min"); v!="" { n,ok:=strconv.parse_f32(v); metadata.constraints.min=n; metadata.constraints.has_min=ok }
        if v:=reflect.struct_tag_get(f.tag,"max"); v!="" { n,ok:=strconv.parse_f32(v); metadata.constraints.max=n; metadata.constraints.has_max=ok }
        if v:=reflect.struct_tag_get(f.tag,"speed"); v!="" { n,_:=strconv.parse_f32(v); metadata.constraints.speed=n }
        base:=reflect.type_info_base(f.type)
        #partial switch info in base.variant {
        case runtime.Type_Info_Float: metadata.kind=.Float
        case runtime.Type_Info_Integer: metadata.kind=.Int
        case runtime.Type_Info_Boolean: metadata.kind=.Bool
        case runtime.Type_Info_String: metadata.kind=.String
        case runtime.Type_Info_Struct: metadata.kind=.Struct
        case runtime.Type_Info_Enum: metadata.kind=.Enum; metadata.variants=info.names
        case runtime.Type_Info_Array,runtime.Type_Info_Dynamic_Array: metadata.kind=.Vec
        }
        if strings.contains(tag,"color") { metadata.kind=.Color }
        if f.type.id==ecs.Entity_Id || strings.contains(tag,"entity_ref") { metadata.kind=.Entity_Ref }
        append(&entry.fields,metadata)
    }
    entry.decode=proc(data:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
        context.allocator=allocator
        value:=new(T,allocator)
        err:=json.unmarshal(data,value,spec=.JSON,allocator=allocator)
        return value,err==nil
    }
    reg.entries[entry.name]=entry
}
/// Returns borrowed field metadata and an owned, sorted list of registered names.
editor_fields :: proc(reg:^Component_Registry,name:string)->[]Field_Info {
    entry:=reg.entries[name]; if entry==nil { return nil }; return entry.fields[:]
}
editor_type_names :: proc(reg:^Component_Registry)->[dynamic]string {
    names:=make([dynamic]string,reg.allocator)
    for name,_ in reg.entries { append(&names,name) }
    slice.sort(names[:]); return names
}
@(private="package")
editor_add_default :: proc(w:^ecs.World,id:ecs.Entity_Id,entry:^Editor_Entry) {
    context.allocator=w.allocator
    ti:=type_info_of(entry.T)
    copy:=allocate(ti.size,ti.align,w.allocator)
    defer mem.free(copy,w.allocator)
    if entry.ops.clone!=nil { entry.ops.clone(copy,entry.default_value) } else { mem.copy(copy,entry.default_value,ti.size) }
    ecs.insert_component_value(w,id,entry.T,copy)
}
/// Serializes the current component using Odin's standard JSON implementation.
editor_component_json :: proc(w:^ecs.World,id:ecs.Entity_Id,entry:^Editor_Entry)->([]byte,Scene_Error) {
    p:=ecs.component_address(w,id,entry.T)
    if p==nil { return nil,.Component_Not_Found }
    data,err:=json.marshal(any{p,entry.T},allocator=w.allocator)
    if err!=nil { return nil,.Decode_Failed }
    return data,.None
}
@(private="package")
editor_restore :: proc(w:^ecs.World,id:ecs.Entity_Id,entry:^Editor_Entry,data:[]byte)->Scene_Error {
    context.allocator=w.allocator
    value,ok:=entry.decode(data,w.allocator)
    defer mem.free(value,w.allocator)
    if !ok {
        if entry.ops.destroy!=nil { entry.ops.destroy(value) }
        return .Decode_Failed
    }
    if !ecs.insert_component_value(w,id,entry.T,value) {
        if entry.ops.destroy!=nil { entry.ops.destroy(value) }
        return .Entity_Not_Found
    }
    return .None
}
/// Validates field metadata, merges JSON and decodes before replacing the live value.
editor_set_field :: proc(w:^ecs.World,reg:^Component_Registry,id:ecs.Entity_Id,component,field:string,data:[]byte)->Scene_Error {
    context.allocator=w.allocator
    if !ecs.entity_exists(w,id) { return .Entity_Not_Found }
    entry:=reg.entries[component]; if entry==nil { return .Component_Not_Found }
    found:=false
    for f in entry.fields { if f.name==field && !f.constraints.skip { found=true; break } }
    if !found { return .Field_Not_Found }
    snapshot,err:=editor_component_json(w,id,entry); if err!=.None { return err }; defer delete(snapshot)
    object,parse_err:=json.parse(snapshot,spec=.JSON,parse_integers=true)
    if parse_err!=.None { return .Decode_Failed }; defer json.destroy_value(object)
    value,value_err:=json.parse(data,spec=.JSON,parse_integers=true)
    if value_err!=.None { return .Invalid_Field_Value }
    fields,ok:=object.(json.Object)
    if !ok { json.destroy_value(value); return .Invalid_Field_Value }
    old,exists:=fields[field]
    if !exists { json.destroy_value(value); return .Field_Not_Found }
    json.destroy_value(old)
    fields[field]=value
    merged,marshal_err:=json.marshal(object)
    if marshal_err!=nil { return .Invalid_Field_Value }; defer delete(merged)
    result:=editor_restore(w,id,entry,merged)
    if result==.Decode_Failed { return .Invalid_Field_Value }
    return result
}

Scene_Op_Kind :: enum { Spawn, Destroy, Set_Field, Query_Entities, Get_Hierarchy, Duplicate, List_Components, Add_Component, Remove_Component, Get_Attributes, Set_Parent, Spawn_Model, Application }
/// Describes one CPU scene request; variable data is borrowed by synchronous execution.
Scene_Op :: struct {
    kind:Scene_Op_Kind, entity,parent:ecs.Entity_Id, has_parent:bool,
    component,field,name,path,tool_name:string, value:[]byte,
    position,rotation:[3]f32, scale:[3]f32,
    limit:int,
}
Component_Snapshot :: struct { name:string, data:[]byte }
/// Owns affected IDs, JSON output and its allocation policy.
Tool_Result :: struct { error:Scene_Error, entities:[dynamic]ecs.Entity_Id, data:[]byte, allocator:mem.Allocator }
tool_result_destroy :: proc(result:^Tool_Result) {
    delete(result.entities)
    if result.data!=nil { delete(result.data,result.allocator) }
    result^={}
}
@(private="package")
snapshot_entity :: proc(w:^ecs.World,reg:^Component_Registry,id:ecs.Entity_Id)->[dynamic]Component_Snapshot {
    snapshots:=make([dynamic]Component_Snapshot,w.allocator)
    names:=editor_type_names(reg); defer delete(names)
    for name in names {
        data,err:=editor_component_json(w,id,reg.entries[name])
        if err==.None { append(&snapshots,Component_Snapshot{name,data}) }
    }
    return snapshots
}
/// Executes CPU-owned scene operations and returns an owned result and undo group.
scene_execute :: proc(w:^ecs.World,reg:^Component_Registry,op:Scene_Op)->(Tool_Result,Undo_Group) {
    context.allocator=w.allocator
    result:=Tool_Result{entities=make([dynamic]ecs.Entity_Id,w.allocator),allocator=w.allocator}
    command:=Entity_Command{entity=op.entity,allocator=w.allocator}
    group:Undo_Group
    mutation:=op.kind in bit_set[Scene_Op_Kind]{.Spawn,.Destroy,.Set_Field,.Duplicate,.Add_Component,.Remove_Component,.Set_Parent}
    if op.kind in (bit_set[Scene_Op_Kind]{.Spawn_Model,.Set_Parent,.Application}) {
        result.error=.Application_Owned
        return result,group
    }
    if mutation && op.kind!=.Spawn {
        if !ecs.entity_exists(w,op.entity) { result.error=.Entity_Not_Found; return result,group }
        if op.kind!=.Duplicate { command.before_exists=true; command.before=snapshot_entity(w,reg,op.entity) }
    }
    switch op.kind {
    case .Spawn:
        command.entity=ecs.create_entity(w)
        for _,entry in reg.entries {
            editor_add_default(w,command.entity,entry)
            for axis,i in ([3]string{"x","y","z"}) {
                data,_:=json.marshal(op.position[i]); editor_set_field(w,reg,command.entity,entry.name,axis,data); delete(data)
            }
            for axis,i in ([3]string{"scale_x","scale_y","scale_z"}) {
                data,_:=json.marshal(op.scale[i]); editor_set_field(w,reg,command.entity,entry.name,axis,data); delete(data)
            }
            for axis,i in ([3]string{"rot_x","rot_y","rot_z"}) {
                data,_:=json.marshal(op.rotation[i]); editor_set_field(w,reg,command.entity,entry.name,axis,data); delete(data)
            }
            if op.name!="" {
                data,_:=json.marshal(op.name); editor_set_field(w,reg,command.entity,entry.name,"name",data); delete(data)
            }
        }
    case .Destroy: ecs.destroy_entity(w,op.entity)
    case .Set_Field: result.error=editor_set_field(w,reg,op.entity,op.component,op.field,op.value)
    case .Duplicate:
        snapshots:=snapshot_entity(w,reg,op.entity)
        defer { for s in snapshots { delete(s.data) }; delete(snapshots) }
        command.entity=ecs.create_entity(w)
        for s in snapshots { err:=editor_restore(w,command.entity,reg.entries[s.name],s.data); if err!=.None { result.error=err; break } }
    case .Add_Component,.Remove_Component:
        entry:=reg.entries[op.component]
        if entry==nil { result.error=.Component_Not_Found; break }
        if op.kind==.Add_Component { editor_add_default(w,op.entity,entry) }
        else if !ecs.remove_component_type(w,op.entity,entry.T) { result.error=.Component_Not_Found }
    case .Query_Entities,.Get_Hierarchy:
        ids:=ecs.entity_ids(w); defer delete(ids)
        entry:=reg.entries[op.component]
        if op.component!="" && entry==nil { result.error=.Component_Not_Found; break }
        for id in ids {
            if entry!=nil && ecs.component_address(w,id,entry.T)==nil { continue }
            if op.limit>0 && len(result.entities)>=op.limit { break }
            append(&result.entities,id)
        }
    case .List_Components:
        names:=editor_type_names(reg); defer delete(names)
        result.data,_=json.marshal(names[:])
    case .Get_Attributes:
        entry:=reg.entries[op.component]
        if entry==nil { result.error=.Component_Not_Found; break }
        result.data,result.error=editor_component_json(w,op.entity,entry)
    case .Set_Parent,.Spawn_Model,.Application:
    }
    if mutation && result.error==.None {
        command.after_exists=ecs.entity_exists(w,command.entity)
        if command.after_exists { command.after=snapshot_entity(w,reg,command.entity) }
        append(&result.entities,command.entity)
        state:=new(Entity_Command,w.allocator); state^=command
        group=undo_group_create(state,{entity_command_apply,entity_command_destroy,entity_command_remap},{command.entity},w.allocator)
    }
    if result.error!=.None { entity_snapshots_destroy(&command) }
    return result,group
}

@(private="package")
allocate :: proc(size,alignment:int,allocator:mem.Allocator)->rawptr {
    p,err:=mem.alloc(max(size,1),alignment,allocator); assert(err==nil); return p
}
@(private="package")
address :: proc(p:rawptr,offset:int)->rawptr { return rawptr(uintptr(p)+uintptr(offset)) }
