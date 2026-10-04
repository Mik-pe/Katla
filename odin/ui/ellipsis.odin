//! Shaped single-line labels retain complete text for noninteractive hover previews.
package ui
import "core:strings"

@(private="package")
text_ellipsize :: proc(ctx:^Context,node:^Node,width:f32)->(string,bool) {
    text:=node.descriptor.text; font,size:=node_font(ctx,node)
    end:=len(text); for r,byte in text { if r=='\n' || r=='\r' { end=byte; break } }
    if end==len(text) && ctx.fonts.measure(ctx.fonts.state,font,text,size,0).x<=width { return text,false }
    ellipsis::"…"
    if ctx.fonts.measure(ctx.fonts.state,font,ellipsis,size,0).x>width { return strings.clone("",ctx.allocator),true }
    boundaries:=make([dynamic]int,ctx.allocator); defer delete(boundaries)
    append(&boundaries,0)
    for offset:=0;offset<end; {
        next:=ctx.fonts.grapheme(ctx.fonts.state,text,offset,1)
        if next<=offset || next>end { break }
        append(&boundaries,next); offset=next
    }
    low,high:=0,len(boundaries)
    for low+1<high {
        middle:=(low+high)/2
        candidate:=strings.concatenate({text[:boundaries[middle]],ellipsis},ctx.allocator)
        fits:=ctx.fonts.measure(ctx.fonts.state,font,candidate,size,0).x<=width
        delete(candidate,ctx.allocator)
        if fits { low=middle } else { high=middle }
    }
    return strings.concatenate({text[:boundaries[low]],ellipsis},ctx.allocator),true
}

@(private="package")
paint_text_tooltip :: proc(ctx:^Context,window:Rect) {
    if !ctx.window_focused || ctx.captured.key!=0 || ctx.popup.key!=0 { return }
    blocker:=hit_node(ctx,ctx.pointer)
    for i:=len(ctx.order)-1;i>=0;i-=1 {
        node:=node_get(ctx,ctx.order[i]); d:=node.descriptor
        if d.kind!=.Text || d.text_max_width<=0 || !node_visible(ctx,node) || !rect_contains(node.clip,ctx.pointer) { continue }
        if ctx.modal.key!=0 && !node_descends(ctx,node,ctx.modal) { continue }
        if blocker!=nil && node_layer(ctx,blocker)>node_layer(ctx,node) { continue }
        label,shortened:=text_ellipsize(ctx,node,min(d.text_max_width,node.bounds.width))
        if !shortened { continue }; delete(label,ctx.allocator)
        font,size:=node_font(ctx,node); padding:=ctx.theme.padding
        wrap:=max(0,window.width-4*padding); if wrap<=0 { return }
        measured:=ctx.fonts.measure(ctx.fonts.state,font,d.text,size,wrap)
        bounds:=Rect{ctx.pointer.x+12,ctx.pointer.y+18,min(window.width,measured.x+2*padding),min(window.height,measured.y+2*padding)}
        bounds.x=clamp(bounds.x,window.x,window.x+window.width-bounds.width)
        bounds.y=clamp(bounds.y,window.y,window.y+window.height-bounds.height)
        paint_rect(ctx,bounds,window,ctx.theme.panel,ctx.theme.radius)
        paint_text(ctx,node,d.text,{bounds.x+padding,bounds.y+padding},ctx.theme.text,rect_inset(bounds,padding),wrap)
        return
    }
}
