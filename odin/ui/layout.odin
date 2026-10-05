//! Owned flex, wrapping and grid layout uses the same shaped measurements as text drawing.
package ui
import "core:math"

pixels :: proc(value:f32)->Length { return {.Pixels,value} }
percent :: proc(value:f32)->Length { return {.Percent,value} }
@(private="package")
layout_valid :: proc(s:Layout)->bool {
    for l in ([]Length{s.width,s.height,s.min_width,s.min_height,s.max_width,s.max_height}) {
        if l.kind not_in (bit_set[Length_Kind]{.Auto,.Pixels,.Percent}) || !finite(l.value) || l.value<0 { return false }
    }
    for v in ([]f32{s.padding.top,s.padding.right,s.padding.bottom,s.padding.left,s.margin.top,s.margin.right,s.margin.bottom,s.margin.left,s.gap.x,s.gap.y,s.grow,s.shrink,s.aspect_ratio,s.cell_size.x,s.cell_size.y}) { if !finite(v) || v<0 { return false } }
    return finite(s.position.x) && finite(s.position.y) && finite(s.anchor.x) && finite(s.anchor.y) && s.anchor.x>=0 && s.anchor.x<=1 && s.anchor.y>=0 && s.anchor.y<=1
}
@(private="package")
length_value :: proc(value:Length,available:f32,fallback:f32)->f32 {
    switch value.kind {
    case .Auto: return fallback
    case .Pixels: return value.value
    case .Percent: return max(0,available)*value.value
    }
    return fallback
}
@(private="package")
constrain_size :: proc(value,available:f32,minimum,maximum:Length)->f32 {
    low:=length_value(minimum,available,0); high:=length_value(maximum,available,max(f32))
    return clamp(value,low,max(low,high))
}
@(private="package")
node_font :: proc(ctx:^Context,node:^Node)->(Font_Id,f32) {
    font:=node.descriptor.font; if font==0 { font=ctx.theme.font }
    size:=node.descriptor.font_size; if size==0 { size=ctx.theme.font_size }; return font,size
}
@(private="package")
node_text :: proc(ctx:^Context,node:^Node)->string {
    if node.descriptor.kind==.Numeric_Input { return node.field_text }
    if text_editable(node) { if text,ok:=state_get(ctx,node.descriptor.state,string); ok { return text } }
    return node.descriptor.text
}
@(private="package")
measure_node :: proc(ctx:^Context,node:^Node,available:Vec2,constrain_self:bool=true)->Vec2 {
    d:=node.descriptor; s:=d.layout
    if d.hidden { return {} }
    pad:=Vec2{s.padding.left+s.padding.right,s.padding.top+s.padding.bottom}
    inside:=Vec2{max(0,available.x-pad.x),max(0,available.y-pad.y)}
    if constrain_self && s.width.kind!=.Auto { inside.x=max(0,length_value(s.width,available.x,0)-pad.x) }
    font,size:=node_font(ctx,node); intrinsic:=Vec2{}
    if len(node.children)==0 {
        wrap:f32=0; if s.width.kind!=.Auto || d.multiline || d.kind==.Text { wrap=inside.x }
        if d.text_max_width>0 { wrap=0 }
        intrinsic=ctx.fonts.measure(ctx.fonts.state,font,node_text(ctx,node),size,wrap)
        if d.kind==.Icon_Button { intrinsic={size,size} }
        else if d.icon!=0 { intrinsic.x+=size+6 }
        if d.text_max_width>0 { intrinsic.x=min(intrinsic.x,d.text_max_width); intrinsic.y=size }
        #partial switch d.kind {
        case .Button,.Icon_Button,.Menu_Item,.Text_Input,.Code_Editor,.Numeric_Input,.Checkbox,.Combo,.Tree_Row,.Selectable,.Slider,.Drag_Value,.Tabs:
            intrinsic.x+=ctx.theme.padding*2; intrinsic.y=max(intrinsic.y+ctx.theme.padding*2,ctx.theme.row_height)
            if d.kind==.Slider || d.kind==.Drag_Value { intrinsic.x=max(intrinsic.x,120) }
            if (d.kind==.Text_Input || d.kind==.Code_Editor) { intrinsic.x=max(intrinsic.x,100) }
        case .Image: intrinsic={100,100}
        case .Separator: intrinsic={1,1}
        case .Splitter: intrinsic={4,4}
        case .Progress: intrinsic={100,6}
        case .Dock_Space,.Timeline: intrinsic={100,100}
        }
    } else {
        row:=d.kind==.Row || d.kind==.Menu_Bar || d.kind==.Tabs
        count:=0; line_width:f32=0; line_height:f32=0; wrapped_height:f32=0
        for id in node.children {
            child:=node_get(ctx,id); if child.descriptor.hidden || layout_out_of_flow(child) { continue }
            m:=child.descriptor.layout.margin; child_size:=measure_node(ctx,child,inside)+Vec2{m.left+m.right,m.top+m.bottom}
            if d.kind==.Stack || d.kind==.Modal { intrinsic.x=max(intrinsic.x,child_size.x); intrinsic.y=max(intrinsic.y,child_size.y) }
            else if d.kind==.Grid {
                cell:=s.cell_size; if cell.x==0 { cell.x=child_size.x }; if cell.y==0 { cell.y=child_size.y }
                intrinsic.x=max(intrinsic.x,cell.x); intrinsic.y=max(intrinsic.y,cell.y)
            } else if row {
                intrinsic.x+=child_size.x; intrinsic.y=max(intrinsic.y,child_size.y)
                if s.wrap {
                    if line_width>0 && line_width+s.gap.x+child_size.x>inside.x { wrapped_height+=line_height+s.gap.y; line_width=0; line_height=0 }
                    if line_width>0 { line_width+=s.gap.x }; line_width+=child_size.x; line_height=max(line_height,child_size.y)
                }
            } else { intrinsic.x=max(intrinsic.x,child_size.x); intrinsic.y+=child_size.y }
            count+=1
        }
        if d.kind==.Grid {
            cols:=max(1,int(s.columns)); rows:=(count+cols-1)/cols
            intrinsic={intrinsic.x*f32(cols)+s.gap.x*f32(max(0,cols-1)),intrinsic.y*f32(rows)+s.gap.y*f32(max(0,rows-1))}
        } else if count>1 && d.kind!=.Stack && d.kind!=.Modal {
            if row { intrinsic.x+=s.gap.x*f32(count-1) } else { intrinsic.y+=s.gap.y*f32(count-1) }
        }
        if row && s.wrap { intrinsic.x=min(intrinsic.x,inside.x); intrinsic.y=wrapped_height+line_height }
    }
    result:=intrinsic+pad
    if !constrain_self { return result }
    result.x=length_value(s.width,available.x,result.x); result.y=length_value(s.height,available.y,result.y)
    if s.aspect_ratio>0 {
        if s.width.kind!=.Auto && s.height.kind==.Auto { result.y=result.x/s.aspect_ratio }
        else if s.height.kind!=.Auto && s.width.kind==.Auto { result.x=result.y*s.aspect_ratio }
    }
    result.x=constrain_size(result.x,available.x,s.min_width,s.max_width)
    result.y=constrain_size(result.y,available.y,s.min_height,s.max_height)
    return result
}
@(private="package")
Flex_Child :: struct { node:^Node,size:Vec2,main,before,after,cross_before,cross_after,weight:f32,frozen:bool }
@(private="package")
layout_flex_line :: proc(ctx:^Context,children:[]Flex_Child,inside:Rect,row:bool,cross_origin,cross_extent:f32,style:Layout,inherited_clip:Rect) {
    if len(children)==0 { return }
    main_extent:=inside.height; gap:=style.gap.y; if row { main_extent=inside.width; gap=style.gap.x }
    total:=gap*f32(len(children)-1)
    for c in children { total+=c.main+c.before+c.after }
    grow:=total<main_extent
    for i in 0..<len(children) {
        c:=&children[i]; s:=c.node.descriptor.layout
        if grow { c.weight=s.grow } else { c.weight=c.main*(1 if s.shrink==0 else s.shrink) }
        if s.no_shrink && !grow { c.weight=0 }
    }
    // Freeze min/max-constrained items, then distribute the remaining space to the surviving flex items.
    for _ in 0..<len(children)+1 {
        remaining:=main_extent-gap*f32(len(children)-1); weights:f32=0
        for c in children { remaining-=c.main+c.before+c.after; if !c.frozen { weights+=c.weight } }
        if weights<=0 || math.abs(remaining)<0.001 { break }
        newly_frozen:=false
        for i in 0..<len(children) {
            c:=&children[i]; if c.frozen || c.weight==0 { continue }
            proposed:=c.main+remaining*c.weight/weights; s:=c.node.descriptor.layout
            low,high:=s.min_height,s.max_height; if row { low,high=s.min_width,s.max_width }
            actual:=constrain_size(max(0,proposed),main_extent,low,high)
            c.main=actual
            if math.abs(actual-proposed)>0.001 { c.frozen=true; newly_frozen=true }
        }
        if !newly_frozen { break }
    }
    used:=gap*f32(len(children)-1); for c in children { used+=c.main+c.before+c.after }
    free_space:=max(0,main_extent-used); offset:f32=0; extra:f32=0
    switch style.justify {
    case .Start:
    case .Center: offset=free_space/2
    case .End: offset=free_space
    case .Space_Between: if len(children)>1 { extra=free_space/f32(len(children)-1) }
    case .Space_Around: extra=free_space/f32(len(children)); offset=extra/2
    case .Space_Evenly: extra=free_space/f32(len(children)+1); offset=extra
    }
    for c in children {
        cross:=c.size.x; if row { cross=c.size.y }
        cross_available:=max(0,cross_extent-c.cross_before-c.cross_after)
        cross_offset:=c.cross_before
        switch style.align {
        case .Start:
        case .Center: cross_offset+=max(0,cross_available-cross)/2
        case .End: cross_offset+=max(0,cross_available-cross)
        case .Stretch:
            length:=c.node.descriptor.layout.width; if row { length=c.node.descriptor.layout.height }
            if length.kind==.Auto { cross=cross_available }
        }
        cs:=c.node.descriptor.layout
        if row { cross=constrain_size(cross,cross_extent,cs.min_height,cs.max_height) } else { cross=constrain_size(cross,cross_extent,cs.min_width,cs.max_width) }
        offset+=c.before
        bounds:=Rect{inside.x+cross_origin+cross_offset,inside.y+offset,cross,c.main}
        if row { bounds={inside.x+offset,inside.y+cross_origin+cross_offset,c.main,cross} }
        layout_node(ctx,c.node,bounds,inherited_clip)
        offset+=c.main+c.after+gap+extra
    }
}
@(private="package")
layout_node :: proc(ctx:^Context,node:^Node,input_bounds,inherited_clip:Rect) {
    d:=node.descriptor; s:=d.layout
    bounds:=input_bounds
    if d.has_fixed_bounds { bounds=d.fixed_bounds }
    node.bounds=bounds; node.clip=rect_intersection(bounds,inherited_clip)
    node.content={bounds.x+s.padding.left,bounds.y+s.padding.top,max(0,bounds.width-s.padding.left-s.padding.right),max(0,bounds.height-s.padding.top-s.padding.bottom)}
    if d.hidden { node.clip={}; return }
    inside:=node.content
    clip:=inherited_clip; if d.clip_children || d.kind==.Scroll_Area || d.kind==.Modal { clip=rect_intersection(clip,inside) }
    if d.kind==.Scroll_Area {
        size:=measure_node(ctx,node,{inside.width,inside.height},false)
        node.scroll.y=clamp(node.scroll.y,0,max(0,size.y-bounds.height)); node.scroll.x=clamp(node.scroll.x,0,max(0,size.x-bounds.width))
        inside.x-=node.scroll.x; inside.y-=node.scroll.y; inside.height=max(inside.height,size.y-s.padding.top-s.padding.bottom); inside.width=max(inside.width,size.x-s.padding.left-s.padding.right)
        node.content=inside
    }
    row:=d.kind==.Row || d.kind==.Menu_Bar || d.kind==.Tabs
    children:=make([dynamic]Flex_Child,ctx.allocator); defer delete(children)
    for id in node.children {
        child:=node_get(ctx,id); cd:=child.descriptor
        if cd.hidden { child.bounds={}; child.clip={}; continue }
        size:=measure_node(ctx,child,{inside.width,inside.height}); m:=cd.layout.margin
        if layout_out_of_flow(child) || d.kind==.Stack || d.kind==.Modal {
            b:=Rect{inside.x+cd.layout.position.x+m.left+max(0,inside.width-size.x-m.left-m.right)*cd.layout.anchor.x,inside.y+cd.layout.position.y+m.top+max(0,inside.height-size.y-m.top-m.bottom)*cd.layout.anchor.y,size.x,size.y}
            if d.kind==.Stack && cd.layout.width.kind==.Auto && cd.layout.grow>0 { b.width=inside.width-m.left-m.right }
            if d.kind==.Stack && cd.layout.height.kind==.Auto && cd.layout.grow>0 { b.height=inside.height-m.top-m.bottom }
            layout_node(ctx,child,b,clip); continue
        }
        c:=Flex_Child{node=child,size=size,main=size.y,before=m.top,after=m.bottom,cross_before=m.left,cross_after=m.right}
        if row { c.main=size.x; c.before=m.left; c.after=m.right; c.cross_before=m.top; c.cross_after=m.bottom }
        append(&children,c)
    }
    if d.kind==.Grid {
        cols:=max(1,int(s.columns)); cell:=s.cell_size
        if cell.x==0 { cell.x=max(0,(inside.width-s.gap.x*f32(cols-1))/f32(cols)) }
        if cell.y==0 { for c in children { cell.y=max(cell.y,c.size.y) } }
        for c,i in children {
            b:=Rect{inside.x+f32(i%cols)*(cell.x+s.gap.x)+c.cross_before,inside.y+f32(i/cols)*(cell.y+s.gap.y)+c.before,max(0,cell.x-c.cross_before-c.cross_after),max(0,cell.y-c.before-c.after)}
            layout_node(ctx,c.node,b,clip)
        }
    } else if s.wrap && row {
        first:=0; used:f32=0; cross:f32=0; line_height:f32=0
        for c,i in children {
            occupied:=c.main+c.before+c.after
            if i>first && used+s.gap.x+occupied>inside.width {
                layout_flex_line(ctx,children[first:i],inside,true,cross,line_height,s,clip)
                for j in first..<i { children[j].node.clip=rect_intersection(children[j].node.clip,clip) }
                cross+=line_height+s.gap.y; first=i; used=0; line_height=0
            }
            if i>first { used+=s.gap.x }; used+=occupied; line_height=max(line_height,c.size.y+c.cross_before+c.cross_after)
        }
        layout_flex_line(ctx,children[first:],inside,true,cross,line_height,s,clip)
    } else { layout_flex_line(ctx,children[:],inside,row,0,inside.width if !row else inside.height,s,clip) }
    for c in children { c.node.clip=rect_intersection(c.node.clip,clip) }
}
/// Returns the current logical rectangle of an exact live node.
bounds :: proc(ctx:^Context,key:u64)->(Rect,bool) { node,present:=ctx.nodes[key]; if !present { return {},false }; return node.bounds,true }

@(private="package")
layout_out_of_flow :: proc(node:^Node)->bool { d:=node.descriptor; return d.layout.absolute || d.has_fixed_bounds || d.layer>=.Popup || d.kind==.Modal || d.kind==.Context_Menu || d.kind==.Tooltip }

@(private="package")
root_bounds :: proc(ctx:^Context,size:Vec2)->Rect {
    s:=node_get(ctx,ctx.root).descriptor.layout; width:=length_value(s.width,size.x,size.x); height:=length_value(s.height,size.y,size.y)
    if s.aspect_ratio>0 { if s.width.kind!=.Auto && s.height.kind==.Auto { height=width/s.aspect_ratio } else if s.height.kind!=.Auto && s.width.kind==.Auto { width=height*s.aspect_ratio } }
    return {0,0,constrain_size(width,size.x,s.min_width,s.max_width),constrain_size(height,size.y,s.min_height,s.max_height)}
}
