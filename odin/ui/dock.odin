//! Dock mutations are transactional, keyed by tab identity and independent of editor panel policy.
package ui
import "core:mem"
import "core:encoding/json"
import "core:strings"
import "core:fmt"
import "core:strconv"

Dock_Tab_Codec :: struct { state:rawptr,encode:proc(rawptr,Tab_Id)->string,decode:proc(rawptr,string)->(Tab_Id,bool) }
@(private="package")
dock_new :: proc(tree:^Dock_Tree,kind:Dock_Kind)->^Dock_Node {
    tree.next_id+=1; node:=new(Dock_Node,tree.allocator); node^={id=Dock_Id(tree.next_id),kind=kind,ratio=0.5,tabs=make([dynamic]Tab_Id,tree.allocator)}; tree.nodes[node.id]=node; return node
}
dock_init :: proc(tree:^Dock_Tree,tabs:[]Tab_Id=nil,allocator:mem.Allocator=context.allocator)->Dock_Error {
    seen:=make(map[Tab_Id]bool,allocator); defer delete(seen)
    for tab in tabs { if tab==0 || seen[tab] { return .Duplicate_Tab }; seen[tab]=true }
    tree^={allocator=allocator,nodes=make(map[Dock_Id]^Dock_Node,allocator),floating=make([dynamic]Dock_Floating,allocator)}
    root:=dock_new(tree,.Leaf if len(tabs)>0 else .Empty); append(&root.tabs,..tabs); tree.root=root.id; return .None
}
dock_destroy :: proc(tree:^Dock_Tree) { for _,node in tree.nodes { delete(node.tabs); free(node,tree.allocator) }; delete(tree.nodes); delete(tree.floating); tree^={} }
@(private="package")
dock_clone :: proc(tree:^Dock_Tree)->Dock_Tree {
    result:=Dock_Tree{root=tree.root,next_id=tree.next_id,allocator=tree.allocator,nodes=make(map[Dock_Id]^Dock_Node,tree.allocator),floating=make([dynamic]Dock_Floating,tree.allocator)}
    append(&result.floating,..tree.floating[:])
    for id,node in tree.nodes { cloned:=new(Dock_Node,tree.allocator); cloned^=node^; cloned.tabs=make([dynamic]Tab_Id,tree.allocator); append(&cloned.tabs,..node.tabs[:]); result.nodes[id]=cloned }; return result
}
@(private="package")
dock_delete_node :: proc(tree:^Dock_Tree,id:Dock_Id) { node:=tree.nodes[id]; delete_key(&tree.nodes,id); delete(node.tabs); free(node,tree.allocator) }
@(private="package")
dock_collapse :: proc(tree:^Dock_Tree,id:Dock_Id) {
    node:=tree.nodes[id]; if node.kind!=.Split { return }
    dock_collapse(tree,node.children[0]); dock_collapse(tree,node.children[1])
    a,b:=tree.nodes[node.children[0]],tree.nodes[node.children[1]]
    if a.kind!=.Empty && b.kind!=.Empty { return }
    survivor,removed:=b,a; if b.kind==.Empty { survivor,removed=a,b }
    delete(node.tabs); old_id:=node.id; node^=survivor^; node.id=old_id
    survivor.tabs=nil; dock_delete_node(tree,survivor.id); dock_delete_node(tree,removed.id)
}
@(private="package")
dock_apply_unchecked :: proc(tree:^Dock_Tree,action:Dock_Action)->Dock_Error {
    source,source_present:=tree.nodes[action.source]; target,target_present:=tree.nodes[action.target]
    switch action.kind {
    case .Undock: return dock_undock_unchecked(tree,action)
    case .Float_Bounds,.Raise:
        index,present:=dock_floating_index(tree,action.target); if !present { return .Invalid_Id }
        if action.kind==.Float_Bounds { if !dock_float_bounds_valid(action.bounds) { return .Invalid_Bounds }; tree.floating[index].bounds=action.bounds }
        floating:=tree.floating[index]; ordered_remove(&tree.floating,index); append(&tree.floating,floating)
    case .Resize:
        if !target_present || target.kind!=.Split { return .Invalid_Id }
        if !finite(action.ratio) { return .Invalid_Ratio }; target.ratio=clamp(action.ratio,0,1)
    case .Insert:
        if action.tab==0 || !target_present || target.kind==.Split { return .Invalid_Tab }
        for _,leaf in tree.nodes { for tab in leaf.tabs { if tab==action.tab { return .Duplicate_Tab } } }
        added:=dock_new(tree,.Leaf); append(&added.tabs,action.tab)
        moved:=action; moved.kind=.Move; moved.source=added.id
        error:=dock_apply_unchecked(tree,moved); if error==.None && tree.nodes[added.id]!=nil { dock_delete_node(tree,added.id) }; return error
    case .Activate,.Close,.Move:
        if !source_present || source.kind!=.Leaf { return .Not_Leaf }
        index:=-1; for tab,i in source.tabs { if tab==action.tab { index=i; break } }; if index<0 { return .Invalid_Tab }
        if action.kind==.Activate { source.active=index; return .None }
        if action.kind==.Move && (!target_present || target.kind==.Split) { return .Not_Leaf }
        if action.kind==.Move && source==target && action.zone!=.Center && len(source.tabs)==1 { return .Invalid_Tab }
        ordered_remove(&source.tabs,index); source.active=clamp(source.active,0,max(0,len(source.tabs)-1)); if len(source.tabs)==0 { source.kind=.Empty }
        if action.kind==.Close { dock_collapse_all(tree); return .None }
        if action.zone==.Center || target.kind==.Empty {
            target.kind=.Leaf; requested:=action.index; if source==target && requested>index { requested-=1 }; at:=clamp(requested,0,len(target.tabs)); append(&target.tabs,action.tab)
            for i:=len(target.tabs)-1;i>at;i-=1 { target.tabs[i]=target.tabs[i-1] }; target.tabs[at]=action.tab; target.active=at
        } else {
            previous:=dock_new(tree,target.kind); saved_id:=previous.id; previous^=target^; previous.id=saved_id
            target.tabs=make([dynamic]Tab_Id,tree.allocator)
            added:=dock_new(tree,.Leaf); append(&added.tabs,action.tab)
            target.kind=.Split; target.ratio=0.5; target.direction=.Horizontal
            if action.zone==.Top || action.zone==.Bottom { target.direction=.Vertical }
            target.children={previous.id,added.id}; if action.zone==.Left || action.zone==.Top { target.children={added.id,previous.id} }
        }
        dock_collapse_all(tree)
    }
    return .None
}
/// The original tree survives every invalid move or ratio; apply drained actions after the UI frame.
dock_apply :: proc(tree:^Dock_Tree,action:Dock_Action)->Dock_Error {
    candidate:=dock_clone(tree); error:=dock_apply_unchecked(&candidate,action)
    if error!=.None { dock_destroy(&candidate); return error }
    dock_destroy(tree); tree^=candidate; return .None
}
@(private="package")
dock_collect_bounds :: proc(tree:^Dock_Tree,id:Dock_Id,rect:Rect,tab_height,gap:f32,result:^[dynamic]Dock_Bounds) {
    node:=tree.nodes[id]
    if node.kind==.Split {
        a,b:=rect,rect
        if node.direction==.Horizontal { a.width=max(0,(rect.width-gap)*node.ratio); b.x=rect.x+a.width+gap; b.width=max(0,rect.width-a.width-gap) }
        else { a.height=max(0,(rect.height-gap)*node.ratio); b.y=rect.y+a.height+gap; b.height=max(0,rect.height-a.height-gap) }
        append(result,Dock_Bounds{node=id,bounds=rect}); dock_collect_bounds(tree,node.children[0],a,tab_height,gap,result); dock_collect_bounds(tree,node.children[1],b,tab_height,gap,result)
    } else {
        height:=min(tab_height,rect.height); item:=Dock_Bounds{node=id,bounds=rect,content={rect.x,rect.y+height,rect.width,max(0,rect.height-height)},tab_bar={rect.x,rect.y,rect.width,height}}
        if node.kind==.Leaf && len(node.tabs)>0 { item.active=node.tabs[clamp(node.active,0,len(node.tabs)-1)]; item.has_active=true }; append(result,item)
    }
}
/// Caller owns the snapshot; split ratios are evaluated in each split's local bounds.
dock_bounds :: proc(tree:^Dock_Tree,rect:Rect,tab_height:f32=30,gap:f32=4,allocator:mem.Allocator=context.allocator)->[]Dock_Bounds {
    list:=make([dynamic]Dock_Bounds,allocator); defer delete(list)
    if tree.nodes[tree.root]!=nil { dock_collect_bounds(tree,tree.root,rect,tab_height,gap,&list) }
    for floating in tree.floating { start:=len(list); dock_collect_bounds(tree,floating.root,floating.bounds,tab_height,gap,&list); for &region in list[start:] { region.floating_root=floating.root } }
    result:=make([]Dock_Bounds,len(list),allocator); copy(result,list[:]); return result
}
@(private="package")
dock_encode_node :: proc(tree:^Dock_Tree,id:Dock_Id,codec:Dock_Tab_Codec,builder:^strings.Builder) {
    node:=tree.nodes[id]
    switch node.kind {
    case .Empty: strings.write_string(builder,"{\"type\":\"Empty\"}")
    case .Split:
        direction:="Horizontal"; if node.direction==.Vertical { direction="Vertical" }
        strings.write_string(builder,"{\"type\":\"Split\",\"direction\":\""); strings.write_string(builder,direction); fmt.sbprintf(builder,"\",\"ratio\":%g,\"children\":[",node.ratio)
        dock_encode_node(tree,node.children[0],codec,builder); strings.write_string(builder,","); dock_encode_node(tree,node.children[1],codec,builder); strings.write_string(builder,"]}")
    case .Leaf:
        strings.write_string(builder,"{\"type\":\"Leaf\",\"tabs\":[")
        for tab,i in node.tabs {
            if i>0 { strings.write_string(builder,",") }
            if codec.encode!=nil { bytes,error:=json.marshal(codec.encode(codec.state,tab),allocator=tree.allocator); if error==nil { strings.write_string(builder,string(bytes)) }; delete(bytes,tree.allocator) }
            else { fmt.sbprintf(builder,"%d",u64(tab)) }
        }
        fmt.sbprintf(builder,"],\"active\":%d}",node.active)
    }
}
/// Serializes the established Rust dock schema, with optional panel-name mapping for enum tabs.
dock_snapshot :: proc(tree:^Dock_Tree,codec:Dock_Tab_Codec={},allocator:mem.Allocator=context.allocator)->string {
    builder:strings.Builder; strings.builder_init(&builder,allocator)
    if len(tree.floating)==0 { dock_encode_node(tree,tree.root,codec,&builder) } else {
        strings.write_string(&builder,"{\"version\":2,\"root\":"); dock_encode_node(tree,tree.root,codec,&builder); strings.write_string(&builder,",\"floating\":[")
        for floating,i in tree.floating { if i>0 { strings.write_string(&builder,",") }; b:=floating.bounds; strings.write_string(&builder,"{\"bounds\":["); fmt.sbprintf(&builder,"%g,%g,%g,%g],\"root\":",b.x,b.y,b.width,b.height); dock_encode_node(tree,floating.root,codec,&builder); strings.write_string(&builder,"}") }; strings.write_string(&builder,"]}")
    }; return strings.to_string(builder)
}
@(private="package")
dock_decode_node :: proc(tree:^Dock_Tree,value:json.Value,codec:Dock_Tab_Codec,seen:^map[Tab_Id]bool,depth:int)->(Dock_Id,Dock_Error) {
    if depth>128 { return 0,.Invalid_Snapshot }
    object,ok:=value.(json.Object); if !ok { return 0,.Invalid_Snapshot }; kind,kind_ok:=object["type"].(string); if !kind_ok { return 0,.Invalid_Snapshot }
    if kind=="Empty" { return dock_new(tree,.Empty).id,.None }
    if kind=="Split" {
        direction,direction_ok:=object["direction"].(string); ratio,ratio_ok:=object["ratio"].(f64); children,children_ok:=object["children"].(json.Array)
        if !ratio_ok { if integer,integer_ok:=object["ratio"].(i64); integer_ok { ratio=f64(integer); ratio_ok=true } }
        if !direction_ok || (direction!="Horizontal" && direction!="Vertical") || !ratio_ok || !finite(f32(ratio)) || ratio<0 || ratio>1 || !children_ok || len(children)!=2 { return 0,.Invalid_Snapshot }
        node:=dock_new(tree,.Split); node.ratio=f32(ratio); if direction=="Vertical" { node.direction=.Vertical }
        for child,i in children { id,error:=dock_decode_node(tree,child,codec,seen,depth+1); if error!=.None { return 0,error }; node.children[i]=id }; return node.id,.None
    }
    if kind!="Leaf" { return 0,.Invalid_Snapshot }
    tabs,tabs_ok:=object["tabs"].(json.Array); active,active_ok:=object["active"].(i64)
    if !tabs_ok || !active_ok || len(tabs)==0 || active<0 || active>=i64(len(tabs)) { return 0,.Invalid_Snapshot }
    node:=dock_new(tree,.Leaf); node.active=int(active)
    for item in tabs {
        tab:Tab_Id; valid:=false
        if name,named:=item.(string); named {
            if codec.decode!=nil { tab,valid=codec.decode(codec.state,name) } else { numeric,number_ok:=strconv.parse_u64(name); tab=Tab_Id(numeric); valid=number_ok }
        } else if number,numeric:=item.(i64); numeric && number>0 { tab=Tab_Id(number); valid=true }
        if !valid || tab==0 { return 0,.Invalid_Tab }; if seen^[tab] { return 0,.Duplicate_Tab }; seen^[tab]=true; append(&node.tabs,tab)
    }
    return node.id,.None
}
/// A malformed saved layout never replaces the active dock tree.
dock_restore :: proc(tree:^Dock_Tree,snapshot:string,codec:Dock_Tab_Codec={})->Dock_Error {
    parsed,parse_error:=json.parse(snapshot,spec=.JSON,parse_integers=true,allocator=tree.allocator); if parse_error!=nil { return .Invalid_Snapshot }; defer json.destroy_value(parsed)
    candidate:=Dock_Tree{next_id=tree.next_id,allocator=tree.allocator,nodes=make(map[Dock_Id]^Dock_Node,tree.allocator),floating=make([dynamic]Dock_Floating,tree.allocator)}; seen:=make(map[Tab_Id]bool,tree.allocator); defer delete(seen)
    error:=dock_decode_layout(&candidate,parsed,codec,&seen); if error!=.None { dock_destroy(&candidate); return error }
    dock_destroy(tree); tree^=candidate; return .None
}

