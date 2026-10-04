//! Floating chrome shares retained pointer capture with dock tabs and splitters.
package ui

@(private="package")
dock_float_index :: proc(tree:^Dock_Tree,id:Dock_Id)->int { index,_:=dock_floating_index(tree,id); return index }
@(private="package")
dock_host_bounds :: proc(ctx:^Context,tree:^Dock_Tree)->Rect {
    for id in ctx.order { node:=node_get(ctx,id); if node.descriptor.kind==.Dock_Space && node.descriptor.dock==tree && node.descriptor.dock_root==0 { return node.bounds } }; return {0,0,ctx.logical_size.x,ctx.logical_size.y}
}
@(private="package")
dock_raise_node :: proc(ctx:^Context,node:^Node) {
    current:=node
    for current!=nil {
        d:=current.descriptor
        if d.dock!=nil && d.dock_root!=0 { if _,present:=dock_floating_index(d.dock,d.dock_root); present { append(&ctx.actions,Dock_Action{kind=.Raise,target=d.dock_root}); return } }
        current=node_get(ctx,current.parent)
    }
}
@(private="package")
dock_float_pointer_down :: proc(ctx:^Context,node:^Node,position:Vec2,button:Pointer_Button)->bool {
    d:=node.descriptor; if button!=.Left || d.dock_root==0 { return false }; index,present:=dock_floating_index(d.dock,d.dock_root); if !present { return false }
    bounds:=d.dock.floating[index].bounds; if !rect_contains(bounds,position) { return false }; edges:u8
    if position.x<bounds.x+6 { edges|=1 }; if position.x>bounds.x+bounds.width-6 { edges|=2 }
    if position.y<bounds.y+6 { edges|=4 }; if position.y>bounds.y+bounds.height-6 { edges|=8 }
    if edges==0 { return false }; ctx.dock_float=d.dock_root; ctx.dock_resize_edges=edges; ctx.dock_bounds=bounds; return true
}
@(private="package")
dock_float_pointer_move :: proc(ctx:^Context,position:Vec2) {
    delta:=position-ctx.capture_start; bounds:=ctx.dock_bounds; edges:=ctx.dock_resize_edges
    if edges==0 { bounds.x+=delta.x; bounds.y+=delta.y } else {
        if edges&1!=0 { width:=max(120,bounds.width-delta.x); bounds.x+=bounds.width-width; bounds.width=width }
        if edges&2!=0 { bounds.width=max(120,bounds.width+delta.x) }
        if edges&4!=0 { height:=max(60,bounds.height-delta.y); bounds.y+=bounds.height-height; bounds.height=height }
        if edges&8!=0 { bounds.height=max(60,bounds.height+delta.y) }
    }
    append(&ctx.actions,Dock_Action{kind=.Float_Bounds,target=ctx.dock_float,bounds=bounds})
}
