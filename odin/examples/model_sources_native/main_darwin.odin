#+build darwin, arm64
//! Canonical imported children exercise native admission, independent factors and controller skin playback.
package main

import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import shader "../../gfx/shader"
import scene "../../agent/scene"
import km "../../math"
import resources "../../resources"
import NS "core:sys/darwin/Foundation"
import "core:fmt"
import "core:mem"
import "core:os"
import "core:strings"
import "core:time"

Capture_Ops :: struct($R:typeid) {
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
pixels :: proc(consumer:^render.Native_Consumer($R),capture:Capture_Ops(R),frame:render.Frame_Data)->gfx.Readback_Data {
    assert(render.native_consumer_refresh(consumer)=={})
    native:=consumer.active
    token,acquire_error:=render.native_scene_acquire(native);assert(acquire_error==.None)
    submission,error:=render.native_scene_render(native,token,frame,consumer.batch.objects,consumer.batch.draws);assert(error=={})
    source,source_error:=capture.source(consumer.renderer,submission,native.graph.output);assert(source_error==.None)
    ticket,queue_error:=capture.queue(consumer.renderer,source,{width=source.desc.width,height=source.desc.height,aspect=.Color,depth=1});assert(queue_error==.None)
    start:=time.tick_now()
    for {
        result,ready,poll_error:=capture.poll(consumer.renderer,ticket);assert(poll_error==.None)
        if ready { assert(render.native_scene_wait(native,submission)==.None);return result }
        assert(time.tick_since(start)<10*time.Second);time.sleep(time.Millisecond)
    }
}
changed_region :: proc(a,b:^gfx.Readback_Data,start,end:u32)->int {
    assert(a.region==b.region && a.row_pitch==b.row_pitch)
    changed:int
    for y in 0..<a.region.height { for x in start..<end {
        offset:=int(u64(y)*a.row_pitch+u64(x)*4)
        if mem.compare(a.bytes[offset:offset+4],b.bytes[offset:offset+4])!=0 { changed+=1 }
    } }
    return changed
}
reject_sampler :: proc(renderer:^$R,descriptor:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) { return {},.Allocation_Failed }
exercise :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),surface:render.Scene_Pipelines,model:^render.Model_Shader,backend:string) {
    directory,directory_error:=os.make_directory_temp("","katla-model-sources-native-*",context.allocator);assert(directory_error==nil)
    defer { os.remove_all(directory);delete(directory) }
    resource_directory:=strings.concatenate({directory,"/resources"});defer delete(resource_directory);assert(os.make_directory(resource_directory)==nil)
    filename:=strings.concatenate({resource_directory,"/independent.gltf"});defer delete(filename)
    assert(os.write_entire_file(filename,#load("independent.gltf",[]byte))==nil)
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None && app.asset_resources_init(&owner,directory,resource_directory)==resources.Error.None)
    config:=render.Model_Config(R){model,render.Model_GPU_Ops(R){operations,operations.create_sampler,operations.destroy_sampler}}
    consumer:render.Native_Consumer(R)
    assert(render.native_consumer_init(&consumer,&owner,renderer,operations,surface,3,256,256,models=&config)=={})
    defer assert(render.native_consumer_destroy(&consumer)==.None)
    consumer.active.feature_settings.sky=false;consumer.active.feature_settings.grid=false
    consumer.active.feature_settings.shadows=false;consumer.active.feature_settings.outline=false
    consumer.active.feature_settings.postprocess={1,.Linear}
    operation:=editor.Scene_Op{kind=.Spawn_Model,path="independent.gltf",scale={1,1,1}}
    before_owner:=consumer.active
    consumer.model_config.operations.create_sampler=reject_sampler
    rejected,rejected_history:=app.scene_action_execute(&owner,operation)
    assert(rejected.error!=.None && owner.world.live_count==0 && consumer.active==before_owner && consumer.last_error.gpu==.Allocation_Failed)
    editor.tool_result_destroy(&rejected);editor.undo_group_destroy(&rejected_history)
    consumer.model_config.operations.create_sampler=operations.create_sampler
    spawned,spawned_history:=app.scene_action_execute(&owner,operation);defer editor.tool_result_destroy(&spawned);defer editor.undo_group_destroy(&spawned_history)
    assert(spawned.error==.None && len(spawned.entities)==3 && owner.world.live_count==3 && consumer.active!=before_owner)
    root,left,right:=spawned.entities[0],spawned.entities[1],spawned.entities[2]
    controller:=ecs.get_component_mut(&owner.world,root,app.Scene_Model)
    first:=ecs.get_component_mut(&owner.world,left,app.Scene_Model);second:=ecs.get_component_mut(&owner.world,right,app.Scene_Model)
    assert(controller.source.kind==.Group && first.source.kind==.Primitive && second.source.kind==.Primitive && first.revision==second.revision && first.revision==controller.revision)
    assert(first.source.primitive_index==0 && second.source.primitive_index==1 && len(consumer.active.models.batch.entries)==2)
    for entry in consumer.active.models.batch.entries { assert(entry.entity!=root && (entry.entity==left || entry.entity==right)) }
    for id in ([2]ecs.Entity_Id{left,right}) { for role in 0..<5 {
        fallback,known:=render.model_native_image_fallback(consumer.active.models,id,role);assert(fallback && known)
    } }
    camera:=render.camera_default();camera.position={0,0,2.5};camera.target={}
    frame,frame_error:=render.frame_data(camera,256,256,backend=="vulkan");assert(frame_error==.None)
    baseline:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&baseline)
    arguments:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"base_color":[0.05,0.05,0.8,1],"metallic":0.7,"roughness":0.25}}`,u64(left));defer delete(arguments)
    response:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Application,tool_name="material",value=transmute([]byte)arguments},app.authoring_executor(&owner))
    if response.result.error!=.None { fmt.println("Canonical material edit failed",backend,response.result.error,arguments,consumer.last_error) }
    assert(response.result.error==.None)
    edited:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&edited)
    assert(changed_region(&baseline,&edited,0,128)>100 && changed_region(&baseline,&edited,128,256)==0,"independent primitive edit changed sibling native pixels")
    sibling:=ecs.get_component_mut(&owner.world,right,app.Surface_Material);assert(sibling.linear_color==km.Color{.05,.8,.05,1} && sibling.metallic==0 && sibling.roughness==1)
    assert(app.authoring_undo_last(&owner)==.None)
    undone:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&undone);assert(mem.compare(baseline.bytes,undone.bytes)==0)
    assert(app.authoring_redo_last(&owner)==.None)
    redone:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&redone);assert(mem.compare(edited.bytes,redone.bytes)==0)
    assert(app.authoring_undo_last(&owner)==.None)
    result,history:=app.animation_execute(&owner,scene.Animation_Op{action=.Play,entity=root,clip="move-both",speed=1,looping=true});assert(result.error==.None)
    editor.tool_result_destroy(&result);editor.undo_group_destroy(&history)
    result,history=app.animation_execute(&owner,scene.Animation_Op{action=.Seek,entity=root,time_seconds=.5});assert(result.error==.None)
    editor.tool_result_destroy(&result);editor.undo_group_destroy(&history)
    player:=ecs.get_component_mut(&owner.world,root,app.Animation_Player)
    assert(app.scene_model_animation_player(&owner,left)==player && app.scene_model_animation_player(&owner,right)==player)
    stable_owner:=consumer.active;stable_pipeline:=consumer.active.models.pipelines[0]
    animated:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&animated)
    assert(changed_region(&baseline,&animated,0,128)>100 && changed_region(&baseline,&animated,128,256)>100,"controller animation did not deform both native children")
    assert(consumer.active==stable_owner && consumer.active.models.pipelines[0]==stable_pipeline && consumer.active.models.batch.geometry_changed)
    repeated:=pixels(&consumer,capture,frame);defer gfx.readback_data_destroy(&repeated)
    assert(mem.compare(animated.bytes,repeated.bytes)==0 && !consumer.active.models.batch.geometry_changed)
    fmt.println("Canonical Spawn_Model PASS",backend,"two independent selected children, rollback, exact undo/redo, shared controller skin animation without pipeline replacement")
}
main :: proc() {
    assert(len(os.args)==3)
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0);mem.tracking_allocator_destroy(&tracker) }
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer assert(shader.compiler_destroy(&compiler)==.None)
    surface,surface_error:=render.surface_shader_compile(&compiler);assert(surface_error==.None);defer render.surface_shader_destroy(&surface)
    model,model_error:=render.model_shader_compile(&compiler);assert(model_error==.None);defer render.model_shader_destroy(&model)
    {
        renderer:metal.Renderer;assert(metal.renderer_init(&renderer)==.None);defer assert(metal.renderer_destroy(&renderer)==.None)
        operations:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
        exercise(&renderer,operations,Capture_Ops(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},render.surface_pipelines(&surface),&model,"metal")
    }
    {
        renderer:vulkan.Renderer;assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None)
        defer { assert(renderer.validation_errors==0);assert(vulkan.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
        exercise(&renderer,operations,Capture_Ops(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},render.surface_pipelines(&surface),&model,"vulkan")
    }
}
