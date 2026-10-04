//! Multiselection follows visible hierarchy order and survives document identity remapping.
package editor_app

import app ".."
import ecs "../../ecs"

selection_contains :: proc(state:^State,entity:ecs.Entity_Id)->bool {
    for item in state.selection.entries { if item.entity==entity { return true } }
    return false
}
@(private="package")
selection_add :: proc(state:^State,entity:ecs.Entity_Id) {
    if !selectable(state,entity) || selection_contains(state,entity) { return }
    item:=Selected_Entity{entity=entity}
    if key,present:=ecs.get_component(&state.owner.world,entity,app.Scene_Key); present { item.key=key.value; item.has_key=true }
    append(&state.selection.entries,item)
}
/// Clearing selection also removes the range anchor; no fabricated entity represents absence.
selection_clear :: proc(state:^State) {
    clear(&state.selection.entries); state.selection.has_primary=false; state.selection.has_anchor=false
}
/// Range selection uses mounted rows, including the anchor and the activated row.
selection_set :: proc(state:^State,entity:ecs.Entity_Id,mode:Selection_Mode=.Replace)->bool {
    if !selectable(state,entity) { return false }
    selected:=&state.selection
    switch mode {
    case .Replace:
        clear(&selected.entries); selection_add(state,entity)
        selected.anchor=entity; selected.has_anchor=true
    case .Toggle:
        removed:=false
        for item,i in selected.entries {
            if item.entity==entity { ordered_remove(&selected.entries,i); removed=true; break }
        }
        if !removed { selection_add(state,entity) }
        selected.anchor=entity; selected.has_anchor=true
    case .Range:
        first,last:=-1,-1
        for row,i in state.rows {
            if selected.has_anchor && row.entity==selected.anchor { first=i }
            if row.entity==entity { last=i }
        }
        clear(&selected.entries)
        if first<0 || last<0 {
            selection_add(state,entity); selected.anchor=entity; selected.has_anchor=true
        } else {
            low,high:=min(first,last),max(first,last)
            for row in state.rows[low:high+1] { selection_add(state,row.entity) }
        }
    }
    selected.primary=entity; selected.has_primary=selection_contains(state,entity)
    if !selected.has_primary && len(selected.entries)>0 { selected.primary=selected.entries[len(selected.entries)-1].entity; selected.has_primary=true }
    return true
}
/// Resolves persistent keys after scene replacement; stale actions still keep their original IDs.
selection_refresh :: proc(state:^State) {
    selected:=&state.selection
    ids:=ecs.entity_ids(&state.owner.world); defer delete(ids)
    primary:=selected.primary; has_primary:=selected.has_primary
    for i:=0;i<len(selected.entries); {
        item:=&selected.entries[i]
        previous:=item.entity
        if !selectable(state,item.entity) && item.has_key {
            for id in ids {
                if !selectable(state,id) { continue }
                key,present:=ecs.get_component(&state.owner.world,id,app.Scene_Key)
                if present && key.value==item.key { item.entity=id; break }
            }
        }
        if !selectable(state,item.entity) { ordered_remove(&selected.entries,i); continue }
        if has_primary && previous==primary { selected.primary=item.entity }
        if selected.has_anchor && previous==selected.anchor { selected.anchor=item.entity }
        duplicate:=false
        for earlier in selected.entries[:i] { if earlier.entity==item.entity { duplicate=true; break } }
        if duplicate { ordered_remove(&selected.entries,i) } else { i+=1 }
    }
    selected.has_primary=has_primary && selection_contains(state,selected.primary)
    if !selected.has_primary && len(selected.entries)>0 { selected.primary=selected.entries[0].entity; selected.has_primary=true }
    selected.has_anchor=selected.has_anchor && selectable(state,selected.anchor)
}
/// Reveals every valid ancestor of an external viewport pick without looping through corrupt cycles.
selection_reveal :: proc(state:^State,entity:ecs.Entity_Id) {
    if !selectable(state,entity) { return }
    visited:=make(map[ecs.Entity_Id]bool,state.allocator); defer delete(visited)
    current:=entity
    for {
        if visited[current] { break }; visited[current]=true
        parent,present:=ecs.get_component(&state.owner.world,current,app.Scene_Parent)
        if !present || !selectable(state,parent.entity) { break }
        state.expanded[parent.entity]=true; current=parent.entity
    }
}
