//! Font acceptance uses source-pinned shaping and actual grayscale raster output.
package font_native

import "core:testing"
import "core:strings"
import "core:path/filepath"

FONT_LIBRARY :: #config(FONT_LIBRARY,"")
FONT_RESOURCES :: #config(FONT_RESOURCES,"resources")
@(private="file")
font_test_open :: proc(t:^testing.T)->(API,rawptr) {
    api,error:=load(FONT_LIBRARY); testing.expect_value(t,error,Error.None); assert(error==.None,"Pass explicit -define:FONT_LIBRARY for native font acceptance")
    regular:=strings.clone_to_cstring(FONT_RESOURCES+"/fonts/roboto-regular.ttf"); defer delete(regular)
    icons:=strings.clone_to_cstring(FONT_RESOURCES+"/fonts/forkawesome-webfont.ttf"); defer delete(icons)
    engine:=api.create(regular,icons); testing.expect(t,engine!=nil); assert(engine!=nil)
    names:=[]string{"NotoSansArabic.ttf","NotoSansHebrew.ttf","NotoSansSC.ttf","NotoSansDevanagari.ttf","NotoSansThai.ttf","NotoSansSymbols2.ttf","NotoEmoji.ttf"}
    for name in names {
        path,path_error:=filepath.join({filepath.dir(FONT_LIBRARY),"fonts",name}); assert(path_error==nil)
        cpath:=strings.clone_to_cstring(path); testing.expect(t,api.add_fallback(engine,cpath)>2); delete(cpath); delete(path)
    }
    return api,engine
}
@(private="file")
font_test_shape :: proc(api:API,engine:rawptr,text:string,wrap:f32=0)->rawptr { layout:=api.shape(engine,1,raw_data(text),uint(len(text)),20,wrap); assert(layout!=nil); return layout }
@(test)
test_utf8_ligatures_grapheme_carets :: proc(t:^testing.T) {
    api,engine:=font_test_open(t); defer { api.destroy(engine); unload(&api) }
    layout:=font_test_shape(api,engine,"office"); defer api.layout_destroy(layout)
    count:uint; glyphs:=api.glyphs(layout,&count); testing.expect(t,count<6 && count>0)
    for glyph in glyphs[:int(count)] { testing.expect(t,glyph.glyph!=0 && glyph.font==1) }
    carets:=api.carets(layout,&count); positions:[7]f32
    for caret in carets[:int(count)] { positions[caret.byte]=caret.x }
    testing.expect(t,positions[2]<positions[3] && positions[3]<positions[4],"caret inside shaped fi ligature must move")
    accent:=font_test_shape(api,engine,"é"); defer api.layout_destroy(accent)
    carets=api.carets(accent,&count); for caret in carets[:int(count)] { testing.expect(t,caret.byte==0 || caret.byte==3,"combining grapheme split") }
    emoji:=font_test_shape(api,engine,"👩‍💻"); defer api.layout_destroy(emoji)
    glyphs=api.glyphs(emoji,&count); testing.expect_value(t,count,uint(1)); testing.expect(t,glyphs[0].glyph!=0 && glyphs[0].font==9)
    carets=api.carets(emoji,&count); for caret in carets[:int(count)] { testing.expect(t,caret.byte==0 || caret.byte==11,"emoji ZWJ grapheme split") }
    combining:="é"
    testing.expect_value(t,api.grapheme(raw_data(combining),uint(len(combining)),3,-1),u32(0))
    emoji_text:="👩‍💻"
    testing.expect_value(t,api.grapheme(raw_data(emoji_text),uint(len(emoji_text)),0,1),u32(11))
    crlf:="a\r\nb"
    testing.expect_value(t,api.grapheme(raw_data(crlf),uint(len(crlf)),3,-1),u32(1))
    invalid:=[2]u8{0xc0,0x80}; testing.expect(t,api.shape(engine,1,raw_data(invalid[:]),2,20,0)==nil,"invalid UTF8 accepted")
}
@(test)
test_script_fallback_bidi_and_actual_raster :: proc(t:^testing.T) {
    api,engine:=font_test_open(t); defer { api.destroy(engine); unload(&api) }
    samples:=[]string{"العربية","שלום","你好世界","नमस्ते","ไทย"}
    for text in samples {
        layout:=font_test_shape(api,engine,text); defer api.layout_destroy(layout)
        count:uint; glyphs:=api.glyphs(layout,&count); testing.expect(t,count>0)
        coverage:u64
        for glyph in glyphs[:int(count)] {
            testing.expect(t,glyph.glyph!=0 && glyph.font>=3)
            bitmap:Bitmap; testing.expect_value(t,api.raster(engine,glyph.font,glyph.glyph,20,&bitmap),i32(1))
            for y in 0..<bitmap.height { for x in 0..<bitmap.width { coverage+=u64(bitmap.pixels[int(y)*int(bitmap.pitch)+int(x)]) } }
        }
        testing.expect(t,coverage>10000,"fallback glyph pixels missing")
    }
    mixed:=font_test_shape(api,engine,"ABC שלום DEF"); defer api.layout_destroy(mixed)
    count:uint; glyphs:=api.glyphs(mixed,&count)
    descending:=0; latin_after:=false
    for glyph,i in glyphs[:int(count)] { if i>0 && glyph.cluster<glyphs[i-1].cluster { descending+=1 }; if descending>0 && glyph.cluster>=13 { latin_after=true } }
    testing.expect(t,descending>=3 && latin_after,"mixed bidi visual ordering lost")
}
@(test)
test_unicode_wrapping_and_newline_layout :: proc(t:^testing.T) {
    api,engine:=font_test_open(t); defer { api.destroy(engine); unload(&api) }
    samples:=[]string{"Ångström, välj dörren","你好世界你好世界","ABC שלום DEF","éééééé","office office office"}
    for text in samples {
        layout:=font_test_shape(api,engine,text,40); defer api.layout_destroy(layout)
        width,height:f32; api.dimensions(layout,&width,&height); testing.expect(t,width<=40.01 && height>24)
        count:uint; carets:=api.carets(layout,&count)
        for caret in carets[:int(count)] { testing.expect(t,caret.byte<=u32(len(text))); if int(caret.byte)<len(text) { testing.expect(t,text[caret.byte]&0xc0!=0x80) } }
    }
    layout:=font_test_shape(api,engine,"a\r\nb\u2028c\u2029"); defer api.layout_destroy(layout)
    width,height:f32; api.dimensions(layout,&width,&height); testing.expect_value(t,height,f32(96))
    empty:=font_test_shape(api,engine,""); defer api.layout_destroy(empty); api.dimensions(empty,&width,&height); testing.expect_value(t,width,f32(0)); testing.expect_value(t,height,f32(24))
}
