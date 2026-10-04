#+build darwin,arm64
//! Visible image thumbnails use the production cache and actual retained UI composition.
package main
import render "../../app/render"
import gfx "../../gfx"
import ui "../../ui"
import resources "../../resources"
import shader "../../gfx/shader"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:mem"
import "core:fmt"
import "core:time"
import "core:path/filepath"

API :: struct($R:typeid) {
    ui:render.UI_GPU_Ops(R),acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,release:proc(^R,^gfx.Graph)->gfx.Gpu_Error,
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
reject_upload:bool
fail_upload :: proc($R:typeid)->proc(^R,gfx.Texture_Desc,[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    return proc(renderer:^R,desc:gfx.Texture_Desc,bytes:[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error) {
        if reject_upload { return {},.Allocation_Failed }
        when R==metal.Renderer { return metal.create_texture_with_data(renderer,desc,bytes) }
        else { return vulkan.create_texture_with_data(renderer,desc,bytes) }
    }
}
write_color :: proc(path:string,color:[3]byte) {
    bytes:[58]byte;header:=[]byte{0x42,0x4d,58,0,0,0,0,0,0,0,54,0,0,0,40,0,0,0,1,0,0,0,1,0,0,0,1,0,24,0,0,0,0,0,4,0,0,0};copy(bytes[:],header)
    bytes[54]=color[2];bytes[55]=color[1];bytes[56]=color[0]
    assert(os.write_entire_file(path,bytes[:])==nil)
}
complete :: proc(cache:^render.Thumbnail_Cache($R),requests:[]render.Thumbnail_Request,budget:=4)->render.Thumbnail_Receipt {
    total:=render.thumbnail_cache_update(cache,requests,budget)
    start:=time.tick_now()
    for {
        loading:=false;for request in requests { loading=loading || render.thumbnail_cache_lookup(cache,request.root_identity,request.path).loading }
        if !loading { return total }
        next:=render.thumbnail_cache_update(cache,requests,0);total.published+=next.published;total.failed+=next.failed;if next.error!={} { total.error=next.error }
        assert(time.tick_since(start)<10*time.Second);time.sleep(time.Millisecond)
    }
}
capture :: proc(renderer:^$R,api:API(R),owner:^render.UI_GPU(R),fonts:^render.UI_Font_System,cache:^render.Thumbnail_Cache(R),requests:[]render.Thumbnail_Request,down:bool,expected:[3]byte,release_before_wait:bool)->gfx.Readback_Data {
    clip:=ui.Rect{0,0,96,16}
    commands:=make([]ui.Draw_Command,len(requests)); defer delete(commands)
    for request,index in requests { view:=render.thumbnail_cache_lookup(cache,request.root_identity,request.path); assert(view.ready);commands[index]=ui.Image_Draw{texture=view.texture,bounds={f32(index*16),0,16,16},uv={0,0,1,1},clip=clip,tint={1,1,1,1}} }
    mesh,mesh_error:=render.ui_prepare(fonts,{commands=commands,logical_size={96,16},pixel_scale=1}); assert(mesh_error==.None); defer render.ui_mesh_destroy(&mesh)
    prepared,prepare_error:=render.ui_gpu_prepare(owner,fonts,&mesh); assert(prepare_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    desc:=gfx.Texture_Desc{width=96,height=16,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    target,target_error:=gfx.graph_image(&graph,desc,{initial=.Undefined,final=.Transfer_Source},false,true); assert(target_error==.None)
    handle,handle_error:=owner.ops.create_target(renderer,desc);assert(handle_error==.None)
    input,input_error:=render.ui_graph_append(&graph,owner,&prepared,&mesh,target,desc,true,down);assert(input_error=={});defer render.ui_graph_input_destroy(&input)
    textures:=make([dynamic]gfx.Texture_Input);append(&textures,gfx.Texture_Input{target,handle});append(&textures,..input.textures);defer delete(textures)
    plan,plan_error:=gfx.graph_compile(&graph);assert(plan_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=api.acquire(renderer);assert(acquire_error==.None)
    submission,submit_error,packet_error:=api.submit(renderer,token,&graph,&plan,input.buffers,textures[:]);assert(submit_error==.None&&packet_error==.None)
    source,source_error:=api.source(renderer,submission,target);assert(source_error==.None)
    ticket,ticket_error:=api.queue(renderer,source,{width=96,height=16,depth=1,aspect=.Color});assert(ticket_error==.None)
    assert(render.ui_gpu_frame_destroy(owner,&prepared)==.None)
    assert(owner.ops.destroy_texture(renderer,handle)==.None)
    assert(api.release(renderer,&graph)==.None)
    if release_before_wait { assert(render.thumbnail_cache_destroy(cache)=={});assert(len(owner.textures)==0) }
    assert(api.wait(renderer,submission)==.None)
    start:=time.tick_now();data:gfx.Readback_Data
    for { ready:bool;error:gfx.Gpu_Error;data,ready,error=api.poll(renderer,ticket);assert(error==.None);if ready { break };assert(time.tick_since(start)<10*time.Second);time.sleep(time.Millisecond) }
    for y in 0..<16 { for x in 0..<96 { offset:=int(data.row_pitch)*y+x*4;want:=([4]byte{expected[0],expected[1],expected[2],255});if x>=16 { want={255,0,0,255} };assert(mem.compare(data.bytes[offset:offset+4],want[:])==0,fmt.tprintf("pixel %d,%d %v expected%v",x,y,data.bytes[offset:offset+4],want)) } }
    return data
}
exercise :: proc(renderer:^$R,api:API(R),compiler:^shader.Compiler,fonts:^render.UI_Font_System,down:bool,name:string) {
    directory,error:=os.mkdir_temp("","katla-thumbnails",context.allocator);assert(error==nil);defer { assert(os.remove_all(directory)==nil);delete(directory) }
    paths:[6]string;defer { for path in paths { delete(path) } };for index in 0..<6 { paths[index]=fmt.aprintf("image%d.bmp",index);path,_:=filepath.join({directory,paths[index]});write_color(path,{255,0,0});delete(path) }
    root,root_error:=resources.root_open(directory);assert(root_error==.None);defer resources.root_destroy(&root)
    compiled,compile_error:=render.ui_shader_compile(compiler,.RGBA8_Unorm);assert(compile_error==.None);defer render.ui_shader_destroy(&compiled)
    owner:render.UI_GPU(R);assert(render.ui_gpu_init(&owner,renderer,api.ui,&compiled)==.None);defer assert(render.ui_gpu_destroy(&owner)==.None)
    owner.ops.create_texture=fail_upload(R)
    cache:render.Thumbnail_Cache(R);assert(render.thumbnail_cache_init(&cache,&owner,6,128)=={});defer assert(render.thumbnail_cache_destroy(&cache)=={})
    requests:[6]render.Thumbnail_Request;for path,index in paths { requests[index]={&root,11,path,1} }
    first:=render.thumbnail_cache_update(&cache,requests[:]);assert(first.attempted==4)
    resources.root_destroy(&root)
    drained:=complete(&cache,requests[:],0);assert(drained.published==4)
    root,root_error=resources.root_open(directory);assert(root_error==.None)
    second:=complete(&cache,requests[:]);assert(second.attempted==2&&second.published==2&&len(cache.entries)==6)
    baseline:=capture(renderer,api,&owner,fonts,&cache,requests[:],down,{255,0,0},false);defer gfx.readback_data_destroy(&baseline)
    old:=owner.textures[render.thumbnail_cache_lookup(&cache,11,paths[0]).texture].handle
    requests[0].revision=2
    unchanged:=complete(&cache,requests[:]);assert(unchanged.published==0&&owner.textures[render.thumbnail_cache_lookup(&cache,11,paths[0]).texture].handle==old)
    filename,_:=filepath.join({directory,paths[0]});defer delete(filename)
    assert(os.write_entire_file(filename,([]byte{1,2,3}))==nil);requests[0].revision=3
    rejected:=complete(&cache,requests[:]);assert(rejected.failed==1&&render.thumbnail_cache_lookup(&cache,11,paths[0]).ready)
    assert(render.thumbnail_cache_update(&cache,requests[:]).attempted==0)
    write_color(filename,{0,255,0});requests[0].revision=4;reject_upload=true
    native_rejected:=complete(&cache,requests[:]);reject_upload=false
    assert(native_rejected.error.gpu==.Allocation_Failed&&owner.textures[render.thumbnail_cache_lookup(&cache,11,paths[0]).texture].handle==old)
    preserved:=capture(renderer,api,&owner,fonts,&cache,requests[:],down,{255,0,0},false);defer gfx.readback_data_destroy(&preserved);assert(mem.compare(baseline.bytes,preserved.bytes)==0)
    requests[0].revision=5
    repaired:=complete(&cache,requests[:]);assert(repaired.published==1&&render.thumbnail_cache_lookup(&cache,11,paths[0]).accepted_revision==5)
    green:=capture(renderer,api,&owner,fonts,&cache,requests[:],down,{0,255,0},false);defer gfx.readback_data_destroy(&green)
    requests[0].revision=6;assert(render.thumbnail_cache_update(&cache,requests[:]).attempted==1)
    write_color(filename,{0,0,255});requests[0].revision=7
    stale_start:=time.tick_now()
    for render.thumbnail_cache_lookup(&cache,11,paths[0]).loading {
        _=render.thumbnail_cache_update(&cache,requests[:],0)
        assert(render.thumbnail_cache_lookup(&cache,11,paths[0]).accepted_revision==5,"stale worker published a superseded source revision")
        assert(time.tick_since(stale_start)<10*time.Second);time.sleep(time.Millisecond)
    }
    latest:=complete(&cache,requests[:]);assert(latest.published==1&&render.thumbnail_cache_lookup(&cache,11,paths[0]).accepted_revision==7)
    retained:=capture(renderer,api,&owner,fonts,&cache,requests[:],down,{0,0,255},true);defer gfx.readback_data_destroy(&retained)
    assert(render.thumbnail_cache_init(&cache,&owner,2,128)=={})
    assert(complete(&cache,requests[:2]).published==2)
    separate:=requests[0];separate.root_identity=22
    other:=complete(&cache,{separate});assert(other.published==1)
    a:=render.thumbnail_cache_lookup(&cache,11,paths[0]);b:=render.thumbnail_cache_lookup(&cache,22,paths[0])
    assert(b.ready&&(a.texture==0||a.texture!=b.texture)&&len(cache.entries)==2&&len(owner.textures)==2)
    assert(render.thumbnail_cache_destroy(&cache)=={})
    fmt.println("Native image thumbnail PASS",name,"four-visible worker budget, retained root close, SHA dedup, failed decode/upload preserve live pixels, stale worker rejection, root identity/LRU bounds, accepted frame survives cache removal")
}
main :: proc() {
    assert(len(os.args)==6,"Pass offline compiler,font library,resources,Vulkan loader,--metal/--vulkan/--both")
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0&&len(tracker.bad_free_array)==0);mem.tracking_allocator_destroy(&tracker) }
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer assert(shader.compiler_destroy(&compiler)==.None)
    fonts:render.UI_Font_System;assert(render.ui_font_init(&fonts,os.args[2],os.args[3])==.None);defer render.ui_font_destroy(&fonts)
    backend:=os.args[5];assert(backend=="--metal"||backend=="--vulkan"||backend=="--both")
    if backend=="--metal"||backend=="--both" {
    m:metal.Renderer;assert(metal.renderer_init(&m)==.None)
    ma:=API(metal.Renderer){ui={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.create_texture_with_data,metal.destroy_texture,metal.create_sampler,metal.destroy_sampler,metal.create_texture},acquire=metal.acquire,submit=metal.submit,wait=metal.wait,release=metal.release_graph_exports,source=metal.graph_texture_source,queue=metal.queue_texture_readback,poll=metal.poll_texture_readback}
    exercise(&m,ma,&compiler,&fonts,false,"Metal");assert(metal.renderer_destroy(&m)==.None)
    }
    if backend=="--vulkan"||backend=="--both" {
    v:vulkan.Renderer;assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[4])==.None)
    va:=API(vulkan.Renderer){ui={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture},acquire=vulkan.acquire,submit=vulkan.submit,wait=vulkan.wait,release=vulkan.release_graph_exports,source=vulkan.graph_texture_source,queue=vulkan.queue_texture_readback,poll=vulkan.poll_texture_readback}
    exercise(&v,va,&compiler,&fonts,true,"Vulkan");assert(vulkan.validation_error_count(&v)==0);assert(vulkan.renderer_destroy(&v)==.None)
    }
}
