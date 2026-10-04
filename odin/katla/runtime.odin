//! One platform-independent owner loop publishes cameras, UI and captures through the same native graph.
package main
import app "../app"
import editor_app "../app/editor"
import document "../app/document"
import prefs "../app/preferences"
import assets "../app/assets"
import window "../app/window"
import render "../app/render"
import shader "../gfx/shader"
import gfx "../gfx"
import ui "../ui"
import ecs "../ecs"
import editor "../editor"
import "core:fmt"
import "core:mem"
import "base:runtime"
import "core:os"
import "core:time"
import "core:math"
import "core:log"
import "core:encoding/base64"
import "core:path/filepath"

Backend_API :: struct($R:typeid) {
    capture_enable:proc(^R,bool)->gfx.Gpu_Error,
    capture_snapshot:proc(^R,u64,mem.Allocator)->(gfx.Capture_Snapshot,bool),
    gpu:render.GPU_Ops(R),ui:render.UI_GPU_Ops(R),picking:render.Picking_Ops(R),particles:render.Particle_GPU_Ops(R),
    attach:proc(^R,gfx.Surface_Desc)->gfx.Gpu_Error,
    resize:proc(^R,u32,u32)->gfx.Gpu_Error,
    detach:proc(^R)->gfx.Gpu_Error,
    acquire_surface:proc(^R)->(gfx.Surface_Frame,gfx.Surface_Result,gfx.Gpu_Error),
    abort_surface:proc(^R,gfx.Surface_Frame)->gfx.Gpu_Error,
    present:proc(^R,gfx.Surface_Frame,gfx.Submission)->(gfx.Present_Outcome,gfx.Gpu_Error),
}
run_editor :: proc(renderer:^$R,api:Backend_API(R),config:Config,compiler:^shader.Compiler,fonts:^render.UI_Font_System,down:bool)->bool {
    previous_logger:=context.logger
    terminal_logger:log.Logger
    if previous_logger.procedure==nil || previous_logger.procedure==log.nil_logger_proc || previous_logger.procedure==runtime.default_logger_proc { terminal_logger=log.create_console_logger() }
    context.logger=terminal_logger if terminal_logger.procedure!=nil else previous_logger
    console_logger:editor_app.Console_Logger
    captured_logger,logger_error:=editor_app.console_logger_install(&console_logger,context.logger)
    if logger_error!=.None {
        context.logger=previous_logger
        if terminal_logger.procedure!=nil { log.destroy_console_logger(terminal_logger) }
        return false
    }
    context.logger=captured_logger
    success:=run_editor_owned(renderer,api,config,compiler,fonts,down,&console_logger)
    _,restore_error:=editor_app.console_logger_destroy(&console_logger,captured_logger)
    context.logger=previous_logger
    if terminal_logger.procedure!=nil { log.destroy_console_logger(terminal_logger) }
    return success && restore_error==.None
}
run_editor_owned :: proc(renderer:^$R,api:Backend_API(R),config:Config,compiler:^shader.Compiler,fonts:^render.UI_Font_System,down:bool,console_logger:^editor_app.Console_Logger)->bool {
    if config.dump_graph || config.dump_graph_file!="" {
        if error:=api.capture_enable(renderer,true); error!=.None { fmt.eprintln("Cannot enable native graph capture:",error); return false }
    }
    defer { if config.dump_graph || config.dump_graph_file!="" { api.capture_enable(renderer,false) } }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    services_error:=app.authoring_services_init(&owner); if services_error!=.None { fmt.eprintln("Cannot initialize authoring services:",services_error); return false }
    resources_error:=app.asset_resources_init(&owner,config.project_root,config.resource_root); if resources_error!=.None { fmt.eprintln("Cannot initialize project resources:",resources_error); return false }
    if config.luau_library!="" && app.script_native_init(&owner,config.luau_library)!=.None { fmt.eprintln("Cannot initialize native Luau"); return false }
    if config.box_library!="" && app.physics_select_box3d(&owner,config.box_library)!=.None { fmt.eprintln("Cannot initialize native physics"); return false }
    audio_error:=app.audio_runtime_init(&owner); if audio_error!=.None { fmt.eprintln("Audio device unavailable:",audio_error) }
    scene_path:=config.scene_path
    if scene_path=="" {
        startup,_:=filepath.join({config.project_root,"assets/scenes/default.katla"}); defer delete(startup)
        if os.is_file(startup) { scene_path="assets/scenes/default.katla" } else { fmt.eprintln("No default scene at",startup,"— starting an empty document") }
    }
    if scene_path!="" {
        result,undo:=app.scene_file_execute(&owner,{action=.Load,path=scene_path,has_path=true}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&undo)
        if result.error!=.None { fmt.eprintln("Cannot load scene:",result.error); return false }
    }
    doc:document.State; if document.init(&doc,&owner)!=.None { return false }; defer document.destroy(&doc)
    store:prefs.Store; if prefs.store_init(&store,config.preferences_directory)!=.None { fmt.eprintln("Cannot open preferences directory"); return false }; defer prefs.store_destroy(&store)
    preference,preference_error:=prefs.load(&store); defer prefs.destroy(&preference); if preference_error!=.None { fmt.eprintln("Using default preferences:",preference_error) }
    browser:assets.State; assets.init(&browser,&owner); defer assets.destroy(&browser); assets.refresh(&browser)
    ctx:ui.Context; if ui.context_init(&ctx,render.ui_font_provider(fonts))!=.None { fmt.eprintln("Cannot initialize shaped-font UI"); return false }; defer ui.context_destroy(&ctx)
    state:editor_app.State; editor_app.state_init(&state,&owner); defer editor_app.state_destroy(&state)
    shell:editor_app.Shell; if editor_app.shell_init(&shell,&state,&ctx,&doc)!=.None { return false }; defer editor_app.shell_destroy(&shell)
    runtime:=ecs.get_resource_mut(&owner.world,app.Audio_Runtime)
    host_panel:editor_app.Host_Panel; editor_app.host_panel_init(&host_panel,nil,nil); defer editor_app.host_panel_destroy(&host_panel); editor_app.shell_services(&shell,&host_panel,&browser,&preference,&store,runtime.service)
    editor_app.shell_dock_load(&shell)
    defer editor_app.shell_dock_save(&shell)
    native:window.Window; defer window.window_destroy(&native)
    input:window.Native_Input; defer window.native_input_destroy(&input)
    extent:=window.State{width=1440,height=900,visible=true}
    if !config.headless {
        if window.window_create(&native,"Katla",1440,900)!=.None { return false }
        ui.clipboard_provider(&ctx,window.clipboard_provider(&native))
        if window.native_input_init(&input,&native)!=.None { return false }
        extent=window.window_state(&native)
        if api.attach(renderer,window.window_surface(&native))!=.None { return false }
    }
    defer { if !config.headless { api.detach(renderer) } }
    offscreen:gfx.Texture_Handle
    if config.headless { target,error:=api.gpu.create_texture(renderer,{width=extent.width,height=extent.height,depth=1,layers=1,mip_levels=1,format=.BGRA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}); if error!=.None { return false }; offscreen=target }
    defer { if offscreen.owner!=nil { api.gpu.destroy_texture(renderer,offscreen) } }
    if config.has_camera { shell.viewports.slots[0].camera.yaw=config.camera[0]*math.PI/180; shell.viewports.slots[0].camera.pitch=config.camera[1]*math.PI/180; shell.viewports.slots[0].camera.distance=config.camera[2] }
    surface,compile_error:=render.surface_shader_compile(compiler,.RGBA8_Unorm); if compile_error!=.None { fmt.eprintln("Scene shader:",compile_error); return false }; defer render.surface_shader_destroy(&surface)
    models,model_error:=render.model_shader_compile(compiler,.RGBA8_Unorm); if model_error!=.None { fmt.eprintln("Model shader:",model_error); return false }; defer render.model_shader_destroy(&models)
    ui_shader,ui_error:=render.ui_shader_compile(compiler,.BGRA8_Unorm); if ui_error!=.None { fmt.eprintln("UI shader:",ui_error); return false }; defer render.ui_shader_destroy(&ui_shader)
    model_config:=render.Model_Config(R){shader=&models,operations={gpu=api.gpu,create_sampler=api.gpu.create_sampler,destroy_sampler=api.gpu.destroy_sampler}}
    gpu:editor_app.GPU_Owner(R)
    init_error:=editor_app.gpu_owner_init(&gpu,&owner,renderer,api.gpu,render.surface_pipelines(&surface),&model_config,api.ui,&ui_shader,fonts,api.particles,api.picking,compiler,3)
    if init_error!={} { fmt.eprintln("Native scene preparation:",init_error); return false }; defer editor_app.gpu_owner_destroy(&gpu)
    thumbnails:render.Thumbnail_Cache(R)
    thumbnail_init_error:=render.thumbnail_cache_init(&thumbnails,&gpu.ui); if thumbnail_init_error!={} { fmt.eprintln("Thumbnail cache:",thumbnail_init_error);return false }
    defer render.thumbnail_cache_destroy(&thumbnails)
    previews:render.Material_Preview_Cache(R)
    preview_error:=render.material_preview_init(&previews,&gpu.ui,api.gpu);if preview_error!={} { fmt.eprintln("Material previews:",preview_error);return false }
    defer render.material_preview_destroy(&previews)
    shader_root:=config.shader_root
    if shader_root=="" { shader_root="odin/app/render/shaders" }
    shader_reload:editor_app.Editor_Shader_Reload(R)
    shader_reload_init_error:=editor_app.editor_shader_reload_init(&shader_reload,&gpu,&surface,&models,&ui_shader,compiler,shader_root); if shader_reload_init_error!=.None { fmt.eprintln("Shader source family:",shader_reload_init_error);return false }
    defer editor_app.editor_shader_reload_destroy(&shader_reload)
    shader_poll_serial:u64=0
    shader_start:=time.tick_now()
    for {
        shader_poll_serial+=1;status:=render.shader_reload_poll(&shader_reload.service,shader_poll_serial)
        if status.published>0 { break }
        if status.failed>0 || time.tick_diff(shader_start,time.tick_now())>30*time.Second { fmt.eprintln("Initial shader source family:",status.error);return false }
        time.sleep(time.Millisecond)
    }
    host_panel.capture_state=&gpu; host_panel.capture_request=editor_app.gpu_capture_callback(R)
    views:editor_app.View_Service
    view_init_error:=editor_app.view_service_init(&views,&shell,&gpu,editor_app.gpu_view_capture_callback(R),config.mcp_socket); if view_init_error!=.None { fmt.eprintln("Editor view endpoint:",view_init_error); return false }
    defer editor_app.view_service_destroy(&views)
    owner.before_mutation_state=&shell; owner.before_mutation=editor_app.shell_before_mutation
    views.mutation_state=&shell; views.prepare_mutation=editor_app.shell_before_mutation
    defer { owner.before_mutation=nil; owner.before_mutation_state=nil }
    if config.screenshot!="" { editor_app.gpu_capture_request(&gpu) }
    journey:=CLI_Journey{directory=config.interaction_test if config.interaction_test!="" else config.ui_test,checks=make([dynamic]CLI_Check),interaction=config.interaction_test!=""}
    defer delete(journey.checks)
    if journey.directory!="" && !os.is_dir(journey.directory) { error:=os.make_directory_all(journey.directory); if error!=nil { fmt.eprintln("CLI output directory:",error); return false } }
    texture_reload:render.Texture_Reload_Service
    origin:=time.tick_now(); previous:=origin; last_script_poll:=origin; frame_count:=0
    for !doc.quit_requested {
        pool:=window.frame_begin()
        now:=time.tick_now(); elapsed:=max(0,f32(time.duration_seconds(time.tick_diff(previous,now)))); delta:=min(elapsed,.1); previous=now
        actual:=extent; input_frame:=ui.Input{pixel_scale=1,time=time.duration_seconds(time.tick_diff(origin,now))}
        if !config.headless { actual,input_frame=window.native_input_poll(&input,input_frame.time) }
        if actual.closed { fmt.eprintln("Native window owner closed unexpectedly"); window.frame_end(pool); return false }
        if input.close_requested { input.close_requested=false; editor_app.shell_request_close(&shell) }
        if actual.width==0 || actual.height==0 || !actual.visible { window.frame_end(pool); time.sleep(10*time.Millisecond); continue }
        wait_error:=editor_app.gpu_owner_wait(&gpu); if wait_error!=.None { fmt.eprintln("Combined frame retirement:",gpu.serial,wait_error); window.frame_end(pool); return false }
        thumbnail_receipt:=editor_app.shell_asset_thumbnails_update(&shell,&thumbnails,f64(delta))
        if thumbnail_receipt.failed>0 { log.warn("Asset thumbnail retained previous image",thumbnail_receipt.error) }
        shader_poll_serial+=1
        shader_status:=render.shader_reload_poll(&shader_reload.service,shader_poll_serial)
        if shader_status.failed>0 { log.warn("Shader reload retained previous family",shader_status.error) }
        model_caches:[4]^render.Native_Model(R); model_count:=0
        for view in gpu.views { if view.active!=nil && view.active.models!=nil { model_caches[model_count]=view.active.models; model_count+=1 } }
        receipt,reload_error:=render.model_texture_reload_poll(&texture_reload,&owner,model_caches[:model_count])
        if reload_error!={} { log.warn("Texture hot reload retained previous GPU resources",reload_error) }
        if receipt.cleanup!=.None { fmt.eprintln("Texture hot reload published with cleanup failure:",receipt.cleanup) }
        editor_app.selection_refresh(&state)
        shell.material_preview_has_entity=false
        if state.selection.has_primary {
            models:^render.Native_Model(R);if gpu.views[0].active!=nil { models=gpu.views[0].active.models }
            preview:=render.material_preview_update(&previews,models,&owner,state.selection.primary)
            shell.material_preview_has_entity=preview.ready;shell.material_preview_entity=preview.entity;shell.material_preview_textures=preview.textures
            if preview.error!={} { log.warn("Material preview retained previous images",preview.error) }
        }
        if !config.headless && (actual.width!=extent.width || actual.height!=extent.height) { if api.resize(renderer,actual.width,actual.height)!=.None { window.frame_end(pool); return false }; extent=actual }
        completed,capture_error:=editor_app.gpu_capture_poll(&gpu)
        if capture_error!=.None { editor_app.host_panel_capture_failed(&host_panel); editor_app.view_service_capture_failed(&views); fmt.eprintln("Viewport capture:",capture_error) }
        if completed {
            editor_app.gpu_selection_complete(&gpu,&shell)
            png,valid_png:=editor_app.capture_png(&gpu.snapshot); defer delete(png)
            if !valid_png { editor_app.host_panel_capture_failed(&host_panel); editor_app.view_service_capture_failed(&views) }
            else {
                editor_app.view_service_capture(&views,&gpu.snapshot,gpu.capture_context,png)
                if config.screenshot!="" { error:=os.write_entire_file(config.screenshot,png); if error!=nil { fmt.eprintln("Screenshot:",error); return false } }
                if host_panel.capturing {
                    committed,valid_metadata:=editor_app.view_committed_metadata(&gpu.snapshot,gpu.capture_context); defer delete(committed)
                    if valid_metadata { encoded,encode_error:=base64.encode(png); if encode_error==nil { editor_app.host_panel_committed(&host_panel,string(committed),string(encoded)); delete(encoded) } else { editor_app.host_panel_capture_failed(&host_panel) } }
                    else { editor_app.host_panel_capture_failed(&host_panel) }
                }
            }
        }
        _,socket_error:=editor_app.view_service_tick(&views,time.tick_diff(origin,now)); if socket_error!=.None { fmt.eprintln("Editor view transport:",socket_error) }
        editor_app.host_panel_poll(&host_panel)
        if config.luau_library!="" && owner.mode==.Editing && time.tick_diff(last_script_poll,now)>=time.Second { last_script_poll=now; error:=app.script_native_sync(&owner); if error!=.None { editor_app.shell_runtime_diagnostic(&shell,"Script hot reload",error) } }
        editor_app.shell_script_input(&shell,&input.input)
        if owner.mode==.Playing { error:=app.simulation_step(&owner,delta); if error!=.None { fmt.eprintln("Simulation:",error) } }
        app.animation_editor_step(&owner,delta)
        if owner.mode==.Editing { error:=app.animation_events_dispatch(&owner); if error!=.None { editor_app.shell_runtime_diagnostic(&shell,"Animation events",error) } }
        app.audio_runtime_update(&owner,delta)
        editor_app.shell_script_poll(&shell,time.duration_seconds(time.tick_diff(origin,now)))
        editor_app.shell_console_poll(&shell)
        editor_app.console_logger_drain(console_logger,&shell.console)
        editor_app.gpu_owner_particle_statistics(&gpu,&shell)
        if preference.show_physics_debug { error:=app.physics_prepare(&owner); if error!=.None { fmt.eprintln("Physics debug admission:",error) } }
        editor_app.shell_camera_tick(&shell,delta)
        if shell.particle_reset_requested {
            particle_reset_error:=render.particle_reset_all(&gpu.particles); if particle_reset_error!={} { log.warn("Particle reset retained for retry",particle_reset_error) }
            else { shell.particle_reset_requested=false }
        }
        refresh_error:=editor_app.gpu_owner_refresh(&gpu); if refresh_error!={} { fmt.eprintln("Editor asset revision preparation:",refresh_error);window.frame_end(pool);return false }
        size:=ui.Vec2{f32(actual.width)/input_frame.pixel_scale,f32(actual.height)/input_frame.pixel_scale}
        events:[dynamic]ui.Input_Event
        if journey.directory!="" { events=cli_input(&journey,&shell,frame_count,config.interaction_test!=""); input_frame.events=events[:] }
        descriptor:=editor_app.shell_build(&shell,size)
        draw_list,result:=ui.frame(&ctx,descriptor,input_frame,size)
        if !config.headless { window.native_input_ime(&input,result.ime) }
        editor_app.shell_viewport_input(&shell,input_frame,result)
        delete(events)
        if result.error!=.None { fmt.eprintln("UI frame:",frame_count,result.error); diagnostic_duplicates(descriptor); window.frame_end(pool); return false }
        editor_app.shell_actions(&shell)
        mesh,mesh_error:=render.ui_prepare(fonts,draw_list)
        if mesh_error!=.None { fmt.eprintln("UI mesh:",mesh_error); window.frame_end(pool); return false }
        editor_app.gpu_selection_request(&gpu,&shell)
        token,acquire_error:=api.gpu.acquire(renderer)
        if acquire_error!=.None { fmt.eprintln("Frame acquisition:",gpu.serial,acquire_error); render.ui_mesh_destroy(&mesh); window.frame_end(pool); return false }
        drawable:=gfx.Surface_Frame{width=extent.width,height=extent.height,texture=offscreen}
        surface_result:=gfx.Surface_Result.Presented; surface_error:=gfx.Gpu_Error.None
        if !config.headless { drawable,surface_result,surface_error=api.acquire_surface(renderer) }
        if surface_error!=.None || surface_result!=.Presented { api.gpu.abort(renderer,token); render.ui_mesh_destroy(&mesh); window.frame_end(pool); time.sleep(time.Millisecond); continue }
        particle_delta_error:=render.particle_frame_delta(&gpu.particles,0 if owner.mode==.Paused else delta); if particle_delta_error!={} { fmt.eprintln("Particle interval:",particle_delta_error); if !config.headless { api.abort_surface(renderer,drawable) }; api.gpu.abort(renderer,token); render.ui_mesh_destroy(&mesh); window.frame_end(pool); return false }
        submission,render_error:=editor_app.gpu_owner_frame(&gpu,&shell,fonts,&mesh,token,drawable,down,config.headless || config.check_black_frames)
        render.ui_mesh_destroy(&mesh)
        if render_error!={} { if !config.headless { api.abort_surface(renderer,drawable) }; api.gpu.abort(renderer,token); fmt.eprintln("Combined editor frame:",render_error); window.frame_end(pool); return false }
        if !config.headless { outcome,present_error:=api.present(renderer,drawable,submission); if present_error!=.None || outcome.surface==.Fatal { fmt.eprintln("Presentation:",present_error,outcome.surface); window.frame_end(pool); return false } }
        frame_count+=1
        editor_app.shell_frame_statistics(&shell,max(.000001,elapsed),len(gpu.plan.order),gpu.serial)
        window.frame_end(pool)
        if frame_count==1 && (config.dump_layout || config.dump_layout_file!="") { if !diagnostic_layout(&shell,config.dump_layout_file) { return false } }
        if frame_count==1 && (config.dump_graph || config.dump_graph_file!="") { if !diagnostic_graph(&gpu,api.capture_snapshot,config.dump_graph_file) { return false }; if api.capture_enable(renderer,false)!=.None { return false } }
        if config.check_black_frames { if !diagnostic_image(&gpu,extent.width,extent.height,"",true) { return false } }
        if journey.directory!="" && !cli_after_frame(&journey,&shell,&gpu,frame_count,extent.width,extent.height,config.interaction_test!="") { return false }
        if config.frames>0 && frame_count>=config.frames { break }
        time.sleep(time.Millisecond)
    }
    if gpu.capture.submission.owner!=nil {
        deadline:=time.tick_now()
        for time.tick_since(deadline)<5*time.Second {
            completed,capture_error:=editor_app.gpu_capture_poll(&gpu); if capture_error!=.None { return false }; if completed { png,valid:=editor_app.capture_png(&gpu.snapshot); if !valid { return false }; defer delete(png); if config.screenshot!="" { if os.write_entire_file(config.screenshot,png)!=nil { return false } }; break }; time.sleep(time.Millisecond)
        }
    }
    if gpu.capture.submission.owner!=nil { fmt.eprintln("Viewport screenshot capture timed out"); return false }
    if journey.directory!="" && !cli_summary(&journey) { return false }
    fmt.printf("Canonical editor accepted %d combined %s frames\n",frame_count,config.backend)
    return true
}