/// Finds the current leaf containing an exact panel identity, including after split collapse or migration.
dock_find_tab :: proc(tree:^Dock_Tree,tab:Tab_Id)->(Dock_Id,int,bool) { for id,node in tree.nodes { for value,index in node.tabs { if value==tab { return id,index,true } } }; return 0,0,false }

@(private="package")
dock_first_leaf :: proc(tree:^Dock_Tree,id:Dock_Id)->Dock_Id { node:=tree.nodes[id]; if node==nil { return 0 }; if node.kind!=.Split { return id }; return dock_first_leaf(tree,node.children[0]) }
/// Reopens or activates one unique panel, choosing the first actual leaf when no target is requested.
dock_open :: proc(tree:^Dock_Tree,tab:Tab_Id,target:Dock_Id=0,index:int=-1)->Dock_Error {
    if leaf,_,present:=dock_find_tab(tree,tab); present { return dock_apply(tree,{kind=.Activate,source=leaf,tab=tab}) }
    destination:=target; if destination==0 { destination=dock_first_leaf(tree,tree.root) }
    node,present:=tree.nodes[destination]; if !present || node.kind==.Split { return .Not_Leaf }
    at:=index; if at<0 { at=len(node.tabs) }; return dock_apply(tree,{kind=.Insert,target=destination,tab=tab,index=at})
}
