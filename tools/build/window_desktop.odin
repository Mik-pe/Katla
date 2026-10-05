#+build linux, windows
//! Source-pinned SDL window/input compilation without a generated build-system dependency.
package katla_build
import "core:path/filepath"
import "core:fmt"

build_window :: proc(folder:string)->string {
        mkdir(folder)
        upstream:=source("sdl3","https://github.com/libsdl-org/SDL.git","7f3ae3d57459e59943a4ecfefc8f6277ec6bf540")
        config:=join(folder,"config"); mkdir(config)
        copy_file(join(config,"SDL_build_config.h"),join(root,"tools/window_native/config","windows.h" if ODIN_OS==.Windows else "linux.h"))
        mkdir(join(config,"SDL3")); write(join(config,"SDL3/SDL_revision.h"),`#define SDL_REVISION "release-3.2.28-7f3ae3d"`+"\n")
        flags:=native_flags(); append(&flags,"-std=c11","-DSDL_BUILD_MAJOR_VERSION=3","-DSDL_BUILD_MINOR_VERSION=2","-DSDL_BUILD_MICRO_VERSION=28","-I",config,"-I",join(upstream,"include"),"-I",join(upstream,"src"),"-I",join(upstream,"include/build_config"))
        sources:=make([dynamic]string)
        for directory in ([]string{"","atomic","audio","audio/dummy","camera","camera/dummy","core","cpuinfo","dialog","dynapi","events","io","io/generic","filesystem","gpu","joystick","joystick/dummy","haptic","haptic/dummy","hidapi","locale","main","misc","power","process","render","sensor","sensor/dummy","stdlib","storage","thread","time","timer","tray","video","video/yuv2rgb","libm","video/dummy","storage/generic"}) {
            append(&sources,..files(join(upstream,"src",directory),".c",false))
        }
        link_flags:=native_flags()
        when ODIN_OS==.Linux {
            packages:=[]string{"x11","xext","xrandr","xcursor","xfixes","xi","xscrnsaver","wayland-client","wayland-egl","wayland-cursor","xkbcommon","dbus-1","ibus-1.0","egl"}
            command:=make([dynamic]string); append(&command,"pkg-config","--cflags"); append(&command,..packages); append(&flags,..fields(capture(command[:])))
            command[1]="--libs"; append(&link_flags,..fields(capture(command[:])))
            append(&link_flags,"-ldl","-lm","-pthread","-Wl,-soname,libSDL3.so.0","-Wl,--no-undefined")
            for directory in ([]string{"main/generic","core/linux","core/unix","filesystem/unix","filesystem/posix","locale/unix","misc/unix","power/linux","time/unix","timer/unix","loadso/dlopen","thread/pthread","video/x11","video/wayland","process/posix","dialog/dummy","tray/dummy"}) { append(&sources,..files(join(upstream,"src",directory),".c",false)) }
            protocols:=join(folder,"protocols"); mkdir(protocols); append(&flags,"-I",protocols)
            for xml in files(join(upstream,"wayland-protocols"),".xml",false) {
                name:=filepath.stem(xml); header:=join(protocols,cat(name,"-client-protocol.h")); file:=join(protocols,cat(name,"-protocol.c"))
                run({"wayland-scanner","client-header",xml,header}); run({"wayland-scanner","private-code",xml,file}); append(&sources,file)
            }
        } else when ODIN_OS==.Windows {
            for directory in ([]string{"core/windows","main/windows","io/windows","filesystem/windows","locale/windows","misc/windows","time/windows","timer/windows","loadso/windows","thread/windows","video/windows","video/offscreen","power/windows","process/windows","dialog/windows","tray/windows"}) { append(&sources,..files(join(upstream,"src",directory),".c",false)) }
            for file in ([]string{"thread/generic/SDL_syscond.c","thread/generic/SDL_sysrwlock.c"}) { append(&sources,join(upstream,"src",file)) }
            append(&flags,"-DDLL_EXPORT")
            for name in ([]string{"kernel32","user32","gdi32","winmm","imm32","ole32","oleaut32","version","uuid","advapi32","setupapi","shell32"}) { append(&link_flags,cat("-l",name)) }
        }
        objects:=make([dynamic]string)
        for file,i in sources { object:=join(folder,"objects",fmt.aprintf("%d-%s.o",i,filepath.stem(file))); compile(cc,file,object,flags[:]); append(&objects,object) }
        sdl:=join(folder,"SDL3.dll" if ODIN_OS==.Windows else "libSDL3.so.0"); shared(cc,sdl,objects[:],link_flags[:])
        bridge_flags:=native_flags(); append(&bridge_flags,"-std=c11","-DSDL_MAIN_HANDLED","-Wall","-Wextra","-Werror","-I",join(upstream,"include"))
        object:=join(folder,"bridge.o"); compile(cc,join(root,"tools/window_native/bridge.c"),object,bridge_flags[:])
        bridge_link:=native_flags(); append(&bridge_link,sdl if ODIN_OS==.Linux else join(folder,"SDL3.lib"))
        when ODIN_OS==.Linux { append(&bridge_link,"-Wl,-rpath,$ORIGIN") }
        library:=join(folder,"katla_window_native.dll" if ODIN_OS==.Windows else "libkatla_window_native.so"); shared(cc,library,{object},bridge_link[:])
        if build_smoke {
            append(&bridge_flags,"-I",join(root,"tools/window_native"))
            smoke:=join(folder,"smoke.o"); compile(cc,join(root,"tools/window_native/smoke.c"),smoke,bridge_flags[:]); finish_compiles()
            smoke_command:=make([dynamic]string); append(&smoke_command,cc,smoke,object); append(&smoke_command,..bridge_link[:]); append(&smoke_command,"-o",join(folder,cat("katla_window_smoke",executable_suffix())))
            when ODIN_OS==.Windows { append(&smoke_command,"-fuse-ld=lld") }
            run(smoke_command[:])
        }
        copy_file(join(folder,"SDL-LICENSE.txt"),join(upstream,"LICENSE.txt")); return library
}
