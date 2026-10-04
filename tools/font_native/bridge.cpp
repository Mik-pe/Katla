#include "bridge.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include <hb.h>
#include <hb-ft.h>
#include <hb-ot.h>
#include <SheenBidi/SheenBidi.h>
#include <linebreak.h>
#include <graphemebreak.h>
#include <memory>
#include <map>
#include <string>
#include <algorithm>
#include <cmath>
#include <new>
#include <vector>

struct Layout { std::vector<KatlaFontGlyph> glyphs; std::vector<KatlaFontCaret> carets; float width{},height{}; };
struct Cached { Layout layout;uint64_t tick;size_t bytes; };
struct Engine { FT_Library library{}; std::vector<FT_Face> faces; std::vector<hb_font_t*> fonts;std::map<std::string,Cached> cache;uint64_t tick{};size_t cache_bytes{}; };
template<class T> struct NativeOwner { T value;void(*release)(T);NativeOwner(T v,void(*r)(T)):value(v),release(r){}NativeOwner(const NativeOwner&)=delete;~NativeOwner(){if(value)release(value);} };
struct Run { std::vector<hb_glyph_info_t> info; std::vector<hb_glyph_position_t> positions; float width{}; };
static size_t next_byte(const uint8_t *text,size_t end,size_t byte) {
    if (byte>=end) return end;
    byte++;
    while (byte<end && (text[byte]&0xc0)==0x80) byte++;
    return byte;
}
static bool valid_utf8(const uint8_t *text,size_t n) {
    for (size_t i=0;i<n;) {
        unsigned a=text[i++],v=a; int remaining=0;
        if (a<0x80) continue;
        if (a>=0xc2 && a<=0xdf) {remaining=1;v=a&31;}
        else if (a>=0xe0 && a<=0xef) {remaining=2;v=a&15;}
        else if (a>=0xf0 && a<=0xf4) {remaining=3;v=a&7;}
        else return false;
        int bytes=remaining;
        while (remaining--) {if (i>=n || (text[i]&0xc0)!=0x80) return false;v=(v<<6)|(text[i++]&63);}
        if ((bytes==2 && v<0x800)||(bytes==3 && v<0x10000)||v>0x10ffff||(v>=0xd800&&v<=0xdfff)) return false;
    }
    return true;
}
struct Script { size_t start,end;hb_script_t script; };
struct VisualRun { Run shaped;size_t start,end;bool rtl;uint32_t font; };
static Run shape(hb_font_t *font,const uint8_t *text,size_t start,size_t end,bool rtl,hb_script_t script) {
    Run r; hb_buffer_t *buffer=hb_buffer_create();NativeOwner<hb_buffer_t*> owner(buffer,hb_buffer_destroy);
    if(!hb_buffer_allocation_successful(buffer))throw std::bad_alloc();
    hb_buffer_set_cluster_level(buffer,HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES);
    hb_buffer_add_utf8(buffer,reinterpret_cast<const char*>(text),static_cast<int>(end),static_cast<unsigned>(start),static_cast<int>(end-start));
    hb_buffer_set_direction(buffer,rtl?HB_DIRECTION_RTL:HB_DIRECTION_LTR);
    hb_buffer_set_script(buffer,script);hb_buffer_guess_segment_properties(buffer); hb_shape(font,buffer,nullptr,0);
    unsigned count=0;
    auto info=hb_buffer_get_glyph_infos(buffer,&count); auto positions=hb_buffer_get_glyph_positions(buffer,nullptr);
    r.info.assign(info,info+count);r.positions.assign(positions,positions+count);
    for (auto p:r.positions) r.width+=p.x_advance/64.f;
    if(!hb_buffer_allocation_successful(buffer))throw std::bad_alloc();
    r.width=std::abs(r.width);return r;
}
struct Paragraph {
    SBAlgorithmRef algorithm{};SBParagraphRef paragraph{};std::vector<Script> scripts;size_t offset{};
    ~Paragraph(){if(paragraph)SBParagraphRelease(paragraph);if(algorithm)SBAlgorithmRelease(algorithm);}
    Paragraph(const uint8_t *text,size_t size,size_t start,size_t end) {
      try {
        (void)size;offset=start;SBCodepointSequence sequence={SBStringEncodingUTF8,text+start,end-start};
        algorithm=SBAlgorithmCreate(&sequence);if(!algorithm)throw std::bad_alloc();
        paragraph=SBAlgorithmCreateParagraph(algorithm,0,end-start,SBLevelDefaultLTR);
        if(!paragraph)throw std::bad_alloc();
        SBScriptLocatorRef locator=SBScriptLocatorCreate();if(!locator)throw std::bad_alloc();NativeOwner<SBScriptLocatorRef> locator_owner(locator,SBScriptLocatorRelease);
        SBScriptLocatorLoadCodepoints(locator,&sequence);
        while(SBScriptLocatorMoveNext(locator)) {
            auto agent=SBScriptLocatorGetAgent(locator);
            size_t a=start+agent->offset,b=start+agent->offset+agent->length;
            if(a<b)scripts.push_back({a,b,hb_script_from_iso15924_tag(SBScriptGetUnicodeTag(agent->script))});
        }
      } catch(...) {if(paragraph)SBParagraphRelease(paragraph);if(algorithm)SBAlgorithmRelease(algorithm);paragraph=nullptr;algorithm=nullptr;throw;}
    }
};
static uint32_t select_font(Engine &engine,uint32_t primary,const SBCodepointSequence &sequence,size_t begin,size_t end) {
    auto supports=[&](size_t index){for(size_t offset=begin;offset<end;){auto codepoint=SBCodepointSequenceGetCodepointAt(&sequence,&offset);
        if(hb_unicode_general_category(hb_unicode_funcs_get_default(),codepoint)==HB_UNICODE_GENERAL_CATEGORY_FORMAT||(codepoint>=0xfe00&&codepoint<=0xfe0f)||(codepoint>=0xe0100&&codepoint<=0xe01ef))continue;
        if(!FT_Get_Char_Index(engine.faces[index],codepoint))return false;}return true;};
    if(supports(primary-1))return primary;
    for(size_t i=2;i<engine.faces.size();i++)if(supports(i))return uint32_t(i+1);
    return primary;
}
static std::vector<VisualRun> visual_runs(Paragraph &p,Engine &engine,uint32_t primary,const uint8_t *text,size_t size,const std::vector<char> &graphemes,size_t start,size_t end) {
    std::vector<VisualRun> result;
    SBLineRef line=SBParagraphCreateLine(p.paragraph,start-p.offset,end-start);if(!line)throw std::bad_alloc();NativeOwner<SBLineRef> line_owner(line,SBLineRelease);
    auto runs=SBLineGetRunsPtr(line);size_t count=SBLineGetRunCount(line);
    SBCodepointSequence sequence={SBStringEncodingUTF8,text,size};
    struct Part {size_t start,end;hb_script_t script;uint32_t font;};
    for(size_t i=0;i<count;i++) {
        auto run=runs[i];run.offset+=p.offset;bool rtl=(run.level&1)!=0;std::vector<Part> parts;
        for(auto script:p.scripts){size_t a=std::max(script.start,run.offset),b=std::min(script.end,run.offset+run.length);if(a>=b)continue;
            for(size_t begin=a;begin<b;){size_t finish=next_byte(text,b,begin);while(finish<b&&graphemes[finish-1]!=GRAPHEMEBREAK_BREAK)finish=next_byte(text,b,finish);
                auto selected=select_font(engine,primary,sequence,begin,finish);
                if(!parts.empty()&&parts.back().end==begin&&parts.back().font==selected&&parts.back().script==script.script)parts.back().end=finish;
                else parts.push_back({begin,finish,script.script,selected});begin=finish;}
        }
        if(rtl)std::reverse(parts.begin(),parts.end());
        for(auto part:parts) result.push_back({shape(engine.fonts[part.font-1],text,part.start,part.end,rtl,part.script),part.start,part.end,rtl,part.font});
    }
    return result;
}
static float line_width(const std::vector<VisualRun> &runs){float width=0;for(auto &run:runs)width+=run.shaped.width;return width;}
static void append_line(Layout &layout,const std::vector<VisualRun> &runs,Engine &engine,const std::vector<char> &graphemes,float ascent,float height,uint32_t line) {
    float x=0;
    for(auto &run:runs){
        auto &r=run.shaped;std::vector<size_t> clusters{run.end};
        for(auto info:r.info)clusters.push_back(info.cluster);
        std::sort(clusters.begin(),clusters.end());clusters.erase(std::unique(clusters.begin(),clusters.end()),clusters.end());
        for(size_t i=0;i<r.info.size();){
            size_t j=i+1;while(j<r.info.size()&&r.info[j].cluster==r.info[i].cluster)j++;
            size_t begin=r.info[i].cluster,end=*std::upper_bound(clusters.begin(),clusters.end(),begin);
            float advance=0;for(size_t k=i;k<j;k++)advance+=r.positions[k].x_advance/64.f;
            std::vector<size_t> boundaries{begin};for(size_t k=begin+1;k<end;k++)if(graphemes[k-1]==GRAPHEMEBREAK_BREAK)boundaries.push_back(k);boundaries.push_back(end);
            std::vector<hb_position_t> ligatures(boundaries.size());unsigned total=unsigned(ligatures.size());
            unsigned available=hb_ot_layout_get_ligature_carets(engine.fonts[run.font-1],run.rtl?HB_DIRECTION_RTL:HB_DIRECTION_LTR,r.info[i].codepoint,0,&total,ligatures.data());
            for(size_t k=0;k<boundaries.size();k++){
                float fraction=float(k)/float(boundaries.size()-1);float position=advance*fraction;
                if(k>0&&k+1<boundaries.size()&&available>=boundaries.size()-2&&k<=total)position=ligatures[k-1]/64.f;
                layout.carets.push_back({uint32_t(boundaries[k]),x+(run.rtl?advance-position:position),line*height});
            }
            for(size_t k=i;k<j;k++){auto p=r.positions[k];auto info=r.info[k];layout.glyphs.push_back({info.codepoint,info.cluster,x+p.x_offset/64.f,line*height+ascent-p.y_offset/64.f,p.x_advance/64.f,line,run.font});x+=p.x_advance/64.f;}
            i=j;
        }
    }
    layout.width=std::max(layout.width,x);
}
extern "C" uint32_t katla_font_grapheme(const uint8_t *text,size_t n,uint32_t byte,int32_t direction) {
    if((!text&&n)||n>8*1024*1024||!valid_utf8(text,n)||(direction!=-1&&direction!=1))return UINT32_MAX;
    if(!n)return 0;byte=std::min(byte,uint32_t(n));
    try {std::vector<char> breaks(n);set_graphemebreaks_utf8(text,n,nullptr,breaks.data());uint32_t previous=0;
        for(size_t i=0;i<n;i++)if(breaks[i]==GRAPHEMEBREAK_BREAK||i+1==n){uint32_t boundary=uint32_t(i+1);
            if(direction>0&&boundary>byte)return boundary;
            if(direction<0&&boundary>=byte)return previous;previous=boundary;}
        return direction>0?uint32_t(n):previous;
    }catch(...){return UINT32_MAX;}
}
extern "C" uint32_t katla_font_abi(void) {return 3;}
extern "C" void katla_font_destroy(void *pointer) {
    auto e=static_cast<Engine*>(pointer);if (!e)return;
    for (size_t i=0;i<e->faces.size();i++){if(e->fonts[i])hb_font_destroy(e->fonts[i]);if(e->faces[i])FT_Done_Face(e->faces[i]);}
    if(e->library)FT_Done_FreeType(e->library);delete e;
}
extern "C" uint32_t katla_font_add_fallback(void *pointer,const char *path) {
    auto e=static_cast<Engine*>(pointer);if(!e||!path||e->faces.size()>=64)return 0;
    FT_Face face{};if(FT_New_Face(e->library,path,0,&face))return 0;
    hb_font_t *font=hb_ft_font_create_referenced(face);hb_ft_font_set_load_flags(font,FT_LOAD_NO_HINTING);
    try{e->faces.push_back(face);try{e->fonts.push_back(font);}catch(...){e->faces.pop_back();throw;}}
    catch(...){hb_font_destroy(font);FT_Done_Face(face);return 0;}
    e->cache.clear();e->cache_bytes=0;return uint32_t(e->faces.size());
}
extern "C" void *katla_font_create(const char *regular,const char *icons) {
    auto e=new(std::nothrow) Engine;if(!e)return nullptr;
    if(FT_Init_FreeType(&e->library)){katla_font_destroy(e);return nullptr;}
    if(!katla_font_add_fallback(e,regular)||!katla_font_add_fallback(e,icons)){katla_font_destroy(e);return nullptr;}
    return e;
}
extern "C" void *katla_font_shape(void *pointer,uint32_t font,const uint8_t *text,size_t n,float size,float wrap) {
    auto e=static_cast<Engine*>(pointer);
    if(!e||font<1||font>2||(!text&&n)||n>8*1024*1024||!std::isfinite(size)||size<=0||size>512||!std::isfinite(wrap)||wrap<0||!valid_utf8(text,n))return nullptr;
    std::string key;
    try {
        key.assign(reinterpret_cast<const char*>(text?text:reinterpret_cast<const uint8_t*>("")),n);
        key.append(reinterpret_cast<const char*>(&font),sizeof(font));key.append(reinterpret_cast<const char*>(&size),sizeof(size));key.append(reinterpret_cast<const char*>(&wrap),sizeof(wrap));
        auto found=e->cache.find(key);
        if(found!=e->cache.end()){found->second.tick=++e->tick;return new Layout(found->second.layout);}
    } catch(...){return nullptr;}
    auto l=new(std::nothrow) Layout;if(!l)return nullptr;
    try {
        for(size_t i=0;i<e->faces.size();i++){FT_Set_Char_Size(e->faces[i],0,static_cast<FT_F26Dot6>(std::lround(size*64)),72,72);hb_ft_font_changed(e->fonts[i]);}
        float ascent=e->faces[font-1]->size->metrics.ascender/64.f;
        float line_height=size*1.2f;uint32_t line=0;
        std::vector<char> breaks(n),graphemes(n);
        if(n){set_linebreaks_utf8(text,n,nullptr,breaks.data());set_graphemebreaks_utf8(text,n,nullptr,graphemes.data());}
        size_t paragraph_start=0;
        do {
            size_t paragraph_end=paragraph_start,separator_end=paragraph_start;
            while(paragraph_end<n){
                size_t next=next_byte(text,n,paragraph_end);
                if(text[paragraph_end]=='\n'||text[paragraph_end]=='\r'||(next-paragraph_end==3&&text[paragraph_end]==0xe2&&text[paragraph_end+1]==0x80&&(text[paragraph_end+2]==0xa8||text[paragraph_end+2]==0xa9))){separator_end=next;if(text[paragraph_end]=='\r'&&next<n&&text[next]=='\n')separator_end++;break;}
                paragraph_end=next;separator_end=next;
            }
            if(paragraph_end==paragraph_start){l->carets.push_back({uint32_t(paragraph_start),0,line*line_height});line++;}
            else {
                Paragraph paragraph(text,n,paragraph_start,paragraph_end);
                auto full=visual_runs(paragraph,*e,font,text,n,graphemes,paragraph_start,paragraph_end);
                std::map<size_t,float> advances;
                for(auto &run:full)for(size_t i=0;i<run.shaped.info.size();i++)advances[run.shaped.info[i].cluster]+=run.shaped.positions[i].x_advance/64.f;
                size_t start=paragraph_start;
                while(start<paragraph_end){
                    size_t end=paragraph_end;
                    if(wrap>0){float width=0;size_t fit=start,last_break=start;
                        for(size_t cursor=start;cursor<paragraph_end;){size_t next=next_byte(text,paragraph_end,cursor);width+=advances[cursor];
                            if(graphemes[next-1]==GRAPHEMEBREAK_BREAK||next==paragraph_end){if(width>wrap&&fit>start){end=last_break>start?last_break:fit;break;}fit=next;if(breaks[next-1]==LINEBREAK_ALLOWBREAK||breaks[next-1]==LINEBREAK_MUSTBREAK)last_break=next;}cursor=next;}
                    }
                    auto runs=visual_runs(paragraph,*e,font,text,n,graphemes,start,end);
                    // A boundary removes cross-line kerning. Recheck the final shaped line only.
                    while(wrap>0&&line_width(runs)>wrap&&end>next_byte(text,paragraph_end,start)){
                        size_t previous=end-1;while(previous>start&&graphemes[previous-1]!=GRAPHEMEBREAK_BREAK)previous--;
                        if(previous<=start)break;end=previous;runs=visual_runs(paragraph,*e,font,text,n,graphemes,start,end);
                    }
                    append_line(*l,runs,*e,graphemes,ascent,line_height,line);line++;start=end;
                }
            }
            if(paragraph_end==n)break;
            paragraph_start=separator_end;
        }while(paragraph_start<=n);
        l->height=line*line_height;
    } catch (...) {delete l;return nullptr;}
    size_t bytes=key.size()+l->glyphs.size()*sizeof(KatlaFontGlyph)+l->carets.size()*sizeof(KatlaFontCaret);
    if(bytes<=16*1024*1024){
        try {
            while(!e->cache.empty()&&(e->cache.size()>=128||e->cache_bytes+bytes>64*1024*1024)){
                auto oldest=e->cache.begin();for(auto it=e->cache.begin();it!=e->cache.end();++it)if(it->second.tick<oldest->second.tick)oldest=it;
                e->cache_bytes-=oldest->second.bytes;e->cache.erase(oldest);
            }
            e->cache.emplace(std::move(key),Cached{*l,++e->tick,bytes});e->cache_bytes+=bytes;
        } catch(...) {}
    }
    return l;
}
extern "C" void katla_font_layout_destroy(void *layout){delete static_cast<Layout*>(layout);}
extern "C" const KatlaFontGlyph *katla_font_glyphs(void *layout,size_t *count){auto l=static_cast<Layout*>(layout);if(!l||!count)return nullptr;*count=l->glyphs.size();return l->glyphs.data();}
extern "C" const KatlaFontCaret *katla_font_carets(void *layout,size_t *count){auto l=static_cast<Layout*>(layout);if(!l||!count)return nullptr;*count=l->carets.size();return l->carets.data();}
extern "C" void katla_font_dimensions(void *layout,float *width,float *height){auto l=static_cast<Layout*>(layout);if(l&&width&&height){*width=l->width;*height=l->height;}}
extern "C" int katla_font_raster(void *pointer,uint32_t font,uint32_t glyph,float size,KatlaFontBitmap *out){
    auto e=static_cast<Engine*>(pointer);if(!e||font<1||font>e->faces.size()||!out||!std::isfinite(size)||size<=0||size>2048)return 0;
    FT_Face face=e->faces[font-1];
    if(FT_Set_Char_Size(face,0,static_cast<FT_F26Dot6>(std::lround(size*64)),72,72)||FT_Load_Glyph(face,glyph,FT_LOAD_DEFAULT)||FT_Render_Glyph(face->glyph,FT_RENDER_MODE_NORMAL))return 0;
    auto &b=face->glyph->bitmap;if(b.pixel_mode!=FT_PIXEL_MODE_GRAY&&b.width)return 0;
    *out={b.width,b.rows,b.pitch,face->glyph->bitmap_left,face->glyph->bitmap_top,b.buffer};return 1;
}
