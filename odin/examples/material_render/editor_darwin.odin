#+build darwin, arm64
//! Persistent native controls edit the same scene components used by the agent and renderer.
package main

import app "../../app"
import window "../../app/window"
import render "../../app/render"
import agent "../../agent"
import editor "../../editor"
import ecs "../../ecs"
import km "../../math"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import "core:fmt"
import "core:time"
import "core:mem"
import "core:os"
import "core:strings"
import llm "../../agent/llm"
import shader "../../gfx/shader"
import resources "../../resources"
import "core:path/filepath"

Editor_Window :: struct($R:typeid) {
    native:^render.Native_Scene(R),
    consumer:^render.Native_Consumer(R),
    particles:^render.Particle_Consumer(R),
    authoring:^app.Authoring,
    viewport:^window.Window,
    controls:^window.Controls,
    assistant:^app.Assistant,
    assistant_panel:^window.Assistant_Panel,
    surface:Surface_Ops(R),
    capture:Capture_Ops(R),
    ids:[2]ecs.Entity_Id,
    camera:render.Camera,
    gesture:app.Material_Gesture,
    backend,output:string,
    revision:u64,
    assistant_feedback:string,
    baseline,last_edit:gfx.Readback_Data,
    failed:bool,
    selection_available:bool,
    last_frame:time.Tick,
}
editor_system_prompt :: proc(selected:ecs.Entity_Id,available:bool)->string {
    if !available { return fmt.aprintf("No drawable entity is selected. Query the current scene before editing. Scene loads replace generational entity IDs. Use supported scene and asset tools; base colors use sRGB.") }
    return fmt.aprintf("Selected entity_id=%d. Selected authored SceneKey=1. Scene loads replace generational entity IDs; query_entities after a load before editing. Use the material and asset tools for the scene. All base colors use sRGB. Explain accepted changes briefly.",u64(selected))
}
editor_selection :: proc(ui:^Editor_Window($R)) {
    ids:=ecs.entity_ids(&ui.authoring.world); defer delete(ids)
    selected:ecs.Entity_Id; found:=false
    for id in ids {
        if _,hidden:=ecs.get_component(&ui.authoring.world,id,app.Editor_Hidden); hidden { continue }
        if _,surface:=ecs.get_component(&ui.authoring.world,id,app.Surface_Material); !surface { continue }
        mesh,has_mesh:=ecs.get_component(&ui.authoring.world,id,app.Scene_Mesh)
        _,has_model:=ecs.get_component(&ui.authoring.world,id,app.Scene_Model)
        if !has_model && (!has_mesh || mesh.source.kind==.Empty) { continue }
        if !found { selected=id; found=true }
        if key,has_key:=ecs.get_component(&ui.authoring.world,id,app.Scene_Key); has_key && key.value==1 { selected=id; found=true; break }
    }
    changed:=ui.ids[0]!=selected || ui.selection_available!=found
    ui.ids[0]=selected; ui.selection_available=found
    if changed && ui.assistant!=nil && ui.assistant.initialized { prompt:=editor_system_prompt(selected,found); defer delete(prompt); assert(app.assistant_context_refresh(ui.assistant,prompt)==.None); editor_assistant_refresh(ui) }
}
editor_command_count :: proc(session:^editor.Agent_Session)->int {
    count:int; for action in session.actions { if action.undo.state!=nil { count+=1 } }; return count
}
editor_controls_refresh :: proc(ui:^Editor_Window($R),status:string) {
    editor_selection(ui)
    window.controls_selection(ui.controls,ui.selection_available)
    if !ui.selection_available { window.controls_set(ui.controls,{},editor.agent_can_undo(&ui.authoring.agent.session),editor.agent_can_redo(&ui.authoring.agent.session),"No mesh selected"); return }
    surface,exists:=ecs.get_component(&ui.authoring.world,ui.ids[0],app.Surface_Material)
    if !exists { window.controls_set(ui.controls,{},editor.agent_can_undo(&ui.authoring.agent.session),editor.agent_can_redo(&ui.authoring.agent.session),"No mesh selected"); return }
    value:=app.material_values(surface)
    window.controls_set(ui.controls,{value.base_color[0],value.base_color[1],value.base_color[2],value.base_color[3],value.metallic,value.roughness,value.ao},editor.agent_can_undo(&ui.authoring.agent.session),editor.agent_can_redo(&ui.authoring.agent.session),status)
}
editor_window_render :: proc(ui:^Editor_Window($R),capture_pixels:=false)->gfx.Readback_Data {
    state:=window.window_state(ui.viewport)
    if !state.visible || state.closed || state.width==0 || state.height==0 { return {} }
    assert(render.native_consumer_refresh(ui.consumer)=={})
    if ui.native!=ui.consumer.active { gfx.readback_data_destroy(&ui.baseline); gfx.readback_data_destroy(&ui.last_edit) }
    ui.native=ui.consumer.active
    if state.width!=ui.native.graph.color_desc.width || state.height!=ui.native.graph.color_desc.height {
        assert(render.native_consumer_resize(ui.consumer,state.width,state.height)=={})
        assert(ui.surface.resize(ui.native.renderer,state.width,state.height)==.None)
    }
    frame,error:=render.frame_data(ui.camera,state.width,state.height,ui.backend=="vulkan"); assert(error==.None)
    delta:=f32(time.duration_seconds(time.tick_since(ui.last_frame))); ui.last_frame=time.tick_now()
    assert(render.particle_frame_delta(ui.particles,min(delta,1))=={})
    token,acquire_error:=render.native_scene_acquire(ui.native); assert(acquire_error==.None)
    target,result,surface_error:=ui.surface.acquire(ui.native.renderer); assert(surface_error==.None && result==.Presented)
    submission,render_error:=render.native_scene_render(ui.native,token,frame,ui.consumer.batch.objects,ui.consumer.batch.draws,target.texture)
    if render_error!={} { ui.surface.abort(ui.native.renderer,target); ui.native.operations.abort(ui.native.renderer,token); fmt.println(render_error); assert(false) }
    source:gfx.Texture_Source; ticket:gfx.Readback_Ticket
    if capture_pixels {
        source,surface_error=ui.capture.source(ui.native.renderer,submission,ui.native.graph.output); assert(surface_error==.None)
        ticket,surface_error=ui.capture.queue(ui.native.renderer,source,{width=source.desc.width,height=source.desc.height,aspect=.Color,depth=1}); assert(surface_error==.None)
    }
    outcome,present_error:=ui.surface.present(ui.native.renderer,target,submission)
    assert(present_error==.None && outcome.submission==submission && outcome.surface==.Presented)
    assert(render.native_scene_wait(ui.native,submission)==.None)
    if capture_pixels { return poll_pixels(ui.native.renderer,ui.capture,ticket,source) }
    return {}
}
editor_control_event :: proc(ui:^Editor_Window($R),event:window.Control_Event) {
    editor_selection(ui)
    if !ui.selection_available && event.field!=.Undo && event.field!=.Redo { editor_controls_refresh(ui,"No mesh selected"); return }
    error:=editor.Scene_Error.None
    before_count:=len(ui.authoring.agent.session.actions)
    switch event.phase {
    case .Preview:
        if !ui.gesture.active { error=app.material_gesture_begin(ui.authoring,&ui.gesture,ui.ids[:1]) }
        if error==.None {
            surface,exists:=ecs.get_component(&ui.authoring.world,ui.ids[0],app.Surface_Material); assert(exists)
            values:=app.material_values(surface); fields:bit_set[agent.Material_Field]
            switch event.field {
            case .Red,.Green,.Blue,.Alpha: values.base_color[int(event.field)]=event.value; fields={.Base_Color}
            case .Metallic: values.metallic=event.value; fields={.Metallic}
            case .Roughness: values.roughness=event.value; fields={.Roughness}
            case .Occlusion: values.ao=event.value; fields={.AO}
            case .Undo,.Redo,.Preset: error=.Invalid_Operation
            }
            if error==.None { error=app.material_gesture_preview(ui.authoring,&ui.gesture,fields,values) }
        }
        assert(len(ui.authoring.agent.session.actions)==before_count,"pointer preview incorrectly recorded a history step")
    case .Finish:
        if ui.gesture.active { changed:=ui.gesture.changed; error=app.material_gesture_finish(ui.authoring,&ui.gesture); if error==.None { assert(len(ui.authoring.agent.session.actions)==before_count+int(changed)) } }
    case .Cancel:
        if ui.gesture.active { error=app.material_gesture_cancel(ui.authoring,&ui.gesture) }
    case .Activate:
        if ui.gesture.active { error=app.material_gesture_finish(ui.authoring,&ui.gesture) }
        if error==.None {
            if event.field==.Undo { error=app.authoring_undo_last(ui.authoring) }
            if event.field==.Redo { error=app.authoring_redo_last(ui.authoring) }
            if event.field==.Preset {
                preset:=agent.Material_Preset(int(event.value))
                arguments:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"preset":"%s"}}`,u64(ui.ids[0]),agent.material_preset_name(preset)); defer delete(arguments)
                action:=editor.agent_execute(&ui.authoring.agent.session,&ui.authoring.world,&ui.authoring.registry,{kind=.Application,tool_name="material",value=transmute([]byte)arguments},app.authoring_executor(ui.authoring))
                error=action.result.error
            }
        }
    }
    if error!=.None { editor_controls_refresh(ui,"Edit rejected; scene preserved"); fmt.println("Native control rejected:",error); return }
    ui.revision+=1
    pixels:=editor_window_render(ui,event.phase!=.Preview)
    if len(pixels.bytes)>0 {
        if event.phase==.Activate && event.field==.Undo && !editor.agent_can_undo(&ui.authoring.agent.session) && len(ui.baseline.bytes)>0 && pixels.region==ui.baseline.region {
            assert(mem.compare(pixels.bytes,ui.baseline.bytes)==0,"actual native Undo click did not restore baseline pixels")
        }
        if event.phase==.Activate && event.field==.Redo && len(ui.last_edit.bytes)>0 && pixels.region==ui.last_edit.region {
            assert(mem.compare(pixels.bytes,ui.last_edit.bytes)==0,"actual native Redo click did not restore last edited pixels")
        }
        save_pixels(&pixels,ui.output)
        if event.phase==.Finish || event.field==.Preset { gfx.readback_data_destroy(&ui.last_edit); ui.last_edit=pixels } else { gfx.readback_data_destroy(&pixels) }
    }
    text:=fmt.aprintf("%d edits · %d redo",editor_command_count(&ui.authoring.agent.session),len(ui.authoring.agent.session.redo_actions)); defer delete(text)
    editor_controls_refresh(ui,text)
    if event.phase!=.Preview { fmt.println("Native control accepted:",event.field,event.phase,"history",len(ui.authoring.agent.session.actions),"redo",len(ui.authoring.agent.session.redo_actions)) }
}
editor_assistant_refresh :: proc(ui:^Editor_Window($R)) {
    service:=ui.assistant
    model:=service.config.model
    if service.state==.Disabled { model="Assistant disabled" } else if service.config.provider==.Disabled { model="Assistant unavailable" }
    can_send:=service.state==.Idle || service.state==.Completed
    can_cancel:=service.state==.Running
    can_reset:=service.initialized && service.config.provider!=.Disabled && service.state!=.Running && service.state!=.Cancelling
    status:=ui.assistant_feedback; if status=="" { status=app.assistant_status(service) }
    window.assistant_panel_set(ui.assistant_panel,model,status,string(service.output[:]),can_send,can_cancel,can_reset)
}
editor_assistant_event :: proc(ui:^Editor_Window($R),action:window.Assistant_Action,prompt:string) {
    ui.assistant_feedback=""
    error:=llm.Error.None
    switch action {
    case .Send:
        if strings.trim_space(prompt)=="" { ui.assistant_feedback="Enter a request before sending." }
        else if len(prompt)>llm.MAX_TEXT_BYTES { ui.assistant_feedback="Shorten the request to less than 1 MiB." }
        else { error=app.assistant_start(ui.assistant,prompt) }
    case .Cancel: app.assistant_cancel(ui.assistant)
    case .New_Conversation: error=app.assistant_reset(ui.assistant)
    }
    if error!=.None { ui.assistant_feedback="Request unavailable. Check the assistant status before retrying." }
    editor_assistant_refresh(ui)
}
metal_assistant_event :: proc(data:rawptr,action:window.Assistant_Action,prompt:string) { editor_assistant_event(cast(^Editor_Window(metal.Renderer))data,action,prompt) }
vulkan_assistant_event :: proc(data:rawptr,action:window.Assistant_Action,prompt:string) { editor_assistant_event(cast(^Editor_Window(vulkan.Renderer))data,action,prompt) }
metal_control_event :: proc(data:rawptr,event:window.Control_Event) { editor_control_event(cast(^Editor_Window(metal.Renderer))data,event) }
vulkan_control_event :: proc(data:rawptr,event:window.Control_Event) { editor_control_event(cast(^Editor_Window(vulkan.Renderer))data,event) }

exercise_editor :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),surface:Surface_Ops(R),descriptor:render.Scene_Pipelines,compiler:^shader.Compiler,model_ops:render.Model_GPU_Ops(R),particle_ops:render.Particle_GPU_Ops(R),backend,output,resource_path:string,callback:proc(rawptr,window.Control_Event),assistant_callback:proc(rawptr,window.Assistant_Action,string)) {
    native_window:window.Window; assert(window.window_create(&native_window,"Katla · Material scene",680,440)==.None); defer window.window_destroy(&native_window)
    initial:=window.window_poll(&native_window)
    authoring:app.Authoring; app.authoring_init(&authoring); defer app.authoring_destroy(&authoring); assert(app.authoring_services_init(&authoring)==.None)
    assert(app.asset_resources_init(&authoring,filepath.dir(resource_path),resource_path)==resources.Error.None)
    source:=transmute([]byte)string(`{"kind":"sphere","radius":0.5,"segments":48,"rings":24}`)
    meshes:[2]app.Scene_Mesh
    for &mesh in meshes { error:app.Mesh_Error; mesh,error=app.scene_mesh_prepare(&authoring,{kind=.Geometry,geometry=source}); assert(error==.None) }
    ids:=[2]ecs.Entity_Id{
        ecs.spawn(&authoring.world,struct { mesh:app.Scene_Mesh,transform:app.Scene_Transform, surface:app.Surface_Material,key:app.Scene_Key }{meshes[0],{km.transform(position={-0.65,0,0},scale={1.4,1.4,1.4})},{km.color_to_linear({0.85,0.12,0.08,1}),true,0,0.7,1},{1}}),
        ecs.spawn(&authoring.world,struct { mesh:app.Scene_Mesh,transform:app.Scene_Transform, surface:app.Surface_Material,key:app.Scene_Key }{meshes[1],{km.transform(position={0.65,0,0},scale={1.4,1.4,1.4})},{km.color_to_linear({0.08,0.2,0.85,1}),true,0,0.4,1},{2}}),
    }
    ecs.get_resource_mut(&authoring.world,app.Scene_Identity).next_entity_id=3
    model_shader,model_error:=render.model_shader_compile(compiler,descriptor.output_format); assert(model_error==.None); defer render.model_shader_destroy(&model_shader)
    models:=render.Model_Config(R){&model_shader,model_ops}
    consumer:render.Native_Consumer(R); assert(render.native_consumer_init(&consumer,&authoring,renderer,operations,descriptor,3,initial.width,initial.height,models=&models)=={})
    particles:render.Particle_Consumer(R)
    particle_error,particle_shader_error:=render.particle_consumer_init(&particles,&authoring,renderer,particle_ops,compiler,descriptor.surface.colors[0].format,4096,64,3)
    assert(particle_error=={} && particle_shader_error==.None); defer { assert(render.native_consumer_destroy(&consumer)==.None); assert(render.particle_consumer_destroy(&particles)==.None) }
    assert(render.native_consumer_compose(&consumer,render.particle_composition(&particles))=={})
    assert(surface.attach(renderer,{view=window.window_view(&native_window),width=initial.width,height=initial.height})==.None); defer { assert(surface.detach(renderer)==.None) }
    controls:window.Controls
    ui:=Editor_Window(R){native=consumer.active,consumer=&consumer,particles=&particles,authoring=&authoring,viewport=&native_window,controls=&controls,surface=surface,capture=capture,ids=ids,camera=render.camera_default(),backend=backend,output=output,last_frame=time.tick_now()}
    ui.camera.position={0,0,3.4}
    assert(window.controls_create(&controls,&ui,callback)==.None); defer window.controls_destroy(&controls)
    window.window_place_beside(&controls.window,&native_window)
    service:app.Assistant; panel:window.Assistant_Panel
    ui.assistant=&service; ui.assistant_panel=&panel
    schemas,schema_error:=agent.tools_select({"material","query_entities","get_component_attributes","search_assets","list_resources","read_resource","save_scene","load_scene","prefab"}); assert(schema_error==.None); defer delete(schemas)
    system_prompt:=editor_system_prompt(ids[0],true); defer delete(system_prompt)
    config_path:=os.get_env("KATLA_ODIN_AGENT_CONFIG",context.allocator); defer delete(config_path)
    app.assistant_init_path(&service,&authoring.agent,config_path,schemas,system_prompt)
    defer app.assistant_destroy(&service)
    assert(window.assistant_panel_create(&panel,&ui,assistant_callback)==.None); defer window.assistant_panel_destroy(&panel)
    window.window_place_beside(&panel.window,&native_window)
    editor_assistant_refresh(&ui)
    defer app.material_gesture_destroy(&ui.gesture)
    defer gfx.readback_data_destroy(&ui.baseline); defer gfx.readback_data_destroy(&ui.last_edit)
    editor_controls_refresh(&ui,"One undo step per drag")
    assert(ui.selection_available && ui.ids[0]==ids[0],"the valid first ECS entity must remain selected")
    ui.baseline=editor_window_render(&ui,true); save_pixels(&ui.baseline,output)
    fmt.println("Native panel frame:",controls.window.native->frame(),"screen:",controls.window.native->screen()->frame())
    fmt.println("Native editor ready:",backend,"drag material sliders, Undo/Redo, then close scene window")
    for {
        inspector:=window.controls_poll(&controls)
        state:=window.window_state(&native_window)
        if state.closed || inspector.closed { break }
        if ui.failed { break }
        accepted:=app.authoring_tick(&authoring)
        if app.assistant_poll(&service) { ui.assistant_feedback=""; editor_assistant_refresh(&ui) }
        if accepted>0 {
            pixels:=editor_window_render(&ui,true)
            gfx.readback_data_destroy(&ui.last_edit); ui.last_edit=pixels; save_pixels(&ui.last_edit,output)
            editor_controls_refresh(&ui,"Assistant operation completed")
            fmt.println("Native assistant executed",accepted,"owner-thread calls; history",len(authoring.agent.session.actions))
        } else { editor_window_render(&ui) }
        time.sleep(16*time.Millisecond)
    }
    if ui.gesture.active { assert(app.material_gesture_cancel(&authoring,&ui.gesture)==.None) }
    assert(!ui.failed,"native editor rejected a user action")
    fmt.println("Native editor closed after",ui.revision,"accepted control events; history",len(authoring.agent.session.actions))
}
