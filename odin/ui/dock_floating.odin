//! Floating dock roots retain tab identity and participate in the same transactional registry.
package ui
import "core:mem"
import "core:encoding/json"

@(private="package")
dock_float_bounds_valid :: proc(bounds:Rect)->bool {
    return finite(bounds.x) && finite(bounds.y) && abs(bounds.x)<=1e7 && abs(bounds.y)<=1e7 && finite(bounds.width) && finite(bounds.height) && bounds.width>=120 && bounds.height>=60 && bounds.width<=1e7 && bounds.height<=1e7
}
@(private="package")
dock_floating_index :: proc(tree:^Dock_Tree,root:Dock_Id)->(int,bool) { for floating,index in tree.floating { if floating.root==root { return index,true } }; return 0,false }
@(private="package")
dock_collapse_all :: proc(tree:^Dock_Tree) {
    dock_collapse(tree,tree.root)
    for i:=len(tree.floating)-1;i>=0;i-=1 {
        root:=tree.floating[i].root; dock_collapse(tree,root)
        if tree.nodes[root].kind==.Empty { dock_delete_node(tree,root); ordered_remove(&tree.floating,i) }
    }
}
@(private="package")
dock_undock_unchecked :: proc(tree:^Dock_Tree,action:Dock_Action)->Dock_Error {
    if !dock_float_bounds_valid(action.bounds) { return .Invalid_Bounds }
    source,present:=tree.nodes[action.source]; if !present || source.kind!=.Leaf { return .Not_Leaf }
    index:=-1; for tab,i in source.tabs { if tab==action.tab { index=i; break } }; if index<0 { return .Invalid_Tab }
    if floating_index,is_floating:=dock_floating_index(tree,source.id); is_floating && len(source.tabs)==1 {
        floating:=tree.floating[floating_index]; floating.bounds=action.bounds; ordered_remove(&tree.floating,floating_index); append(&tree.floating,floating); return .None
    }
    if len(tree.floating)>=128 { return .Invalid_Snapshot }
    ordered_remove(&source.tabs,index); source.active=clamp(source.active,0,max(0,len(source.tabs)-1)); if len(source.tabs)==0 { source.kind=.Empty }
    floating:=dock_new(tree,.Leaf); append(&floating.tabs,action.tab); append(&tree.floating,Dock_Floating{floating.id,action.bounds}); dock_collapse_all(tree); return .None
}
/// Returns one main or floating subtree for a scoped Dock_Space descriptor.
dock_subtree_bounds :: proc(tree:^Dock_Tree,root:Dock_Id,rect:Rect,tab_height:f32=30,gap:f32=4,allocator:mem.Allocator=context.allocator)->[]Dock_Bounds {
    id:=root; if id==0 { id=tree.root }; list:=make([dynamic]Dock_Bounds,allocator); defer delete(list)
    if tree.nodes[id]!=nil {
        bounds:=rect; floating_root:Dock_Id
        if index,present:=dock_floating_index(tree,id); present { bounds=tree.floating[index].bounds; floating_root=id }
        dock_collect_bounds(tree,id,bounds,tab_height,gap,&list); for &region in list { region.floating_root=floating_root }
    }
    result:=make([]Dock_Bounds,len(list),allocator); copy(result,list[:]); return result
}
@(private="package")
dock_json_number :: proc(value:json.Value)->(f32,bool) { if number,ok:=value.(f64); ok { return f32(number),finite(f32(number)) }; if integer,ok:=value.(i64); ok { return f32(integer),finite(f32(integer)) }; return 0,false }
@(private="package")
dock_decode_layout :: proc(tree:^Dock_Tree,value:json.Value,codec:Dock_Tab_Codec,seen:^map[Tab_Id]bool)->Dock_Error {
    object,valid:=value.(json.Object); if !valid { return .Invalid_Snapshot }
    root_value:=value; floating:json.Array
    if version,versioned:=object["version"]; versioned {
        revision,revision_ok:=version.(i64); if !revision_ok || revision!=2 { return .Invalid_Snapshot }
        root_present:bool; root_value,root_present=object["root"]; if !root_present { return .Invalid_Snapshot }
        floats_ok:bool; floating,floats_ok=object["floating"].(json.Array); if !floats_ok || len(floating)>128 { return .Invalid_Snapshot }
    }
    root,error:=dock_decode_node(tree,root_value,codec,seen,0); if error!=.None { return error }; tree.root=root
    for item in floating {
        entry,entry_ok:=item.(json.Object); values,bounds_ok:=entry["bounds"].(json.Array); if !entry_ok || !bounds_ok || len(values)!=4 { return .Invalid_Snapshot }
        coordinates:[4]f32; for coordinate,i in values { number,number_ok:=dock_json_number(coordinate); if !number_ok { return .Invalid_Bounds }; coordinates[i]=number }
        bounds:=Rect{coordinates[0],coordinates[1],coordinates[2],coordinates[3]}; if !dock_float_bounds_valid(bounds) { return .Invalid_Bounds }
        node,node_error:=dock_decode_node(tree,entry["root"],codec,seen,0); if node_error!=.None { return node_error }; if tree.nodes[node].kind==.Empty { return .Invalid_Snapshot }
        append(&tree.floating,Dock_Floating{node,bounds})
    }; return .None
}
