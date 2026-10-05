//! Build, test and launch Katla with Odin and explicit native compiler commands.
package katla_build
import "core:os"
import "core:strings"
import "core:fmt"
import "core:path/filepath"
import "core:encoding/json"

Options :: struct { tests,optimize,dependencies_only,objects_only,no_build,dry_run:bool,dependency,backend,project,resources,vulkan_loader:string,arguments:[]string }
Mode :: struct { host,architecture:string,sanitize:bool }
Manifest :: struct {
    version:int,host,architecture,root,odin,odin_report:string,sanitize:bool,
    executable:Maybe(string),paths:map[string]string,artifact_sha256:map[string]string,
    foreign_defines:[]string,tests_passed:bool,test_packages:[]string,editor_entrypoint_built:bool,
}
CPU_PACKAGES :: []string{"app","app/render","app/editor","gfx","gfx/shader_tests","script","audio","ui","image","resources","agent/host","deps/font_native"}

foreign_defines :: proc(paths:map[string]string)->[]string {
    result:[dynamic]string
    for item in ([][2]string{{"AUDIO_LIBRARY","audio"},{"CGLTF_LIBRARY","cgltf"},{"TOML_LIBRARY","toml_native"}}) {
        relative,error:=filepath.rel(join(root,"odin/deps",item[1]),paths[item[0]]); require(error==.None,"Cannot relate foreign library")
        append(&result,cat("-define:",item[0],"=",replace(relative,"\\","/")))
    }
    for name in ([]string{"BOX3D_LIBRARY","LUAU_LIBRARY","FONT_LIBRARY","SHADER_COMPILER"}) { append(&result,cat("-define:",name,"=",paths[name])) }
    return result[:]
}
select_compilers :: proc() {
    odin=find_tool(odin); report=capture({odin,"report"}); fmt.println(report)
    cc=env("CC","clang"); cxx=env("CXX","clang++"); ar=find_tool(env("AR","llvm-ar" if ODIN_OS==.Windows else "ar"))
    if sanitize {
        _,llvm,found:=cut(report,"LLVM "); require(found,"Cannot identify Odin LLVM runtime")
        major,_,_:=cut(llvm,".")
        when ODIN_OS==.Darwin {
            if env("CC","")=="" { cc=cat("/opt/homebrew/opt/llvm@",major,"/bin/clang") }
            if env("CXX","")=="" { cxx=cat("/opt/homebrew/opt/llvm@",major,"/bin/clang++") }
        } else {
            if env("CC","")=="" { cc=cat("clang-",major) }
            if env("CXX","")=="" { cxx=cat("clang++-",major) }
        }
        cc=find_tool(cc); cxx=find_tool(cxx)
        for compiler in ([]string{cc,cxx}) {
            version:=capture({compiler,"--version"})
            require(!strings.contains(version,"Apple clang") && strings.contains(version,cat("clang version ",major,".")),"ASan requires non-Apple Clang and Clang++ matching Odin LLVM")
        }
        when ODIN_OS==.Darwin {
            set_env("PATH",cat(filepath.dir(cc),":",env("PATH","")))
            driver:=find_tool("clang"); version:=capture({driver,"--version"})
            require(!strings.contains(version,"Apple clang") && strings.contains(version,cat("clang version ",major,".")),"Odin's Clang linker driver must match its LLVM sanitizer runtime")
        }
    } else { cc=find_tool(cc); cxx=find_tool(cxx) }
}
cpu_leak_environment :: proc() {
    if !sanitize { return }
    options:[dynamic]string
    for part in strings.split(env("ASAN_OPTIONS",""),":") { if part!="" && !strings.has_prefix(part,"detect_leaks=") { append(&options,part) } }
    append(&options,"detect_leaks=1"); set_env("ASAN_OPTIONS",strings.join(options[:],":"))
    when ODIN_OS==.Darwin {
        suppression:=join(output,"cpu-external-cfprefs.lsan"); write(suppression,"leak:CFPrefsPlistSource\nleak:CFPrefsSearchListSource\n")
        clear(&options)
        for part in strings.split(env("LSAN_OPTIONS",""),":") { if part!="" && !strings.has_prefix(part,"suppressions=") { append(&options,part) } }
        append(&options,cat("suppressions=",suppression)); set_env("LSAN_OPTIONS",strings.join(options[:],":"))
    }
}
build :: proc(options:Options) {
    mkdir(output); mode:=Mode{host(),architecture(),sanitize}; mode_file:=join(output,"mode.json")
    if os.is_file(mode_file) {
        previous:Mode; require(json.unmarshal(read(mode_file),&previous)==nil && previous==mode,"Output belongs to a different host or sanitizer mode")
    }
    write_json(mode_file,mode); remove(join(output,"build.json")); select_compilers()
    paths:=make(map[string]string); deps:=join(output,"deps"); mkdir(deps)
    paths["AUDIO_LIBRARY"]=build_audio(join(deps,"audio"))
    paths["CGLTF_LIBRARY"]=build_gltf(join(deps,"cgltf"))
    paths["TOML_LIBRARY"]=build_toml(join(deps,"toml"))
    paths["BOX3D_LIBRARY"]=build_box(join(deps,"box",cat("libkatla_box3d",library_suffix())))
    paths["LUAU_LIBRARY"]=build_luau(join(deps,"luau",cat("libkatla_luau",library_suffix())))
    paths["FONT_LIBRARY"]=build_fonts(join(deps,"fonts"))
    when ODIN_OS==.Windows || ODIN_OS==.Linux { paths["WINDOW_LIBRARY"]=build_window(join(deps,"window")) }
    cargo=find_tool(cargo)
    shader:=join(output,"shader-compiler"); run({cargo,"build","--manifest-path",join(root,"tools/naga_bridge/Cargo.toml"),"--locked","--target-dir",shader})
    paths["SHADER_COMPILER"]=join(shader,"debug",cat("katla-shader-compiler",executable_suffix()))
    shader_root:=join(output,"shaders"); remove(shader_root)
    for path in files(join(root,"odin/app/render/shaders"),"") {
        relative,error:=filepath.rel(join(root,"odin/app/render/shaders"),path); require(error==.None,"Cannot ship shader"); copy_file(join(shader_root,relative),path)
    }
    paths["SHADER_ROOT"]=shader_root
    defines:=foreign_defines(paths); flags:=make([dynamic]string)
    append(&flags,"-vet","-strict-style","-debug"); append(&flags,..defines)
    if sanitize { append(&flags,"-sanitize:address") }; if options.optimize || !sanitize { append(&flags,"-o:speed") }
    bin:=join(output,"bin"); mkdir(bin); editor:=join(bin,cat("katla",executable_suffix()))
    manifest:=Manifest{version=1,host=host(),architecture=architecture(),root=root,odin=odin,odin_report=report,sanitize=sanitize,paths=paths,artifact_sha256=make(map[string]string),foreign_defines=defines}
    if !options.dependencies_only {
        command:=make([dynamic]string); append(&command,odin,"build",join(root,"odin/katla")); append(&command,..flags[:])
        if options.objects_only { remove(join(output,"objects")); mkdir(join(output,"objects")); append(&command,"-build-mode:obj",cat("-out:",join(output,"objects/katla"))) }
        else { append(&command,cat("-out:",editor)) }
        run(command[:]); clear(&command)
        for name in ([]string{"mcp_stdio","mcp_proxy"}) {
            binary:=join(bin,cat("katla-",replace(name,"_","-"),executable_suffix()))
            append(&command,odin,"build",join(root,"odin",name)); append(&command,..flags[:]); append(&command,cat("-out:",binary)); run(command[:]); clear(&command)
            manifest.artifact_sha256[binary]=digest(read(binary))
        }
        if !options.objects_only { manifest.executable=editor; manifest.editor_entrypoint_built=true; manifest.artifact_sha256[editor]=digest(read(editor)) }
        else { for path in files(join(output,"objects"),"") { manifest.artifact_sha256[path]=digest(read(path)) } }
    }
    if options.tests {
        cpu_leak_environment()
        for name in CPU_PACKAGES {
            command:=make([dynamic]string); append(&command,odin,"test",join(root,"odin",name),"-all-packages"); append(&command,..flags[:])
            append(&command,"-define:ODIN_TEST_THREADS=1","-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true",cat("-out:",join(bin,cat(replace(name,"/","-"),"-tests",executable_suffix())))); run(command[:])
        }
        for tool_package in ([]string{"tools/author","tools/wire"}) {
        command:=[]string{odin,"test",join(root,tool_package),"-vet","-strict-style","-define:ODIN_TEST_THREADS=1","-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true",cat("-out:",join(bin,cat(replace(tool_package,"/","-"),"-tests",executable_suffix())))}
        extra:=make([dynamic]string); append(&extra,..command); if sanitize { append(&extra,"-sanitize:address") }; run(extra[:])
        }
        packages:=make([dynamic]string); append(&packages,..CPU_PACKAGES); append(&packages,"../tools/author","../tools/wire")
        manifest.tests_passed=true; manifest.test_packages=packages[:]
    }
    for name,path in paths {
        if name=="SHADER_ROOT" { for file in files(path,"") { manifest.artifact_sha256[file]=digest(read(file)) } }
        else { require(os.is_file(path),cat("Missing native build output: ",path)); manifest.artifact_sha256[path]=digest(read(path)) }
    }
    for file in files(join(deps,"fonts/fonts"),"") { manifest.artifact_sha256[file]=digest(read(file)) }
    when ODIN_OS==.Windows || ODIN_OS==.Linux {
        for file in files(join(deps,"window"),"") { if strings.contains(filepath.base(file),"SDL3") { manifest.artifact_sha256[file]=digest(read(file)) } }
    }
    temporary:=join(output,"build.json.tmp"); write_json(temporary,manifest); require(os.rename(temporary,join(output,"build.json"))==nil,"Cannot publish completed build")
    fmt.println("Build manifest:",join(output,"build.json"))
}
verified_manifest :: proc()->Manifest {
    manifest:Manifest
    require(os.is_file(join(output,"build.json")),"No completed build; run odin run tools/build first")
    require(json.unmarshal(read(join(output,"build.json")),&manifest)==nil,"Invalid build receipt")
    require(manifest.version==1 && manifest.root==root && manifest.host==host() && manifest.architecture==architecture() && manifest.sanitize==sanitize,"Receipt belongs to another checkout, host or sanitizer mode")
    require(manifest.executable!=nil && len(manifest.artifact_sha256)>0,"Receipt has no editor executable")
    require(filepath.is_abs(manifest.odin) && os.is_file(manifest.odin),"Receipt has no compiler")
    for name in ([]string{"AUDIO_LIBRARY","CGLTF_LIBRARY","TOML_LIBRARY","BOX3D_LIBRARY","LUAU_LIBRARY","FONT_LIBRARY","SHADER_COMPILER","SHADER_ROOT"}) {
        path,ok:=manifest.paths[name]; require(ok && filepath.is_abs(path),cat("Receipt lacks absolute artifact: ",name))
        if name=="SHADER_ROOT" { require(os.is_dir(path) && len(files(path,""))>0,"Receipt has no shaders"); for file in files(path,"") { require(file in manifest.artifact_sha256,"Shader missing from artifact hashes") } }
        else { require(path in manifest.artifact_sha256,"Native artifact missing from hashes") }
    }
    editor,_:=manifest.executable.?; require(filepath.is_abs(editor) && editor in manifest.artifact_sha256,"Editor missing from artifact hashes")
    expected:=foreign_defines(manifest.paths); require(len(expected)==len(manifest.foreign_defines),"Foreign library defines differ from artifacts")
    for define,index in expected { require(define==manifest.foreign_defines[index],"Foreign library define differs from verified artifact") }
    for path,hash in manifest.artifact_sha256 { require(os.is_file(path) && digest(read(path))==hash,cat("Artifact changed or missing: ",path)) }
    return manifest
}
launch :: proc(options:Options) {
    if !options.no_build { build(options) }; manifest:=verified_manifest(); editor,_:=manifest.executable.?; paths:=manifest.paths
    command:=make([dynamic]string); append(&command,editor,"--backend",options.backend,"--project",options.project,"--resources",options.resources)
    for pair in ([][2]string{{"--shader-compiler","SHADER_COMPILER"},{"--shader-root","SHADER_ROOT"},{"--font-library","FONT_LIBRARY"},{"--luau-library","LUAU_LIBRARY"},{"--box-library","BOX3D_LIBRARY"}}) { append(&command,pair[0],paths[pair[1]]) }
    if window,ok:=paths["WINDOW_LIBRARY"]; ok {
        append(&command,"--window-library",window)
        when ODIN_OS==.Windows { set_env("PATH",cat(filepath.dir(window),";",env("PATH",""))) }
    }
    if options.backend=="metal" { set_env("MTL_DEBUG_LAYER","1"); set_env("METAL_DEVICE_WRAPPER_TYPE","1") }
    else {
        loader:=options.vulkan_loader
        if loader=="" { loader=env("KATLA_VULKAN_LIBRARY","vulkan-1.dll" if ODIN_OS==.Windows else "libvulkan.so.1" if ODIN_OS==.Linux else "libvulkan.dylib") }
        append(&command,"--vulkan-loader",loader)
    }
    append(&command,..options.arguments)
    if options.dry_run { fmt.println("+",strings.join(command[:]," ")); return }; run(command[:])
}
main :: proc() {
    root=absolute("."); require(os.is_file(join(root,"odin/katla/config.odin")),"Run from Katla's repository root")
    odin=env("ODIN","odin"); cargo=env("CARGO","cargo")
    export_source,export_entry,export_stage,export_compiler:string
    options:=Options{backend="metal" if ODIN_OS==.Darwin else "vulkan",project=root,resources=join(root,"resources")}; command:="build"
    for i:=1; i<len(os.args); i+=1 {
        arg:=os.args[i]
        switch arg {
        case "build","run","compiler","validate","shader-export": command=arg
        case "gpu","render","ui","shader","audio","physics","luau","processes","host","proxy","mcp","socket","http": validation.suite=arg
        case "--": options.arguments=os.args[i+1:]; i=len(os.args)
        case "--tests": options.tests=true
        case "--native-metal": validation.metal=true
        case "--native-vulkan": validation.vulkan=true
        case "--native-surface": validation.surface=true
        case "--vulkan-baseline": validation.baseline=true
        case "--vulkan-probe-array-capabilities": validation.probe=true
        case "--particles": validation.particles=true
        case "--skip-cpu-tests": validation.skip_cpu=true
        case "--cpu-only": validation.cpu_only=true
        case "--native-leaks": validation.native_leaks=true
        case "--skip-shader-checks": validation.skip_shader=true
        case "--skip-font-checks": validation.skip_fonts=true
        case "--native": validation.native=true
        case "--switch-default": validation.switch_default=true
        case "--slow": validation.slow=true
        case "--sanitize": sanitize=true
        case "--build-smoke": build_smoke=true
        case "--optimize": options.optimize=true
        case "--dependencies-only": options.dependencies_only=true
        case "--objects-only": options.objects_only=true
        case "--no-build": options.no_build=true
        case "--dry-run": options.dry_run=true
        case "--help": fmt.println("odin run tools/build -- [build|run|validate SUITE|shader-export] [--tests] [--sanitize] [--output DIR] [--dependencies-only|--objects-only] [--no-build] [--backend metal|vulkan] [--project DIR] [--resources DIR] [--vulkan-loader FILE] [--dependency audio|gltf|toml|box|luau|fonts|window] [-- app arguments]. Validation suites: gpu, render, ui, shader, audio, physics, luau, processes, http, mcp, socket, host, proxy. Native selection: --native-metal, --native-vulkan, --native-surface; --build-manifest FILE reuses verified artifacts."); return
        case "--output","--output-dir","--build-dir","--odin","--cargo","--dependency","--backend","--project","--resources","--vulkan-loader","--vulkan-library","--vulkan-icd","--glslc","--build-manifest","--native-asan-leaks","--source","--entry","--stage","--compiler":
            i+=1; require(i<len(os.args),cat("Missing value for ",arg)); value:=os.args[i]
            switch arg {
            case "--output","--build-dir": output=absolute(value)
            case "--output-dir": validation.output=absolute(value)
            case "--odin": odin=value
            case "--cargo": cargo=value
            case "--dependency": options.dependency=value
            case "--backend": require(value=="metal" || value=="vulkan" || (command=="validate" && value=="both"),"Invalid backend"); options.backend=value; validation.metal=value=="metal"; validation.vulkan=value=="vulkan" || value=="both"; validation.metal=validation.metal || value=="both"
            case "--project": options.project=absolute(value)
            case "--resources": options.resources=absolute(value)
            case "--vulkan-loader","--vulkan-library": options.vulkan_loader=value; validation.loader=absolute(value)
            case "--vulkan-icd": validation.icd=absolute(value)
            case "--glslc": validation.glslc=value
            case "--build-manifest": validation.manifest=value
            case "--native-asan-leaks": require(value=="check" || value=="external-driver","Invalid audio leak mode"); validation.audio_external_driver=value=="external-driver"
            case "--source": export_source=value
            case "--entry": export_entry=value
            case "--stage": export_stage=value
            case "--compiler": export_compiler=absolute(value)
            }
        case: fail(cat("Unknown build option: ",arg))
        }
    }
    if output=="" { output=default_output() }
    if command=="shader-export" { require(export_source!="" && export_entry!="" && export_stage!="" && export_compiler!="","Shader export requires source, entry, stage and compiler"); export_shader(export_compiler,export_source,export_entry,export_stage,output); return }
    if command=="validate" { require(validation.suite!="","Choose a validation suite"); validate(options); return }
    if command=="compiler" { select_compilers(); fmt.println(cc); return }
    if options.dependency!="" {
        select_compilers()
        switch options.dependency {
        case "audio": fmt.println(build_audio(output))
        case "gltf": fmt.println(build_gltf(output))
        case "toml": fmt.println(build_toml(output))
        case "box": fmt.println(build_box(output))
        case "luau": fmt.println(build_luau(output))
        case "fonts": fmt.println(build_fonts(output))
        case "window": fmt.println(build_window(output))
        case: fail("Unknown native dependency")
        }
    } else if command=="run" { launch(options) } else { build(options) }
}
