//! Scrollbars use the same content extent and thumb geometry for rendering and capture.
package ui

@(private="package")
scrollbar :: proc(ctx:^Context,node:^Node,horizontal:bool)->(Rect,Rect,bool) {
    viewport:=rect_inset(node.bounds,1); track:=Rect{viewport.x+viewport.width-8,viewport.y,8,viewport.height}
    extent:=node.content.height; visible:=viewport.height; offset:=node.scroll.y
    if horizontal { track={viewport.x,viewport.y+viewport.height-8,viewport.width,8}; extent=node.content.width; visible=viewport.width; offset=node.scroll.x }
    if extent<=visible || visible<=0 { return {},{},false }
    thumb_size:=max(16,visible*visible/extent); fraction:=clamp(offset/max(1,extent-visible),0,1); thumb:=track
    if horizontal { thumb.x+=fraction*max(0,visible-thumb_size); thumb.width=min(visible,thumb_size) } else { thumb.y+=fraction*max(0,visible-thumb_size); thumb.height=min(visible,thumb_size) }
    _=ctx; return track,thumb,true
}
@(private="package")
paint_scrollbars :: proc(ctx:^Context,node:^Node) {
    for horizontal in ([]bool{false,true}) { track,thumb,present:=scrollbar(ctx,node,horizontal); if !present { continue }; paint_rect(ctx,track,node.clip,ctx.theme.panel,4); paint_rect(ctx,thumb,node.clip,ctx.theme.active if ctx.captured==node.id else ctx.theme.hover,4) }
}
@(private="package")
scroll_pointer_down :: proc(ctx:^Context,node:^Node,position:Vec2)->bool {
    for horizontal in ([]bool{false,true}) {
        track,thumb,present:=scrollbar(ctx,node,horizontal); if !present || !rect_contains(track,position) { continue }
        ctx.scroll_drag=true; ctx.scroll_drag_horizontal=horizontal; ctx.capture_value=node.scroll.y; if horizontal { ctx.capture_value=node.scroll.x }
        if !rect_contains(thumb,position) {
            direction:f32=-1; if (position.x>thumb.x if horizontal else position.y>thumb.y) { direction=1 }
            if horizontal { node.scroll.x=clamp(node.scroll.x+direction*node.bounds.width,0,max(0,node.content.width-node.bounds.width)); ctx.capture_value=node.scroll.x }
            else { node.scroll.y=clamp(node.scroll.y+direction*node.bounds.height,0,max(0,node.content.height-node.bounds.height)); ctx.capture_value=node.scroll.y }
            append(&ctx.actions,Scroll_Action{node.id,node.descriptor.action,node.descriptor.payload,node.scroll})
        }; return true
    }; return false
}
@(private="package")
scroll_pointer_move :: proc(ctx:^Context,node:^Node,position:Vec2) {
    if !ctx.scroll_drag { return }; horizontal:=ctx.scroll_drag_horizontal; track,thumb,present:=scrollbar(ctx,node,horizontal); if !present { return }
    extent:=node.content.height-node.bounds.height; span:=track.height-thumb.height; delta:=position.y-ctx.capture_start.y
    if horizontal { extent=node.content.width-node.bounds.width; span=track.width-thumb.width; delta=position.x-ctx.capture_start.x }
    value:=clamp(ctx.capture_value+delta*max(0,extent)/max(1,span),0,max(0,extent))
    if horizontal { node.scroll.x=value } else { node.scroll.y=value }; append(&ctx.actions,Scroll_Action{node.id,node.descriptor.action,node.descriptor.payload,node.scroll})
}
