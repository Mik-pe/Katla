//! Direct C ABI to source-pinned FreeType and HarfBuzz; no engine code crosses the boundary.
package font_native

import "core:dynlib"

Glyph :: struct { glyph,cluster:u32, x,y,advance:f32, line,font:u32 }
Caret :: struct { byte:u32, x,y:f32 }
Bitmap :: struct { width,height:u32, pitch,left,top:i32, pixels:[^]u8 }
Error :: enum { None, Library, ABI, Font, Shape, Raster }
API :: struct {
    library:dynlib.Library,
    abi:proc "c" ()->u32,
    grapheme:proc "c" ([^]u8,uint,u32,i32)->u32,
    create:proc "c" (cstring,cstring)->rawptr,
    add_fallback:proc "c" (rawptr,cstring)->u32,
    destroy:proc "c" (rawptr),
    shape:proc "c" (rawptr,u32,[^]u8,uint,f32,f32)->rawptr,
    layout_destroy:proc "c" (rawptr),
    glyphs:proc "c" (rawptr,^uint)->[^]Glyph,
    carets:proc "c" (rawptr,^uint)->[^]Caret,
    dimensions:proc "c" (rawptr,^f32,^f32),
    raster:proc "c" (rawptr,u32,u32,f32,^Bitmap)->i32,
}
/// Loads the explicit dependency and verifies every mandatory function before publication.
load :: proc(path:string)->(API,Error) {
    library,ok:=dynlib.load_library(path); if !ok { return {},.Library }
    api:=API{library=library}
    api.abi=cast(type_of(api.abi))dynlib.symbol_address(library,"katla_font_abi")
    api.grapheme=cast(type_of(api.grapheme))dynlib.symbol_address(library,"katla_font_grapheme")
    api.create=cast(type_of(api.create))dynlib.symbol_address(library,"katla_font_create")
    api.add_fallback=cast(type_of(api.add_fallback))dynlib.symbol_address(library,"katla_font_add_fallback")
    api.destroy=cast(type_of(api.destroy))dynlib.symbol_address(library,"katla_font_destroy")
    api.shape=cast(type_of(api.shape))dynlib.symbol_address(library,"katla_font_shape")
    api.layout_destroy=cast(type_of(api.layout_destroy))dynlib.symbol_address(library,"katla_font_layout_destroy")
    api.glyphs=cast(type_of(api.glyphs))dynlib.symbol_address(library,"katla_font_glyphs")
    api.carets=cast(type_of(api.carets))dynlib.symbol_address(library,"katla_font_carets")
    api.dimensions=cast(type_of(api.dimensions))dynlib.symbol_address(library,"katla_font_dimensions")
    api.raster=cast(type_of(api.raster))dynlib.symbol_address(library,"katla_font_raster")
    if api.abi==nil || api.grapheme==nil || api.create==nil || api.add_fallback==nil || api.destroy==nil || api.shape==nil || api.layout_destroy==nil || api.glyphs==nil || api.carets==nil || api.dimensions==nil || api.raster==nil || api.abi()!=3 { dynlib.unload_library(library); return {},.ABI }
    return api,.None
}
/// The caller destroys all font and layout owners before unloading the dependency.
unload :: proc(api:^API) { if api.library!=nil { dynlib.unload_library(api.library) }; api^={} }
