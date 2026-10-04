//! UI commands become an immutable ordered triangle stream with independent command clips.
package render

import ui "../../ui"
import "core:mem"
import m "core:math"

UI_Batch :: struct { texture:ui.Texture_Id, clip:ui.Rect, first,count:u32 }
/// Geometry is independent of native atlas replacement and remains owned through graph publication.
UI_Mesh :: struct { vertices:[dynamic]ui.Vertex, batches:[dynamic]UI_Batch, logical_size:ui.Vec2, pixel_scale:f32, atlas_revision:u64, allocator:mem.Allocator }
@(private="package")
ui_rect_valid :: proc(rect:ui.Rect)->bool {
    for value in ([4]f32{rect.x,rect.y,rect.width,rect.height}) { if m.is_nan(value) || m.is_inf(value) { return false } }
    return rect.width>=0 && rect.height>=0
}
@(private="package")
ui_clip :: proc(a,b:ui.Rect)->ui.Rect {
    x,y:=max(a.x,b.x),max(a.y,b.y)
    return {x,y,max(f32(0),min(a.x+a.width,b.x+b.width)-x),max(f32(0),min(a.y+a.height,b.y+b.height)-y)}
}
@(private="package")
ui_batch :: proc(mesh:^UI_Mesh,texture:ui.Texture_Id,clip:ui.Rect,start:int) {
    count:=len(mesh.vertices)-start; if count==0 { return }
    if len(mesh.batches)>0 {
        last:=&mesh.batches[len(mesh.batches)-1]
        if last.texture==texture && last.clip==clip && u64(last.first)+u64(last.count)==u64(start) { last.count+=u32(count); return }
    }
    append(&mesh.batches,UI_Batch{texture,clip,u32(start),u32(count)})
}
@(private="package")
ui_quad :: proc(mesh:^UI_Mesh,bounds,uv:ui.Rect,color:ui.Color) {
    p:=[4]ui.Vec2{{bounds.x,bounds.y},{bounds.x+bounds.width,bounds.y},{bounds.x+bounds.width,bounds.y+bounds.height},{bounds.x,bounds.y+bounds.height}}
    t:=[4]ui.Vec2{{uv.x,uv.y},{uv.x+uv.width,uv.y},{uv.x+uv.width,uv.y+uv.height},{uv.x,uv.y+uv.height}}
    for index in ([6]int{0,1,2,0,2,3}) { append(&mesh.vertices,ui.Vertex{p[index],t[index],color}) }
}
@(private="package")
ui_round_rect :: proc(mesh:^UI_Mesh,bounds:ui.Rect,radius:f32,color:ui.Color,uv:ui.Vec2) {
    r:=min(max(f32(0),radius),min(bounds.width,bounds.height)*0.5)
    if r==0 { ui_quad(mesh,bounds,{uv[0],uv[1],0,0},color); return }
    center:=ui.Vertex{{bounds.x+bounds.width*0.5,bounds.y+bounds.height*0.5},uv,color}
    corners:=[4]ui.Vec2{{bounds.x+bounds.width-r,bounds.y+r},{bounds.x+bounds.width-r,bounds.y+bounds.height-r},{bounds.x+r,bounds.y+bounds.height-r},{bounds.x+r,bounds.y+r}}
    ring:[36]ui.Vertex
    for corner,i in corners { for j in 0..<9 {
        angle:=(f32(i)*0.5-0.5+f32(j)/16)*m.PI
        ring[i*9+j]={{corner[0]+f32(m.cos(angle))*r,corner[1]+f32(m.sin(angle))*r},uv,color}
    } }
    for point,i in ring { append(&mesh.vertices,center,point,ring[(i+1)%len(ring)]) }
}
/// Rebuilds an evicted or enlarged atlas before publication; older submitted atlas versions stay native-owned.
ui_prepare :: proc(fonts:^UI_Font_System,list:ui.Draw_List,allocator:=context.allocator)->(UI_Mesh,UI_Error) {
    result,error:=ui_prepare_once(fonts,list,allocator)
    if error!=.Atlas_Full { return result,error }
    ui_atlas_reset(fonts,fonts.atlas.width)
    result,error=ui_prepare_once(fonts,list,allocator)
    for error==.Atlas_Full && fonts.atlas.width<8192 {
        ui_atlas_reset(fonts,fonts.atlas.width*2)
        result,error=ui_prepare_once(fonts,list,allocator)
    }
    return result,error
}
/// Shapes and rasterizes all text before native publication; failure returns no partial mesh.
ui_prepare_once :: proc(fonts:^UI_Font_System,list:ui.Draw_List,allocator:=context.allocator)->(UI_Mesh,UI_Error) {
    if fonts==nil || fonts.engine==nil || fonts.error!=.None { return {},.Invalid_Font }
    for value in ([3]f32{list.logical_size[0],list.logical_size[1],list.pixel_scale}) { if m.is_nan(value) || m.is_inf(value) || value<=0 { return {},.Invalid_Command } }
    result:=UI_Mesh{vertices=make([dynamic]ui.Vertex,allocator),batches=make([dynamic]UI_Batch,allocator),logical_size=list.logical_size,pixel_scale=list.pixel_scale,allocator=allocator}
    success:=false; defer { if !success { ui_mesh_destroy(&result) } }
    frame:=ui.Rect{0,0,list.logical_size[0],list.logical_size[1]}
    white:=ui.Vec2{0.5/f32(fonts.atlas.width),0.5/f32(fonts.atlas.height)}
    for command in list.commands {
        start:=len(result.vertices)
        switch draw in command {
        case ui.Rect_Draw:
            if !ui_rect_valid(draw.bounds) || !ui_rect_valid(draw.clip) || m.is_nan(draw.radius) || m.is_inf(draw.radius) { return {},.Invalid_Command }
            clip:=ui_clip(draw.clip,frame)
            if clip.width==0 || clip.height==0 { continue }
            ui_round_rect(&result,draw.bounds,draw.radius,draw.color,white)
            ui_batch(&result,UI_TEXTURE_ATLAS,clip,start)
        case ui.Image_Draw:
            if !ui_rect_valid(draw.bounds) || !ui_rect_valid(draw.clip) || !ui_rect_valid(draw.uv) || draw.texture==0 { return {},.Invalid_Command }
            clip:=ui_clip(draw.clip,frame)
            if clip.width==0 || clip.height==0 { continue }
            ui_quad(&result,draw.bounds,draw.uv,draw.tint); ui_batch(&result,draw.texture,clip,start)
        case ui.Text_Draw:
            if !ui_rect_valid(draw.clip) { return {},.Invalid_Command }
            clip:=ui_clip(draw.clip,frame)
            if clip.width==0 || clip.height==0 { continue }
            for run in draw.runs {
                if run.start<0 || run.end<run.start || run.end>len(draw.text) || (run.start<len(draw.text) && draw.text[run.start]&0xc0==0x80) || (run.end<len(draw.text) && draw.text[run.end]&0xc0==0x80) { return {},.Invalid_Command }
                for value in run.color { if m.is_nan(value) || m.is_inf(value) { return {},.Invalid_Command } }
            }
            layout:=ui_shape(fonts,draw.font,draw.text,draw.size,draw.wrap_width)
            if layout==nil { return {},.Invalid_Text }
            count:uint; glyphs:=fonts.api.glyphs(layout,&count)
            error:=UI_Error.None
            for glyph in glyphs[:int(count)] {
                baseline:=ui.Vec2{draw.position[0]+glyph.x,draw.position[1]+glyph.y}
                if baseline[0]+draw.size*2<clip.x || baseline[0]-draw.size*2>clip.x+clip.width || baseline[1]+draw.size*2<clip.y || baseline[1]-draw.size*2>clip.y+clip.height { continue }
                entry:UI_Glyph_Atlas
                entry,error=ui_atlas_glyph(fonts,ui.Font_Id(glyph.font),glyph.glyph,draw.size*list.pixel_scale)
                if error!=.None { break }
                if entry.width==0 || entry.height==0 { continue }
                bounds:=ui.Rect{draw.position[0]+glyph.x+f32(entry.left)/list.pixel_scale,draw.position[1]+glyph.y-f32(entry.top)/list.pixel_scale,f32(entry.width)/list.pixel_scale,f32(entry.height)/list.pixel_scale}
                if ui_clip(bounds,clip).width==0 || ui_clip(bounds,clip).height==0 { continue }
                uv:=ui.Rect{f32(entry.x)/f32(fonts.atlas.width),f32(entry.y)/f32(fonts.atlas.height),f32(entry.width)/f32(fonts.atlas.width),f32(entry.height)/f32(fonts.atlas.height)}
                color:=draw.color
                for run in draw.runs { if int(glyph.cluster)>=run.start && int(glyph.cluster)<run.end { color=run.color; break } }
                ui_quad(&result,bounds,uv,color)
            }
            fonts.api.layout_destroy(layout)
            if error!=.None { return {},error }
            ui_batch(&result,UI_TEXTURE_ATLAS,clip,start)
        case ui.Mesh_Draw:
            if !ui_rect_valid(draw.clip) || draw.texture==0 || len(draw.indices)%3!=0 { return {},.Invalid_Command }
            clip:=ui_clip(draw.clip,frame)
            if clip.width==0 || clip.height==0 { continue }
            for index in draw.indices {
                if u64(index)>=u64(len(draw.vertices)) { return {},.Invalid_Command }
                append(&result.vertices,draw.vertices[index])
            }
            ui_batch(&result,draw.texture,clip,start)
        }
    }
    for vertex in result.vertices { for value in ([8]f32{vertex.position[0],vertex.position[1],vertex.uv[0],vertex.uv[1],vertex.color[0],vertex.color[1],vertex.color[2],vertex.color[3]}) { if m.is_nan(value) || m.is_inf(value) { return {},.Invalid_Command } } }
    result.atlas_revision=fonts.atlas.revision; success=true; return result,.None
}
/// Releases the owned triangle stream after the host has cloned its graph packets and uploads.
ui_mesh_destroy :: proc(mesh:^UI_Mesh) { delete(mesh.vertices); delete(mesh.batches); mesh^={} }
