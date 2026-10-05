//! Verified typography sources and licensed fallback font assets.
package katla_build
import "core:fmt"
import "core:path/filepath"

Font_Asset :: struct { name,directory,filename,sha256:string }
FONT_ASSETS :: []Font_Asset{
    {"NotoSansArabic.ttf","notosansarabic","NotoSansArabic%5Bwdth%2Cwght%5D.ttf","63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e"},
    {"NotoSansHebrew.ttf","notosanshebrew","NotoSansHebrew%5Bwdth%2Cwght%5D.ttf","7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88"},
    {"NotoSansSC.ttf","notosanssc","NotoSansSC%5Bwght%5D.ttf","a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da"},
    {"NotoSansDevanagari.ttf","notosansdevanagari","NotoSansDevanagari%5Bwdth%2Cwght%5D.ttf","14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e"},
    {"NotoSansThai.ttf","notosansthai","NotoSansThai%5Bwdth%2Cwght%5D.ttf","5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8"},
    {"NotoSansSymbols2.ttf","notosanssymbols2","NotoSansSymbols2-Regular.ttf","7d5fb73b7ca67a6798101741f5d280a3d016a56a197afcd4199dbb57b4b82a21"},
    {"NotoEmoji.ttf","notoemoji","NotoEmoji%5Bwght%5D.ttf","de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551"},
}
build_fonts :: proc(folder:string)->string {
    mkdir(folder)
    font_folder:=join(folder,"fonts"); mkdir(font_folder)
    for asset in FONT_ASSETS {
        url:=cat("https://raw.githubusercontent.com/google/fonts/9710da1eacb3be272583c3224dcb70f9da6eadbb/ofl/",asset.directory,"/")
        download(cat(url,asset.filename),join(font_folder,asset.name),asset.sha256)
        download(cat(url,"OFL.txt"),join(font_folder,cat(asset.directory,"-OFL.txt")),"")
    }
    freetype:=source("freetype","https://github.com/freetype/freetype.git","526ec5c47b9ebccc4754c85ac0c0cdf7c85a5e9b")
    harfbuzz:=source("harfbuzz","https://github.com/harfbuzz/harfbuzz.git","720f1a3b6699df28c6f600f5d750100ba22d389b")
    sheenbidi:=source("sheenbidi","https://github.com/Tehreer/SheenBidi.git","cfe430e7375a7845b679adae9d51dac6deaa8858")
    unibreak:=source("unibreak","https://github.com/adah1972/libunibreak.git","304585d8e2d63187507368d612c3d5fff1486368")
    write(join(folder,"katla_ft_modules.h"),"FT_USE_MODULE(FT_Driver_ClassRec, tt_driver_class)\nFT_USE_MODULE(FT_Module_Class, sfnt_module_class)\nFT_USE_MODULE(FT_Module_Class, psnames_module_class)\nFT_USE_MODULE(FT_Renderer_Class, ft_smooth_renderer_class)\nFT_USE_MODULE(FT_Module_Class, autofit_module_class)\n")
    objects:=make([dynamic]string)
    flags:=native_flags(); append(&flags,"-DFT2_BUILD_LIBRARY",`-DFT_CONFIG_MODULES_H="katla_ft_modules.h"`,"-I",folder,"-I",join(freetype,"include"))
    for file in ([]string{"base/ftsystem.c","base/ftinit.c","base/ftbase.c","base/ftdebug.c","base/ftglyph.c","base/ftbitmap.c","base/ftmm.c","base/ftbbox.c","base/ftfstype.c","base/ftgasp.c","base/ftstroke.c","base/ftsynth.c","truetype/truetype.c","sfnt/sfnt.c","smooth/smooth.c","psnames/psnames.c","autofit/autofit.c","gzip/ftgzip.c"}) {
        object:=join(folder,fmt.aprintf("ft-%d.o",len(objects))); compile(cc,join(freetype,"src",file),object,flags[:]); append(&objects,object)
    }
    flags=native_flags(); append(&flags,"-std=c++17","-DHAVE_FREETYPE","-DHAVE_FT_GET_TRANSFORM","-DHAVE_FT_GET_VAR_BLEND_COORDINATES","-DHAVE_FT_DONE_MM_VAR","-I",join(freetype,"include"),"-I",join(harfbuzz,"src"))
    object:=join(folder,"harfbuzz.o"); compile(cxx,join(harfbuzz,"src/harfbuzz.cc"),object,flags[:]); append(&objects,object)
    flags=native_flags(); append(&flags,"-DSB_CONFIG_UNITY","-I",join(sheenbidi,"Headers"),"-I",join(sheenbidi,"Source"))
    object=join(folder,"sheenbidi.o"); compile(cc,join(sheenbidi,"Source/SheenBidi.c"),object,flags[:]); append(&objects,object)
    flags=native_flags(); append(&flags,"-I",join(unibreak,"src"))
    for name in ([]string{"unibreakbase","unibreakdef","linebreak","linebreakdata","linebreakdef","eastasianwidthdef","emojidef","graphemebreak","wordbreak"}) {
        object=join(folder,cat(name,".o")); compile(cc,join(unibreak,"src",cat(name,".c")),object,flags[:]); append(&objects,object)
    }
    flags=native_flags(); append(&flags,"-std=c++17","-Wall","-Wextra","-Werror","-I",join(freetype,"include"),"-I",join(harfbuzz,"src"),"-I",join(sheenbidi,"Headers"),"-I",join(unibreak,"src"))
    object=join(folder,"bridge.o"); compile(cxx,join(root,"tools/font_native/bridge.cpp"),object,flags[:]); append(&objects,object)
    name:="katla_font_native.dll" if ODIN_OS==.Windows else cat("libkatla_font_native",library_suffix())
    flags=native_flags(); when ODIN_OS==.Darwin { append(&flags,cat("-Wl,-install_name,@rpath/",name)) }
    library:=join(folder,name); shared(cxx,library,objects[:],flags[:])
    for file in files(join(root,"tools/font_native/licenses"),"") { copy_file(join(folder,"licenses",filepath.base(file)),file) }
    return library
}
