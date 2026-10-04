//! Typed cells belong to live generational nodes and own their text allocations.
package ui
import "core:strings"

@(private="package")
node_get :: proc(ctx:^Context,id:Node_Id)->^Node {
    if id.owner!=ctx || id.key==0 { return nil }
    node,present:=ctx.nodes[id.key]
    if !present || node.id!=id { return nil }
    return node
}
@(private="package")
node_ensure :: proc(ctx:^Context,key:u64)->^Node {
    if key==0 || !ctx.initialized || ctx.closed || ctx.generation==max(u64) { return nil }
    if node,present:=ctx.nodes[key]; present { return node }
    ctx.generation+=1
    node:=new(Node,ctx.allocator)
    node.id={ctx,key,ctx.generation}; node.children=make([dynamic]Node_Id,ctx.allocator)
    node.undo=make([dynamic]Text_Snapshot,ctx.allocator); node.redo=make([dynamic]Text_Snapshot,ctx.allocator)
    node.state=make(map[u32]State_Cell,ctx.allocator); ctx.nodes[key]=node
    return node
}
@(private="package")
value_clone :: proc(value:Value,allocator:=context.allocator)->Value {
    switch item in value {
    case string: return strings.clone(item,allocator)
    case bool,f32,i64,Vec2: return value
    }
    return value
}
@(private="package")
value_destroy :: proc(value:Value,allocator:=context.allocator) {
    switch item in value {
    case string: delete(item,allocator)
    case bool,f32,i64,Vec2:
    }
}
/// Reserves a stable explicit hook slot on a node; removal invalidates every previous State_Id.
state :: proc(ctx:^Context,key:u64,slot:u32,initial:Value)->State_Id {
    node:=node_ensure(ctx,key); if node==nil { return {} }
    node.retained_until=ctx.frame_index+1
    if _,present:=node.state[slot]; !present { node.state[slot]={value=value_clone(initial,ctx.allocator)} }
    return {node.id,slot}
}
/// Text returned here is borrowed until that cell changes or its node is removed.
state_get :: proc(ctx:^Context,id:State_Id,$T:typeid)->(T,bool) {
    node:=node_get(ctx,id.node); if node==nil { return {},false }
    cell,present:=node.state[id.slot]; if !present { return {},false }
    return cell.value.(T)
}
/// Replaces a cell only when its existing variant matches, capturing owned text once.
state_set :: proc(ctx:^Context,id:State_Id,value:Value)->bool {
    node:=node_get(ctx,id.node); if node==nil { return false }
    cell,present:=node.state[id.slot]; if !present { return false }
    same_type:=false
    switch old in cell.value {
    case bool: _,same_type=value.(bool)
    case f32: _,same_type=value.(f32)
    case i64: _,same_type=value.(i64)
    case string: _,same_type=value.(string)
    case Vec2: _,same_type=value.(Vec2)
    }
    if !same_type { return false }
    next:=value_clone(value,ctx.allocator); value_destroy(cell.value,ctx.allocator); cell.value=next; cell.dirty=true
    node.state[id.slot]=cell; return true
}
/// Drains one exact action variant; other variants remain available for their owning application consumer.
actions_drain :: proc(ctx:^Context,$T:typeid,allocator:=context.allocator)->[]T {
    result:=make([dynamic]T,allocator); defer delete(result)
    for i:=0;i<len(ctx.actions); {
        if action,present:=ctx.actions[i].(T); present { append(&result,action); ordered_remove(&ctx.actions,i) }
        else { i+=1 }
    }
    owned:=make([]T,len(result),allocator); copy(owned,result[:]); return owned
}
/// Releases undrained actions after the application has processed the current frame.
actions_clear :: proc(ctx:^Context) { clear(&ctx.actions); for text in ctx.action_snapshots { delete(text,ctx.allocator) }; clear(&ctx.action_snapshots) }

@(private="package")
retain_node :: proc(ctx:^Context,node:^Node) {
    node.retained_until=ctx.frame_index+1
    for id in node.children { if child:=node_get(ctx,id); child!=nil { retain_node(ctx,child) } }
}
/// Reserves an existing inactive subtree for the next frame without mounting it or admitting input.
/// Applications call this for their inactive dock panels before frame reconciliation.
retain :: proc(ctx:^Context,key:u64)->bool { node,present:=ctx.nodes[key]; if !present { return false }; retain_node(ctx,node); return true }
@(private="package")
forget_node :: proc(ctx:^Context,node:^Node) {
    for id in node.children { if child:=node_get(ctx,id); child!=nil { forget_node(ctx,child) } }
    if ctx.focused==node.id { ctx.focused={} }; if ctx.captured==node.id { ctx.captured={} }; delete_key(&ctx.nodes,node.id.key); node_destroy(ctx,node)
}
/// Explicitly invalidates a dormant subtree and every previous State_Id it owns.
forget :: proc(ctx:^Context,key:u64)->bool { node,present:=ctx.nodes[key]; if !present || node.mounted { return false }; forget_node(ctx,node); return true }

/// Drains all variants in their original event order; captured text stays owned until actions_clear.
actions_drain_all :: proc(ctx:^Context,allocator:=context.allocator)->[]Action { result:=make([]Action,len(ctx.actions),allocator); copy(result,ctx.actions[:]); clear(&ctx.actions); return result }
