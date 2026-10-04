#+build darwin,arm64
//! Real model pixels and exact native export ownership survive transactional resize rejection.
package main
import app "../../app"
import render "../../app/render"
import gfx "../../gfx"
import ecs "../../ecs"
import km "../../math"
import shader "../../gfx/shader"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:path/filepath"

texture_budget:int=-1
reject_exports:bool
track_candidates:bool
candidate_handles:[dynamic]gfx.Texture_Handle
fault_texture :: proc($R:typeid)->proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    return proc(renderer:^R,descriptor:gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
        if texture_budget==0 { return {},.Allocation_Failed }
        if texture_budget>0 { texture_budget-=1 }
        handle:gfx.Texture_Handle; error:gfx.Gpu_Error
        when R==metal.Renderer { handle,error=metal.create_texture(renderer,descriptor) }
        else { handle,error=vulkan.create_texture(renderer,descriptor) }
        if error==.None && track_candidates { append(&candidate_handles,handle) }
        return handle,error
    }
}
fault_destroy_texture :: proc($R:typeid)->proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error {
    return proc(renderer:^R,handle:gfx.Texture_Handle)->gfx.Gpu_Error {
        error:gfx.Gpu_Error
        when R==metal.Renderer { error=metal.destroy_texture(renderer,handle) }
        else { error=vulkan.destroy_texture(renderer,handle) }
        if error==.None && track_candidates {
            for candidate,i in candidate_handles { if candidate==handle { ordered_remove(&candidate_handles,i); break } }
        }
        return error
    }
}
fault_exports :: proc($R:typeid)->proc(^R,^gfx.Graph)->gfx.Gpu_Error {
    return proc(renderer:^R,graph:^gfx.Graph)->gfx.Gpu_Error {
        if reject_exports { return .Native_Failure }
        when R==metal.Renderer { return metal.release_graph_exports(renderer,graph) }
        else { return vulkan.release_graph_exports(renderer,graph) }
    }
}
Capture_Ops :: struct($R:typeid) {
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
pixels :: proc(renderer:^$R,operations:Capture_Ops(R),ticket:gfx.Readback_Ticket)->gfx.Readback_Data {
    started:=time.tick_now()
    for {
        data,done,error:=operations.poll(renderer,ticket); assert(error==.None)
        if done { return data }
        assert(time.tick_since(started)<10*time.Second); time.sleep(time.Millisecond)
    }
}
render_frame :: proc(scene:^render.Native_Scene($R),frame:render.Frame_Data)->gfx.Submission {
    token,error:=render.native_scene_acquire(scene); assert(error==.None)
    submission,submit_error:=render.native_scene_render(scene,token,frame,nil,nil); assert(submit_error=={})
    assert(render.native_scene_wait(scene,submission)==.None); return submission
}
exercise :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),compiler:^shader.Compiler,descriptor:gfx.Graphics_Desc,model_ops:render.Model_GPU_Ops(R),backend,resource_root:string) {
    texture_budget=-1; reject_exports=false
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None)
    assert(app.asset_resources_init(&owner,filepath.dir(resource_root),resource_root)==.None)
    source,error:=app.scene_model_prepare(&owner,{path="models/Box.gltf"}); assert(error==.None)
    _=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{source,{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1}})
    compiled,compile_error:=render.model_shader_compile(compiler); assert(compile_error==.None); defer render.model_shader_destroy(&compiled)
    config:=render.Model_Config(R){&compiled,model_ops}
    consumer:render.Native_Consumer(R)
    initialize_error:=render.native_consumer_init(&consumer,&owner,renderer,operations,descriptor,3,96,64,models=&config); fmt.println("Resize fixture preparation",backend,initialize_error); assert(initialize_error=={})
    defer { assert(render.native_consumer_destroy(&consumer)==.None) }
    scene:=consumer.active
    camera:=render.camera_default(); camera.position={0,0,3}; camera.target={0,0,0}
    frame,frame_error:=render.frame_data(camera,96,64,backend=="Vulkan1.3"); assert(frame_error==.None)
    baseline_submission:=render_frame(scene,frame)
    baseline_source,source_error:=capture.source(renderer,baseline_submission,scene.graph.color); assert(source_error==.None)
    baseline_ticket,queue_error:=capture.queue(renderer,baseline_source,{width=96,height=64,depth=1,aspect=.Color}); assert(queue_error==.None)
    baseline:=pixels(renderer,capture,baseline_ticket); defer gfx.readback_data_destroy(&baseline)
    non_background:int
    for i:=0;i<len(baseline.bytes);i+=4 { if baseline.bytes[i]!=9 || baseline.bytes[i+1]!=10 || baseline.bytes[i+2]!=13 { non_background+=1 } }
    assert(non_background>64,"real model produced no visible pixels")
    saved_slots:=make([]render.Native_Slot,len(scene.slots)); defer delete(saved_slots); copy(saved_slots,scene.slots)
    saved_model_slots:=make(type_of(scene.models.slots),len(scene.models.slots)); defer delete(saved_model_slots); copy(saved_model_slots,scene.models.slots)
    saved_model:=scene.models
    graph_order:=raw_data(scene.graph.plan.order); graph_passes:=raw_data(scene.graph.graph.passes); graph_revision:=scene.graph.graph.revision
    model_images:=raw_data(scene.models.image_ids); model_passes:=raw_data(scene.models.passes); model_order:=raw_data(scene.models.order); model_frame:=scene.models.frame
    for fault in 0..<4 {
        saved_frame_desc:=scene.models.frame_desc
        switch fault {
        case 0: texture_budget=2
        case 1: texture_budget=6
        case 2: scene.models.frame_desc.size=0
        case 3: reject_exports=true
        }
        track_candidates=true
        rejected:=render.native_scene_resize(scene,80,80)
        track_candidates=false; assert(len(candidate_handles)==0,"failed resize leaked a native candidate texture")
        texture_budget=-1; reject_exports=false; scene.models.frame_desc=saved_frame_desc
        fmt.println("Resize rejection",backend,fault,rejected)
        assert(rejected!={} && scene==consumer.active && scene.models==saved_model)
        assert(scene.graph.color_desc.width==96 && scene.graph.color_desc.height==64 && scene.graph.graph.revision==graph_revision)
        assert(raw_data(scene.graph.plan.order)==graph_order && raw_data(scene.graph.graph.passes)==graph_passes)
        assert(raw_data(scene.models.image_ids)==model_images && raw_data(scene.models.passes)==model_passes && raw_data(scene.models.order)==model_order && scene.models.frame==model_frame && scene.models.graph==&scene.graph)
        for slot,i in scene.slots { assert(slot==saved_slots[i]) }
        for slot,i in scene.models.slots { assert(slot==saved_model_slots[i]) }
        // The exact unqueued old source remains admissible after every failed candidate.
        preserved_ticket,preserved_error:=capture.queue(renderer,baseline_source,{width=96,height=64,depth=1,aspect=.Color}); assert(preserved_error==.None)
        preserved:=pixels(renderer,capture,preserved_ticket); assert(mem.compare(preserved.bytes,baseline.bytes)==0); gfx.readback_data_destroy(&preserved)
        continued_submission:=render_frame(scene,frame)
        continued_source,continued_source_error:=capture.source(renderer,continued_submission,scene.graph.color); assert(continued_source_error==.None)
        continued_ticket,continued_ticket_error:=capture.queue(renderer,continued_source,{width=96,height=64,depth=1,aspect=.Color}); assert(continued_ticket_error==.None)
        continued:=pixels(renderer,capture,continued_ticket); assert(mem.compare(continued.bytes,baseline.bytes)==0); gfx.readback_data_destroy(&continued)
        baseline_source=continued_source
    }
    retained_ticket,retained_error:=capture.queue(renderer,baseline_source,{width=96,height=64,depth=1,aspect=.Color}); assert(retained_error==.None)
    assert(render.native_scene_resize(scene,80,80)=={})
    _,stale_error:=capture.queue(renderer,baseline_source,{width=96,height=64,depth=1,aspect=.Color}); assert(stale_error==.Invalid_Resource)
    retained:=pixels(renderer,capture,retained_ticket); assert(mem.compare(retained.bytes,baseline.bytes)==0); gfx.readback_data_destroy(&retained)
    resized_frame,resized_frame_error:=render.frame_data(camera,80,80,backend=="Vulkan1.3"); assert(resized_frame_error==.None)
    resized_submission:=render_frame(scene,resized_frame)
    resized_source,resized_source_error:=capture.source(renderer,resized_submission,scene.graph.color); assert(resized_source_error==.None)
    resized_ticket,resized_ticket_error:=capture.queue(renderer,resized_source,{width=80,height=80,depth=1,aspect=.Color}); assert(resized_ticket_error==.None)
    resized:=pixels(renderer,capture,resized_ticket); assert(resized.region.width==80 && resized.region.height==80 && len(resized.bytes)==80*80*4); gfx.readback_data_destroy(&resized)
    fmt.println("Transactional resize GPU PASS",backend,"four rejected candidates preserve graph/cache/attachments/exports and byte-exact Box pixels; success retires unqueued old source while retained capture survives")
}
main :: proc() {
    assert(len(os.args)==4,"usage: resize_atomic <naga-library> <vulkan-loader> <resource-root>")
    _=NS.scoped_autoreleasepool()
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0); mem.tracking_allocator_destroy(&tracker) }; context.allocator=mem.tracking_allocator(&tracker)
    candidate_handles=make([dynamic]gfx.Texture_Handle); defer delete(candidate_handles)
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer { assert(shader.compiler_destroy(&compiler)==.None) }
    surface,error:=render.surface_shader_compile(&compiler,.RGBA8_Unorm); assert(error==.None); defer render.surface_shader_destroy(&surface)
    {
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer { assert(metal.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,fault_texture(metal.Renderer),fault_destroy_texture(metal.Renderer),metal.acquire,metal.abort,metal.submit,metal.wait,fault_exports(metal.Renderer)}
        capture:=Capture_Ops(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback}
        exercise(&renderer,operations,capture,&compiler,surface.descriptor,render.Model_GPU_Ops(metal.Renderer){operations,metal.create_sampler,metal.destroy_sampler},"Metal4",os.args[3])
    }
    {
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None); defer { assert(vulkan.validation_error_count(&renderer)==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,fault_texture(vulkan.Renderer),fault_destroy_texture(vulkan.Renderer),vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,fault_exports(vulkan.Renderer)}
        capture:=Capture_Ops(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback}
        exercise(&renderer,operations,capture,&compiler,surface.descriptor,render.Model_GPU_Ops(vulkan.Renderer){operations,vulkan.create_sampler,vulkan.destroy_sampler},"Vulkan1.3",os.args[3])
    }
}
