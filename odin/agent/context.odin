//! Scene observation owns JSON snapshots and never exposes World to host threads.
package agent

import ecs "../ecs"
import editor "../editor"
import "core:mem"

/// Counts a registered component among live generational entities.
Component_Count :: struct { type_name:string, count:int }
/// Owns one component's serialized state; type_name borrows the registry.
Component_Context :: struct { type_name:string, data:[]byte }
/// Owns an editor-thread snapshot; registry names outlive the snapshot.
Scene_Context :: struct {
    entity_count:int,
    selected:ecs.Entity_Id,
    has_selection:bool,
    components:[dynamic]Component_Context,
    counts:[dynamic]Component_Count,
    allocator:mem.Allocator,
}
/// Captures all registered counts and an optional live selection in sorted name order.
scene_context :: proc(w:^ecs.World,reg:^editor.Component_Registry,selected:ecs.Entity_Id,has_selection:bool)->Scene_Context {
    result:=Scene_Context{entity_count=w.live_count,selected=selected,has_selection=has_selection && ecs.entity_exists(w,selected),allocator=w.allocator}
    result.components=make([dynamic]Component_Context,w.allocator)
    result.counts=make([dynamic]Component_Count,w.allocator)
    names:=editor.editor_type_names(reg); defer delete(names)
    ids:=ecs.entity_ids(w); defer delete(ids)
    for name in names {
        entry:=reg.entries[name]
        count:=0
        for id in ids { if ecs.component_address(w,id,entry.T)!=nil { count+=1 } }
        if count>0 { append(&result.counts,Component_Count{name,count}) }
        if result.has_selection {
            data,err:=editor.editor_component_json(w,selected,entry)
            if err==.None { append(&result.components,Component_Context{name,data}) }
        }
    }
    return result
}
/// Releases component JSON and observation arrays with their originating allocator.
scene_context_destroy :: proc(snapshot:^Scene_Context) {
    for component in snapshot.components { delete(component.data,snapshot.allocator) }
    delete(snapshot.components); delete(snapshot.counts); snapshot^={}
}
