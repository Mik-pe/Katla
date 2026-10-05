//! Native acceptance orchestration shares the builder's verified artifacts and compiler.
package katla_build
import "core:os"
import "core:time"
import "core:path/filepath"
import "core:fmt"

Validation :: struct {
    suite:string,
    metal,vulkan,surface,baseline,probe,particles,skip_cpu,skip_shader,skip_fonts,native,switch_default,native_leaks,audio_external_driver,slow,cpu_only:bool,
    loader,icd,glslc,manifest,target,output:string,
}
validation_flags: [dynamic]string
validation_manifest: Manifest
validation: Validation

validation_build :: proc(source_package,name:string,tests:=false,extra:[]string=nil)->string {
    binary:=join(output,cat(name,executable_suffix())); command:=make([dynamic]string)
    append(&command,odin,"build",join(root,source_package),cat("-out:",binary),"-vet","-strict-style","-debug")
    append(&command,..validation_flags[:]); append(&command,..extra)
    if tests { append(&command,"-build-mode:test","-all-packages","-define:ODIN_TEST_THREADS=1","-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true") }
    else { append(&command,"-o:speed") }
    run(command[:]); return binary
}
validation_test :: proc(source_package,name:string,extra:[]string=nil) { cpu_leak_environment(); run({validation_build(source_package,name,true,extra)}) }
validation_gpu_environment :: proc() {
    when ODIN_OS==.Darwin { set_env("MTL_DEBUG_LAYER","1"); set_env("METAL_DEVICE_WRAPPER_TYPE","1") }
    if validation.icd!="" { set_env("VK_ICD_FILENAMES",validation.icd) }
    if sanitize { set_env("ASAN_OPTIONS","detect_leaks=1:abort_on_error=1" if validation.native_leaks else "detect_leaks=0:abort_on_error=1"); fmt.println("Native GPU ASan: address and Odin ownership checks; external driver process-exit leak boundary") }
}
validation_native :: proc(source_package,name:string,args:[]string) {
    validation_gpu_environment(); binary:=validation_build(source_package,name); command:=make([dynamic]string); append(&command,binary); append(&command,..args); run_at(command[:],root,180*time.Second)
}
validation_shader_checks :: proc() {
    if validation.skip_shader { return }
    manifest:=join(root,"tools/naga_bridge/Cargo.toml")
    run({cargo,"fmt","--manifest-path",manifest,"--check"})
    for operation in ([]string{"test","clippy"}) {
        command:=make([dynamic]string); append(&command,cargo,operation,"--manifest-path",manifest,"--locked","--target-dir",validation.target)
        if operation=="clippy" { append(&command,"--all-targets","--","-D","warnings") }; run(command[:])
    }
    validation_test("odin/gfx/shader_tests","shader-tests")
}
validation_gpu :: proc() {
    compiler:=validation_manifest.paths["SHADER_COMPILER"]
    if validation.probe {
        validation_gpu_environment(); binary:=validation_build("odin/gfx_vulkan_native","vulkan-native")
        process,error:=os.process_start({command={binary,"--probe-array-capabilities",validation.loader},stdout=os.stdout,stderr=os.stderr})
        require(error==nil,"Cannot start Vulkan capability probe"); state,wait_error:=os.process_wait(process,60*time.Second)
        if wait_error!=nil { _=os.process_kill(process); _,_=os.process_wait(process,os.TIMEOUT_INFINITE); fail("Vulkan probe deadline") }
        require(state.exited,"Vulkan probe failed to exit"); os.exit(int(state.exit_code))
    }
    validation_shader_checks(); validation_test("odin/gfx","core-tests"); validation_test("odin/gfx/spirv","spirv-tests")
    if validation.metal {
        validation_gpu_environment(); run({validation_build("odin/gfx/metal","metal-tests",true)})
        validation_native("odin/gfx_native","metal-native",{compiler})
    }
    if validation.vulkan {
        binaries:=make(map[string]string)
        for row in ([][3]string{{"fill","odin/gfx_native/shaders","comp"},{"params","odin/gfx_native/shaders","comp"},{"triangle","odin/gfx_vulkan_native/shaders","vert"},{"color","odin/gfx_vulkan_native/shaders","frag"},{"mesh","odin/gfx_vulkan_native/shaders","vert"},{"depth_sense","odin/gfx_vulkan_native/shaders","vert"},{"tint","odin/gfx_vulkan_native/shaders","frag"},{"sample","odin/gfx_vulkan_native/shaders","frag"},{"image","odin/gfx_vulkan_native/shaders","comp"},{"volume","odin/gfx_vulkan_native/shaders","comp"},{"sampler_policy","odin/gfx_vulkan_native/shaders","frag"},{"packed_formats","odin/gfx_vulkan_native/shaders","vert"}}) {
            path:=join(output,cat(row[0],".spv")); run({validation.glslc,"--target-env=vulkan1.3",join(root,row[1],cat(row[0],".",row[2])),"-o",path}); binaries[row[0]]=path
        }
        for row in ([][2]string{{"array","Fragment"},{"storage_array","Compute"}}) {
            basename:=join(output,row[0]); export_shader(compiler,join(root,"odin/gfx_shader_native/shaders",cat(row[0],".wgsl")),"main",row[1],basename); binaries[row[0]]=cat(basename,".spv")
        }
        args:=make([dynamic]string)
        for name in ([]string{"fill","params","triangle","color"}) { append(&args,binaries[name]) }; append(&args,validation.loader)
        append(&args,"--sampling",binaries["sampler_policy"],"--vertex-formats",binaries["packed_formats"],"--volume",binaries["volume"],"--images",binaries["sample"],binaries["image"],"--mesh",binaries["mesh"],binaries["tint"],"--arrays",binaries["array"],"--storage-arrays",binaries["storage_array"],"--depth-sense",binaries["depth_sense"],binaries["tint"])
        if validation.surface { append(&args,"--surface") }; if validation.baseline { append(&args,"--baseline") }
        validation_native("odin/gfx_vulkan_native","vulkan-native",args[:])
    }
    if validation.metal && validation.vulkan { validation_native("odin/gfx_shader_native","shader-native-reload",{compiler,validation.loader}) }
}
validation_render :: proc() {
    if !validation.skip_cpu { validation_test("odin/app/render","render-tests") }
    if !validation.metal && !validation.vulkan { return }
    resources:=join(output,"project/resources"); remove(resources)
    for path in files(join(root,"resources"),"") { relative,error:=filepath.rel(join(root,"resources"),path); require(error==.None,"Cannot snapshot resources"); copy_file(join(resources,relative),path) }
    compiler:=validation_manifest.paths["SHADER_COMPILER"]
    for backend in ([]string{"metal","vulkan"}) {
        if (backend=="metal" && !validation.metal) || (backend=="vulkan" && !validation.vulkan) { continue }
        for suffix in ([]string{"","-models","-window"}) {
            if suffix=="-window" && !validation.surface { continue }
            args:=make([dynamic]string); append(&args,compiler,cat(backend,suffix),join(output,cat(backend,"-", "scene.png" if suffix=="" else "models" if suffix=="-models" else "window.png")),resources)
            if backend=="vulkan" { append(&args,validation.loader) }
            validation_native("odin/examples/material_render","material-render",args[:])
        }
    }
    if validation.particles { validation_native("odin/examples/particles_render","particles-render",{compiler,validation.loader,validation_manifest.paths["LUAU_LIBRARY"],validation_manifest.paths["BOX3D_LIBRARY"],join(output,"project")}) }
    if validation.metal && validation.vulkan {
        validation_native("odin/examples/resize_atomic","resize-atomic",{compiler,validation.loader,resources})
        for name in ([]string{"render_features","material_brdf_native","model_sources_native","material_coverage_native"}) { validation_native(join("odin/examples",name),replace(name,"_","-"),{compiler,validation.loader}) }
        validation_native("odin/examples/material_preview_native","material-preview-native",{compiler,validation_manifest.paths["FONT_LIBRARY"],resources,validation.loader,"--both"})
        validation_native("odin/examples/texture_reload","texture-reload",{compiler,validation.loader,resources})
        validation_native("odin/app_render_shader_reload_native","shader-reload",{compiler,validation.loader,validation_manifest.paths["SHADER_ROOT"]})
    }
    if validation.metal { validation_native("odin/examples/image_precision_native","image-precision-native",{validation.loader if validation.vulkan else "--metal-only"}) }
}
validation_ui :: proc() {
    if !validation.skip_fonts { validation_test("odin/gfx","core-tests"); validation_test("odin/deps/font_native","font-tests",{cat("-define:FONT_RESOURCES=",join(root,"resources"))}) }
    if validation.cpu_only { return }
    if !validation.metal && !validation.vulkan { return }
    backend:="both" if validation.metal && validation.vulkan else "metal" if validation.metal else "vulkan"
    args:=[]string{validation_manifest.paths["SHADER_COMPILER"],validation_manifest.paths["FONT_LIBRARY"],join(root,"resources"),validation.loader,cat("--",backend)}
    validation_native("odin/app_render_ui_native","ui-picking-native",args)
    validation_native("odin/examples/thumbnails_native","thumbnails-native",args)
}
validate :: proc(options:Options) {
    require(!validation.audio_external_driver || (validation.suite=="audio" && validation.native && sanitize),"External audio driver leak mode requires audio --native --sanitize")
    require(!validation.switch_default || (validation.suite=="audio" && validation.native && ODIN_OS==.Darwin),"Default route replacement requires native macOS audio")
    require(!validation.native_leaks || (sanitize && (validation.metal || validation.vulkan)),"Native GPU leak audit requires a sanitized native backend")
    require(!validation.metal || (ODIN_OS==.Darwin && ODIN_ARCH==.arm64),"Native Metal requires macOS arm64 hardware")
    require(!validation.surface || (validation.vulkan || validation.metal),"Native surface needs a selected backend")
    require(!(validation.baseline || validation.probe) || validation.vulkan,"Vulkan baseline/probe needs --native-vulkan")
    require(!validation.probe || (!validation.metal && !validation.surface && !validation.baseline),"Capability probe is a separate Vulkan invocation")
    require(!validation.particles || (validation.metal && validation.vulkan),"Particles need both native adapters")
    require((validation.loader=="" && validation.icd=="") || validation.vulkan,"Explicit Vulkan paths require --native-vulkan")
    require(validation.suite!="gpu" || !validation.surface || (validation.vulkan && ODIN_OS==.Darwin && ODIN_ARCH==.arm64),"GPU surface fixture requires macOS arm64 and --native-vulkan")
    require(!(validation.suite=="render" || validation.suite=="ui") || (!validation.metal && !validation.vulkan) || (ODIN_OS==.Darwin && ODIN_ARCH==.arm64),"Native application/UI acceptance requires Darwin arm64 hardware")
    for path in ([]string{validation.loader,validation.icd}) { if path!="" { require(os.is_file(path),cat("Explicit Vulkan path missing: ",path)) } }
    if validation.loader=="" { validation.loader="libvulkan.dylib" if ODIN_OS==.Darwin else "vulkan-1.dll" if ODIN_OS==.Windows else "libvulkan.so.1" }
    if validation.glslc=="" { validation.glslc="glslc" }
    if validation.manifest!="" { output=filepath.dir(absolute(validation.manifest)) } else { build(options) }
    validation_manifest=verified_manifest()
    output=validation.output if validation.output!="" else join(root,"target/odin-validation",validation.suite,"asan" if sanitize else "normal")
    mkdir(output); odin=validation_manifest.odin; cargo=find_tool(cargo)
    validation.target=join(filepath.dir(validation_manifest.paths["SHADER_COMPILER"]),"..")
    append(&validation_flags,..validation_manifest.foreign_defines); if sanitize { append(&validation_flags,"-sanitize:address") }
    switch validation.suite {
    case "gpu": validation_gpu()
    case "render": validation_render()
    case "ui": validation_ui()
    case "shader": validation_shader_checks(); if validation.native { require(validation.metal && validation.vulkan,"Native shader replacement requires both backends"); validation_native("odin/gfx_shader_native","shader-native-reload",{validation_manifest.paths["SHADER_COMPILER"],validation.loader}) }
    case "audio":
        validation_test("odin/audio","audio-tests"); validation_test("odin/app","app-audio-tests")
        if validation.native {
            cpu_leak_environment(); if validation.audio_external_driver { set_env("ASAN_OPTIONS","detect_leaks=0:abort_on_error=1") }
            args:=make([dynamic]string); append(&args,validation_build("odin/audio_native","audio-native")); if validation.switch_default { append(&args,"--switch-default") }; run(args[:])
        }
    case "physics","luau": validation_test("odin/physics/box3d","box-tests"); validation_test("odin/script","script-tests"); validation_test("odin/app","native-app-tests")
    case "processes": validation_processes()
    case "mcp": validation_mcp()
    case "socket": validation_socket()
    case "http": validation_http()
    case "host": validation_host()
    case "proxy": validation_proxy()
    case: fail(cat("Unknown validation suite: ",validation.suite))
    }
    fmt.println("PASS Odin validation:",validation.suite)
}
