//! Explicit application paths select backend dependencies without reading provider credentials.
package main
import "core:strings"
import "core:strconv"
import "core:math"
import "core:fmt"
import "core:os"
import "core:path/filepath"

Config :: struct { shader_root,shader_compiler,font_library,resource_root,project_root,scene_path,vulkan_loader,luau_library,box_library,mcp_socket,preferences_directory,screenshot,window_library:string,backend:string,frames:int,help,headless,single_frame,gpu_validation,check_black_frames,dump_layout,dump_graph:bool,dump_layout_file,dump_graph_file,ui_test,interaction_test:string,camera:[3]f32,has_camera,owned_screenshot:bool }
config_parse :: proc(arguments:[]string)->(Config,bool) {
    result:=Config{backend="metal",project_root=".",resource_root="resources"}
    when ODIN_OS!=.Darwin { result.backend="vulkan" }
    explicit_frames:=false
    for index:=1;index<len(arguments);index+=1 {
        name:=arguments[index]
        if name=="--help" { result.help=true; return result,true }
        switch name {
        case "--headless": result.headless=true; continue
        case "-s","--single-frame": result.single_frame=true; continue
        case "-v","--gpu-validation": result.gpu_validation=true; continue
        case "--check-black-frames": result.check_black_frames=true; continue
        case "--dump-layout": result.dump_layout=true; continue
        case "--dump-render-graph": result.dump_graph=true; continue
        }
        if index+1>=len(arguments) { return {},false }; index+=1; value:=arguments[index]
        switch name {
        case "--backend":if value!="metal" && value!="vulkan" { return {},false }; result.backend=value
        case "--shader-compiler":result.shader_compiler=value
        case "--shader-root":result.shader_root=value
        case "--window-library":result.window_library=value
        case "--font-library":result.font_library=value
        case "--resources":result.resource_root=value
        case "--project":result.project_root=value
        case "--preferences":result.preferences_directory=value
        case "--screenshot":result.screenshot=value
        case "--scene":result.scene_path=value
        case "--vulkan-loader":result.vulkan_loader=value
        case "--luau-library":result.luau_library=value
        case "--box-library":result.box_library=value
        case "--mcp-socket":result.mcp_socket=value
        case "--dump-layout-file":result.dump_layout_file=value
        case "--dump-render-graph-file":result.dump_graph_file=value
        case "--ui-test":result.ui_test=value; result.headless=true
        case "--interaction-test":result.interaction_test=value; result.headless=true
        case "--camera":
            parts:=strings.split(value,","); defer delete(parts)
            if len(parts)!=3 { return {},false }
            for part,i in parts { number,valid:=strconv.parse_f32(strings.trim_space(part)); if !valid || math.is_nan(number) || math.is_inf(number) { return {},false }; result.camera[i]=number }
            if result.camera[1]<=-89.9 || result.camera[1]>=89.9 || result.camera[2]<.05 || result.camera[2]>100000 { return {},false }; result.has_camera=true
        case "--frames":explicit_frames=true;number,valid:=strconv.parse_int(value); if !valid || number<1 || number>100000 { return {},false }; result.frames=number
        case:return {},false
        }
    }
    if !explicit_frames {
        if result.headless || result.single_frame { result.frames=100 }
        if result.ui_test!="" { result.frames=120 }
        if result.interaction_test!="" { result.frames=180 }
        if result.dump_layout || result.dump_graph || result.dump_layout_file!="" || result.dump_graph_file!="" { result.frames=1 }
    }
    if result.headless && result.screenshot=="" && result.ui_test=="" && result.interaction_test=="" {
        temporary,error:=os.temp_directory(context.allocator);if error!=nil { return {},false };defer delete(temporary)
        result.screenshot,_=filepath.join({temporary,"katla_screenshot.png"});result.owned_screenshot=true
    }
    return result,result.shader_compiler!="" && result.font_library!="" && (result.backend!="vulkan" || result.vulkan_loader!="") && strings.trim_space(result.project_root)!=""
}

config_destroy :: proc(config:^Config) { if config.owned_screenshot { delete(config.screenshot);config.screenshot="";config.owned_screenshot=false } }

config_help :: proc() {
    // Dependencies are explicit; source-pinned launch scripts supply their built artifact paths.
    fmt.println(`Katla Odin editor
  --project DIRECTORY --resources DIRECTORY --preferences DIRECTORY
  --scene SCENE.katla                 Load an explicit scene; otherwise discover assets/scenes/default.katla
  --backend metal|vulkan              Vulkan is the portable default
  --shader-compiler EXE --shader-root DIRECTORY --font-library LIB
  --luau-library LIB --box-library LIB --vulkan-loader LIB --window-library LIB
  --headless                         Render offscreen without an OS window or surface
  -s, --single-frame                  Accept 100 frames and exit
  --frames N                         Accept exactly N combined frames
  -v, --gpu-validation               Enable native GPU validation before device creation
  --camera YAW,PITCH,DISTANCE         Orbit camera angles in degrees
  --screenshot PNG                   Capture the paired active viewport
  --check-black-frames                Check actual completed center pixels each accepted frame
  --dump-layout [--dump-layout-file JSON]
  --dump-render-graph [--dump-render-graph-file JSON]
  --ui-test DIRECTORY                Capture retained editor states offscreen
  --interaction-test DIRECTORY       Route synthetic input, capture native output and write checks
  --mcp-socket ABSOLUTE_PATH          Private editor endpoint
  --help`)
}
