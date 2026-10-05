//! Build the pinned Odin compiler with the current native macOS LLVM runtime.
package katla_build
import "core:fmt"
import "core:strings"

bootstrap_odin :: proc(llvm_config:string) {
    require(!sanitize,"Bootstrap the compiler before instrumenting Katla")
    config:=find_tool(llvm_config)
    require(strings.has_prefix(capture({config,"--version"}),"22."),"Pinned Odin source bootstrap requires LLVM 22")
    compiler:=join(capture({config,"--bindir"}),"clang++")
    require(strings.contains(capture({compiler,"--version"}),"clang version 22."),"Odin bootstrap requires the matching Clang++")
    revision:="a2fb372b76e81ef31fbbc8a2cf2b4fdf5ac6c924"
    folder:=source("odin","https://github.com/odin-lang/Odin.git",revision)
    executable:=join(folder,"odin")
    command:=[]string{
        compiler,"src/main.cpp","src/libtommath.cpp","-std=c++17","-O3","-stdlib=libc++","-fno-exceptions",
        "-Wno-switch","-Wno-macro-redefined","-Wno-unused-value",
        "-D__STDC_CONSTANT_MACROS","-D__STDC_FORMAT_MACROS","-D__STDC_LIMIT_MACROS",
        "-DGIT_SHA=\"a2fb372\"","-DODIN_VERSION_RAW=\"dev-2026-09\"",
        "-I",capture({config,"--includedir"}),"-L",capture({config,"--libdir"}),
        "-isysroot",capture({"xcrun","--sdk","macosx","--show-sdk-path"}),
        "-Wl,-search_paths_first","-Wl,-headerpad_max_install_names",
        "-pthread","-lm","-liconv","-ldl","-framework","System","-lLLVM","-o",executable,
    }
    run_at(command,folder)
    compiler_report:=capture({executable,"report"})
    require(strings.contains(compiler_report,"LLVM 22."),"Built Odin does not use the selected LLVM runtime")
    fmt.println(compiler_report)
    fmt.println(executable)
}
