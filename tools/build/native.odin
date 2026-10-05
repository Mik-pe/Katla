//! Compile the remaining narrow C/C++ ABIs directly; images and decompression use Odin.
package katla_build
import "core:os"
import "core:path/filepath"
import "core:fmt"

build_audio :: proc(folder:string)->string {
    mkdir(folder)
    verify(join(root,"tools/audio_native/miniaudio.h"),"ac7af4de748b7e26b777f37e01cee313a308a7296a3eb080e2906b320cc55c89")
    verify(join(root,"tools/audio_native/stb_vorbis.c"),"4c7cb2ff1f7011e9d67950446b7eb9ca044f2e464d76bfbb0b84dd2e23e65636")
    flags:=native_flags(); append(&flags,"-std=c11","-Wall","-Wextra","-Werror")
    object:=join(folder,"audio_native.o"); compile(cc,join(root,"tools/audio_native/audio_native.c"),object,flags[:]); objects:=make([dynamic]string); append(&objects,object)
    when ODIN_OS==.Darwin { object=join(folder,"audio_route.o"); compile(cc,join(root,"tools/audio_native/audio_route_darwin.c"),object,flags[:]); append(&objects,object) }
    library:=join(folder,"libaudio_native.a"); archive(library,objects[:]); return library
}
build_gltf :: proc(folder:string)->string {
    mkdir(folder); flags:=native_flags(); append(&flags,"-std=c99","-Wall","-Wextra","-Werror")
    object:=join(folder,"cgltf.o"); compile(cc,join(root,"tools/cgltf/cgltf.c"),object,flags[:])
    library:=join(folder,"libcgltf.a"); archive(library,{object}); return library
}
build_toml :: proc(folder:string)->string {
    mkdir(folder); flags:=native_flags(); append(&flags,"-std=c17","-Wall","-Wextra","-Werror"); objects:=make([dynamic]string)
    for relative in ([]string{"katla_toml.c","vendor/tomlc17.c"}) {
        object:=join(folder,cat(filepath.stem(relative),".o")); compile(cc,join(root,"tools/toml_native",relative),object,flags[:]); append(&objects,object)
    }
    library:=join(folder,"libtoml.a"); archive(library,objects[:]); return library
}
build_box :: proc(library:string)->string {
    folder:=filepath.dir(library); mkdir(folder)
    upstream:=source("box3d","https://github.com/erincatto/box3d.git","8441b4a06d6d09dcfb0b0f704df4d847d1437b92")
    listing:=BOX_SOURCES
    adapted:=join(folder,"adapted"); adapt_box(upstream,adapted)
    flags:=native_flags(); append(&flags,"-std=c17","-ffp-contract=off","-DBOX3D_VALIDATE","-Dbox3d_EXPORTS","-I",join(upstream,"include"),"-I",join(upstream,"src"))
    objects:=make([dynamic]string)
    for name in listing {
        file:=join(adapted,name); if !os.is_file(file) { file=join(upstream,"src",name) }
        object:=join(folder,"objects",cat(filepath.stem(name),".o")); compile(cc,file,object,flags[:]); append(&objects,object)
    }
    bridge:=join(folder,"objects/bridge.o"); compile(cc,join(root,"tools/box3d/bridge.c"),bridge,flags[:]); append(&objects,bridge)
    link_flags:=native_flags(); when ODIN_OS!=.Windows { append(&link_flags,"-lm","-pthread") }
    shared(cc,library,objects[:],link_flags[:]); return library
}
build_luau :: proc(library:string)->string {
    folder:=filepath.dir(library); mkdir(folder)
    upstream:=source("luau","https://github.com/luau-lang/luau.git","b968ef742741bb2b703afc3b3c53f06608c87481")
    sources:=make([dynamic]string); flags:=native_flags()
    append(&flags,"-x","c++","-std=c++17","-DLUA_USE_LONGJMP=1",`-DLUA_API=extern "C" __declspec(dllexport)` if ODIN_OS==.Windows else `-DLUA_API=extern "C"`)
    for name in ([]string{"Common","Ast","Compiler","VM"}) {

        append(&flags,"-I",join(upstream,name,"include"))
    }
    for file in LUAU_SOURCES { append(&sources,join(upstream,file)) }
    append(&flags,"-I",join(upstream,"VM/src")); append(&sources,join(root,"tools/luau_native/bridge.cpp"),join(root,"tools/luau_native/protected.cpp"))
    objects:=make([dynamic]string)
    for file,i in sources {
        object:=join(folder,"objects",fmt.aprintf("%d-%s.o",i,filepath.stem(file))); compile(cxx,file,object,flags[:]); append(&objects,object)
    }
    link_flags:=native_flags(); when ODIN_OS!=.Windows { append(&link_flags,"-pthread") }
    shared(cxx,library,objects[:],link_flags[:]); return library
}
