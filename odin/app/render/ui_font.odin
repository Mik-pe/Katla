//! Application font ownership shares shaped text between layout, caret and glyph rendering.
package render

import ui "../../ui"
import font "../../deps/font_native"
import "core:mem"
import "core:strings"
import m "core:math"
import "core:path/filepath"

/// Built-in font identities are stable across atlas and native resource replacement.
UI_FONT_REGULAR :: ui.Font_Id(1)
UI_FONT_ICONS :: ui.Font_Id(2)
UI_TEXTURE_ATLAS :: ui.Texture_Id(1)
UI_Error :: enum { None, Font_Library, Invalid_Font, Invalid_Text, Atlas_Full, Invalid_Command, Missing_Texture }
/// Stationary font owner; callbacks and render preparation execute on the application thread.
UI_Font_System :: struct { api:font.API, engine:rawptr, error:UI_Error, atlas:UI_Atlas, allocator:mem.Allocator }
UI_Glyph_Key :: struct { font:ui.Font_Id, glyph,physical_size:u32 }
UI_Glyph_Atlas :: struct { x,y,width,height:u32, left,top:i32 }
/// CPU atlas updates are revisioned; native submitted textures remain immutable.
UI_Atlas :: struct { pixels:[]byte, width,height,x,y,row_height:u32, revision:u64, glyphs:map[UI_Glyph_Key]UI_Glyph_Atlas }
/// Loads repository fonts through the explicit pinned native dependency.
ui_font_init :: proc(system:^UI_Font_System,library,resources:string,allocator:=context.allocator)->UI_Error {
    api,error:=font.load(library); if error!=.None { return .Font_Library }
    regular:=strings.concatenate({resources,"/fonts/roboto-regular.ttf"},allocator); defer delete(regular,allocator)
    icons:=strings.concatenate({resources,"/fonts/forkawesome-webfont.ttf"},allocator); defer delete(icons,allocator)
    a:=strings.clone_to_cstring(regular,allocator); defer delete(a,allocator)
    b:=strings.clone_to_cstring(icons,allocator); defer delete(b,allocator)
    engine:=api.create(a,b)
    if engine==nil { font.unload(&api); return .Invalid_Font }
    fallbacks:=[]string{"NotoSansArabic.ttf","NotoSansHebrew.ttf","NotoSansSC.ttf","NotoSansDevanagari.ttf","NotoSansThai.ttf","NotoSansSymbols2.ttf","NotoEmoji.ttf"}
    for name in fallbacks {
        filename,path_error:=filepath.join({filepath.dir(library),"fonts",name},allocator)
        if path_error!=nil { api.destroy(engine); font.unload(&api); return .Invalid_Font }
        path:=strings.clone_to_cstring(filename,allocator)
        loaded:=api.add_fallback(engine,path); delete(path,allocator); delete(filename,allocator)
        if loaded==0 { api.destroy(engine); font.unload(&api); return .Invalid_Font }
    }
    system^={api=api,engine=engine,allocator=allocator}
    system.atlas={width=2048,height=2048,x=2,y=2,revision=1,pixels=make([]byte,2048*2048*4,allocator),glyphs=make(map[UI_Glyph_Key]UI_Glyph_Atlas,allocator)}
    for channel in 0..<4 { system.atlas.pixels[channel]=255 }
    return .None
}
/// Releases layouts before unloading the font dependency; GPU atlas owners remain host-owned.
ui_font_destroy :: proc(system:^UI_Font_System) {
    if system.engine!=nil { system.api.destroy(system.engine) }
    delete(system.atlas.pixels,system.allocator); delete(system.atlas.glyphs)
    font.unload(&system.api); system^={}
}
@(private="package")
ui_shape :: proc(system:^UI_Font_System,id:ui.Font_Id,text:string,size,wrap:f32)->rawptr {
    if system==nil || system.engine==nil || (id!=UI_FONT_REGULAR && id!=UI_FONT_ICONS) { if system!=nil { system.error=.Invalid_Font }; return nil }
    layout:=system.api.shape(system.engine,u32(id),raw_data(text),uint(len(text)),size,wrap)
    if layout==nil { system.error=.Invalid_Text }
    return layout
}
@(private="package")
ui_measure :: proc(state:rawptr,id:ui.Font_Id,text:string,size,wrap:f32)->ui.Vec2 {
    system:=cast(^UI_Font_System)state
    layout:=ui_shape(system,id,text,size,wrap); if layout==nil { return {} }; defer system.api.layout_destroy(layout)
    dimensions:ui.Vec2; system.api.dimensions(layout,&dimensions[0],&dimensions[1]); return dimensions
}
@(private="package")
ui_caret :: proc(state:rawptr,id:ui.Font_Id,text:string,size,wrap:f32,byte_offset:int)->ui.Vec2 {
    system:=cast(^UI_Font_System)state
    layout:=ui_shape(system,id,text,size,wrap); if layout==nil { return {} }; defer system.api.layout_destroy(layout)
    count:uint; data:=system.api.carets(layout,&count)
    best:ui.Vec2; distance:=max(int)
    for caret in data[:int(count)] { delta:=abs(int(caret.byte)-byte_offset); if delta<distance { distance=delta; best={caret.x,caret.y} } }
    return best
}
@(private="package")
ui_hit_test :: proc(state:rawptr,id:ui.Font_Id,text:string,size,wrap:f32,position:ui.Vec2)->int {
    system:=cast(^UI_Font_System)state
    layout:=ui_shape(system,id,text,size,wrap); if layout==nil { return 0 }; defer system.api.layout_destroy(layout)
    count:uint; data:=system.api.carets(layout,&count)
    index:int; distance:f32=max(f32)
    line_y:=f32(m.floor(position[1]/(size*1.2)))*size*1.2
    for caret in data[:int(count)] {
        delta:=abs(caret.x-position[0])+abs(caret.y-line_y)*10000
        if delta<distance { distance=delta; index=int(caret.byte) }
    }
    return index
}
@(private="package")
ui_grapheme :: proc(state:rawptr,text:string,offset,direction:int)->int {
    system:=cast(^UI_Font_System)state
    boundary:=system.api.grapheme(raw_data(text),uint(len(text)),u32(clamp(offset,0,len(text))),i32(direction))
    if boundary==max(u32) { system.error=.Invalid_Text; return clamp(offset,0,len(text)) }
    return int(boundary)
}
@(private="package")
ui_navigate :: proc(state:rawptr,id:ui.Font_Id,text:string,size,wrap:f32,offset,direction:int)->int {
    system:=cast(^UI_Font_System)state
    if direction!=1 && direction!= -1 { system.error=.Invalid_Command; return offset }
    layout:=ui_shape(system,id,text,size,wrap); if layout==nil { return offset }; defer system.api.layout_destroy(layout)
    count:uint; data:=system.api.carets(layout,&count)
    current:ui.Vec2; distance:=max(int)
    for caret in data[:int(count)] { delta:=abs(int(caret.byte)-offset); if delta<distance { distance=delta; current={caret.x,caret.y} } }
    found:=false; best:ui.Vec2; result:=offset
    for caret in data[:int(count)] {
        if int(caret.byte)==offset { continue }
        forward:=caret.y>current[1] || (caret.y==current[1] && caret.x>current[0])
        backward:=caret.y<current[1] || (caret.y==current[1] && caret.x<current[0])
        if (direction>0 && !forward) || (direction<0 && !backward) { continue }
        earlier:=caret.y<best[1] || (caret.y==best[1] && caret.x<best[0])
        later:=caret.y>best[1] || (caret.y==best[1] && caret.x>best[0])
        if !found || (direction>0 && earlier) || (direction<0 && later) { found=true; best={caret.x,caret.y}; result=int(caret.byte) }
    }
    return result
}
/// Valid fonts use identical shaping for all UI layout/input callbacks and GPU glyph preparation.
ui_font_provider :: proc(system:^UI_Font_System)->ui.Font_Provider { return {system,ui_measure,ui_caret,ui_hit_test,ui_navigate,ui_grapheme} }
@(private="package")
ui_atlas_reset :: proc(system:^UI_Font_System,width:u32) {
    atlas:=&system.atlas
    delete(atlas.pixels,system.allocator); delete(atlas.glyphs)
    atlas.pixels=make([]byte,int(width)*int(width)*4,system.allocator)
    atlas.glyphs=make(map[UI_Glyph_Key]UI_Glyph_Atlas,system.allocator)
    atlas.width=width; atlas.height=width; atlas.x=2; atlas.y=2; atlas.row_height=0; atlas.revision+=1
    for channel in 0..<4 { atlas.pixels[channel]=255 }
}
@(private="package")
ui_atlas_glyph :: proc(system:^UI_Font_System,id:ui.Font_Id,glyph:u32,size:f32)->(UI_Glyph_Atlas,UI_Error) {
    if (m.is_nan(size) || m.is_inf(size)) || size<=0 || size>2048 { return {},.Invalid_Text }
    key:=UI_Glyph_Key{id,glyph,u32(m.round(size*64))}
    cached,ok:=system.atlas.glyphs[key]; if ok { return cached,.None }
    bitmap:font.Bitmap
    if system.api.raster(system.engine,u32(id),glyph,f32(key.physical_size)/64,&bitmap)==0 { return {},.Invalid_Text }
    atlas:=&system.atlas
    if bitmap.width+2>atlas.width || bitmap.height+2>atlas.height { return {},.Atlas_Full }
    if atlas.x+bitmap.width+1>atlas.width { atlas.x=1; atlas.y+=atlas.row_height+1; atlas.row_height=0 }
    if atlas.y+bitmap.height+1>atlas.height { return {},.Atlas_Full }
    entry:=UI_Glyph_Atlas{atlas.x,atlas.y,bitmap.width,bitmap.height,bitmap.left,bitmap.top}
    for y in 0..<bitmap.height { for x in 0..<bitmap.width {
        source:=int(y)*int(bitmap.pitch)+int(x)
        if bitmap.pitch<0 { source=int(bitmap.height-1-y)*(-int(bitmap.pitch))+int(x) }
        destination:=int(((atlas.y+y)*atlas.width+atlas.x+x)*4)
        atlas.pixels[destination]=255; atlas.pixels[destination+1]=255; atlas.pixels[destination+2]=255; atlas.pixels[destination+3]=bitmap.pixels[source]
    } }
    atlas.x+=bitmap.width+1; atlas.row_height=max(atlas.row_height,bitmap.height); atlas.revision+=1
    atlas.glyphs[key]=entry
    return entry,.None
}
