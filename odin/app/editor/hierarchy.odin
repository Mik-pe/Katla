//! A compact owned hierarchy snapshot supports collapsed ancestors and search descendants.
package editor_app

import app ".."
import ecs "../../ecs"
import "core:fmt"
import "core:strings"

Hierarchy_Row :: struct { entity:ecs.Entity_Id,name:string,depth:int,has_children,expanded,selected,orphan:bool }
@(private="package")
Hierarchy_Node :: struct { entity:ecs.Entity_Id,name:string,parent:int,children:[dynamic]int,matched:bool }
@(private="package")
Hierarchy_Visit :: struct { index,depth:int }
@(private="package")
hierarchy_clear :: proc(state:^State) {
    for row in state.rows { delete(row.name,state.allocator) }
    clear(&state.rows)
}
/// Children matching a search stay discoverable even when their parent was collapsed.
hierarchy_refresh :: proc(state:^State) {
    hierarchy_clear(state); selection_refresh(state)
    nodes:=make([dynamic]Hierarchy_Node,state.allocator)
    defer { for node in nodes { delete(node.name,state.allocator); delete(node.children) }; delete(nodes) }
    indices:=make(map[ecs.Entity_Id]int,state.allocator); defer delete(indices)
    ids:=ecs.entity_ids(&state.owner.world); defer delete(ids)
    needle:=strings.to_lower(state.search,state.allocator); defer delete(needle,state.allocator)
    for id in ids {
        if !selectable(state,id) { continue }
        label:=fmt.aprintf("Entity %d",u64(id),allocator=state.allocator)
        if name,present:=ecs.get_component(&state.owner.world,id,app.Scene_Name); present && name.name!="" {
            delete(label,state.allocator); label=strings.clone(name.name,state.allocator)
        }
        folded:=strings.to_lower(label,state.allocator)
        matched:=needle=="" || strings.contains(folded,needle); delete(folded,state.allocator)
        indices[id]=len(nodes)
        append(&nodes,Hierarchy_Node{entity=id,name=label,parent=-1,children=make([dynamic]int,state.allocator),matched=matched})
    }
    for &node,i in nodes {
        parent,present:=ecs.get_component(&state.owner.world,node.entity,app.Scene_Parent)
        if present {
            if index,known:=indices[parent.entity]; known { node.parent=index; append(&nodes[index].children,i) }
        }
    }
    if needle!="" {
        for node in nodes {
            if !node.matched { continue }
            visited:=make(map[int]bool,state.allocator)
            parent:=node.parent
            for parent>=0 && !visited[parent] { visited[parent]=true; nodes[parent].matched=true; parent=nodes[parent].parent }
            delete(visited)
        }
    }
    visits:=make([dynamic]Hierarchy_Visit,state.allocator); defer delete(visits)
    visited:=make(map[int]bool,state.allocator); defer delete(visited)
    // Roots preserve actual allocator order; sibling order is stable across frames.
    for i:=len(nodes)-1;i>=0;i-=1 { if nodes[i].parent<0 { append(&visits,Hierarchy_Visit{i,0}) } }
    for root:=0;root<len(nodes);root+=1 {
        if len(visits)==0 && !visited[root] { append(&visits,Hierarchy_Visit{root,0}) }
        for len(visits)>0 {
            visit:=pop(&visits)
            if visited[visit.index] { continue }; visited[visit.index]=true
            node:=&nodes[visit.index]
            if !node.matched { continue }
            expanded:=state.expanded[node.entity] || needle!=""
            append(&state.rows,Hierarchy_Row{entity=node.entity,name=strings.clone(node.name,state.allocator),depth=visit.depth,has_children=len(node.children)>0,expanded=expanded,selected=selection_contains(state,node.entity),orphan=visit.depth==0 && node.parent>=0})
            if expanded { for i:=len(node.children)-1;i>=0;i-=1 { append(&visits,Hierarchy_Visit{node.children[i],visit.depth+1}) } }
            else {
                // Mark hidden descendants so the cycle-recovery pass cannot promote collapsed children.
                hidden:=make([dynamic]int,state.allocator); append(&hidden,..node.children[:])
                for len(hidden)>0 { child:=pop(&hidden); if visited[child] { continue }; visited[child]=true; append(&hidden,..nodes[child].children[:]) }
                delete(hidden)
            }
        }
    }
}
