#+build darwin, arm64
//! Actual material coverage is shared by scene depth, cascades, selection and independent picking.
package main
import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import km "../../math"
import shader "../../gfx/shader"
import resources "../../resources"
import editor "../../editor"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:mem"
import "core:fmt"
import "core:time"
import "core:path/filepath"

Capture :: struct($R:typeid) { source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) }
read :: proc(renderer:^$R,ops:Capture(R),submission:gfx.Submission,id:gfx.Image_Id,aspect:gfx.Image_Aspect=.Color)->gfx.Readback_Data {
    source,error:=ops.source(renderer,submission,id);assert(error==.None)
    ticket,queue_error:=ops.queue(renderer,source,{width=source.desc.width,height=source.desc.height,depth=1,aspect=aspect});assert(queue_error==.None)
    start:=time.tick_now()
    for { data,ready,poll_error:=ops.poll(renderer,ticket);assert(poll_error==.None);if ready { return data };assert(time.tick_since(start)<10*time.Second);time.sleep(time.Millisecond) }
}
write_alpha :: proc(path:string) {
    data:[62]byte;header:=[]byte{0x42,0x4d,62,0,0,0,0,0,0,0,54,0,0,0,40,0,0,0,2,0,0,0,1,0,0,0,1,0,32,0,0,0,0,0,8,0,0,0};copy(data[:],header)
    data[54]=255;data[55]=255;data[56]=255;data[58]=255;data[59]=255;data[60]=255;data[61]=255
    assert(os.write_entire_file(path,data[:])==nil)
}
Counts :: struct { scene_depth,stencil,shadow,pick,indicator:int }
exercise :: proc(renderer:^$R,ops:render.GPU_Ops(R),captures:Capture(R),compiler:^shader.Compiler,down:bool,name:string) {
    directory,error:=os.make_directory_temp("","katla-alpha-native-*",context.allocator);assert(error==nil);defer { os.remove_all(directory);delete(directory) }
    resource_directory,_:=filepath.join({directory,"resources"});defer delete(resource_directory);assert(os.make_directory(resource_directory)==nil)
    filename,_:=filepath.join({resource_directory,"alpha.bmp"});defer delete(filename);write_alpha(filename)
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);assert(app.authoring_services_init(&owner)==.None);assert(app.asset_resources_init(&owner,directory,resource_directory)==resources.Error.None)
    mesh,mesh_error:=app.scene_mesh_prepare(&owner,{kind=.Geometry,geometry=transmute([]byte)string(`{"kind":"cube","size":[1,1,1]}`)});assert(mesh_error==.None)
    entity:=ecs.spawn(&owner.world,struct {mesh:app.Scene_Mesh,transform:app.Scene_Transform,surface:app.Surface_Material}{mesh,{km.TRANSFORM_IDENTITY},{linear_color={.8,.3,.1,1},has_tint=true,metallic=0,roughness=.7,ao=1}})
    assigned,history:=app.material_texture_execute(&owner,{entities={entity},role=.Albedo,source={kind=.File,path="alpha.bmp",root=.Resource}});defer editor.tool_result_destroy(&assigned);defer editor.undo_group_destroy(&history);assert(assigned.error==.None)
    _=ecs.spawn(&owner.world,struct {sun:app.Scene_Directional_Light}{{{-.3,-.5,-1},{1,1,1},2}})
    surface,compile_error:=render.surface_shader_compile(compiler);assert(compile_error==.None);defer render.surface_shader_destroy(&surface)
    model,model_error:=render.model_shader_compile(compiler);assert(model_error==.None);defer render.model_shader_destroy(&model)
    config:=render.Model_Config(R){&model,{ops,ops.create_sampler,ops.destroy_sampler}}
    consumer:render.Native_Consumer(R);assert(render.native_consumer_init(&consumer,&owner,renderer,ops,render.surface_pipelines(&surface),3,128,128,models=&config)=={});defer assert(render.native_consumer_destroy(&consumer)==.None)
    picking:render.Picking_Native(R);assert(render.picking_native_init(&picking,renderer,ops,compiler)=={});defer assert(render.picking_native_destroy(&picking)==.None)
    graph:gfx.Graph;gfx.graph_init(&graph);defer { assert(ops.release_exports(renderer,&graph)==.None);gfx.graph_destroy(&graph) }
    occluder_id:ecs.Entity_Id
    results:[11]Counts
    occluded_colors:[2]gfx.Readback_Data
    defer { for &data in occluded_colors { gfx.readback_data_destroy(&data) } }
    for cycle in 0..<11 {
        if cycle==7 {
            occluder,prepare_mesh_error:=app.scene_mesh_prepare(&owner,{kind=.Geometry,geometry=transmute([]byte)string(`{"kind":"cube","size":[1,1,1]}`)});assert(prepare_mesh_error==.None)
            occluder_id=ecs.spawn(&owner.world,struct {mesh:app.Scene_Mesh,transform:app.Scene_Transform,surface:app.Surface_Material}{occluder,{km.transform(position={0,0,.8})},{linear_color={.1,.2,.7,1},has_tint=true,metallic=0,roughness=.7,ao=1}})
        }
        authored:=ecs.get_component_mut(&owner.world,entity,app.Surface_Material)
        authored.surface=app.material_surface_default();authored.has_surface=true
        authored.surface.alpha_mode=.Opaque if cycle==0 || cycle==7 else .Blend if cycle==3 || cycle==6 || cycle==10 else .Mask
        authored.surface.alpha_cutoff=2 if cycle==2 || cycle==9 else .5
        authored.linear_color.a=1e-8 if cycle==3 else 0 if cycle==6 || cycle==10 else 1
        authored.surface.double_sided=cycle==5
        transform:=ecs.get_component_mut(&owner.world,entity,app.Scene_Transform);transform.local.scale[0]= -1 if cycle==4 else 1
        refreshed:=render.native_consumer_refresh(&consumer);if refreshed!={} { fmt.println("Coverage refresh",cycle,refreshed) };assert(refreshed=={})
        scene:=consumer.active;scene.feature_settings.grid=false;scene.feature_settings.sky=false;scene.feature_settings.shadow_size=128;assert(render.native_scene_resize(scene,128,128)=={});assert(render.native_scene_select(scene,{entity})=={})
        assert(ops.release_exports(renderer,&graph)==.None);assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        camera:=render.camera_default();camera.position={0,0,3};camera.far=20
        frame,frame_error:=render.frame_data(camera,128,128,down);assert(frame_error==.None)
        token,acquire_error:=ops.acquire(renderer);assert(acquire_error==.None)
        prepared,prepare_error:=render.native_scene_prepare(scene,token,frame,consumer.batch.objects,consumer.batch.draws,destination=&graph,namespace="Coverage");if prepare_error!={} { fmt.println("Coverage prepare",cycle,prepare_error) };assert(prepare_error=={});defer render.native_scene_prepared_abort(&prepared)
        draws,draw_error:=render.picking_scene_draws(scene,token,picking.pipelines);assert(draw_error=={});defer delete(draws)
        id_desc:=gfx.Texture_Desc{width=128,height=128,mip_levels=1,layers=1,depth=1,format=.R32_Uint,usage={.Color_Attachment,.Transfer_Source}}
        depth_desc:=gfx.Texture_Desc{width=128,height=128,mip_levels=1,layers=1,depth=1,format=.D32_Float,usage={.Depth_Attachment,.Transfer_Source}}
        id_handle,id_error:=ops.create_texture(renderer,id_desc);assert(id_error==.None);defer assert(ops.destroy_texture(renderer,id_handle)==.None)
        depth_handle,depth_error:=ops.create_texture(renderer,depth_desc);assert(depth_error==.None);defer assert(ops.destroy_texture(renderer,depth_handle)==.None)
        id,_:=gfx.graph_image(&graph,id_desc,{},false,true);pick_depth,_:=gfx.graph_image(&graph,depth_desc,{},false,true)
        input,input_error:=render.picking_graph_append(&graph,picking.pipelines.opaque[0],{scene.graph.frame,scene.slots[token.slot].frame,scene.graph.frame_desc},draws,id,pick_depth);assert(input_error=={});defer render.picking_graph_input_destroy(&input)
        for value in input.buffers { found:=false;for previous in prepared.buffers { if previous.resource==value.resource { assert(previous.handle==value.handle);found=true;break } };if !found { append(&prepared.buffers,value) } }
        for value in input.textures { found:=false;for previous in prepared.textures { if previous.resource==value.resource { assert(previous.handle==value.handle);found=true;break } };if !found { append(&prepared.textures,value) } }
        append(&prepared.textures,gfx.Texture_Input{id,id_handle},gfx.Texture_Input{pick_depth,depth_handle})
        for index in ([]int{scene.graph.depth.index,scene.graph.features.atlas.index,scene.graph.features.indicator.index}) { graph.images[index].exported=true };graph.revision+=1
        plan,graph_error:=gfx.graph_compile(&graph);assert(graph_error==.None);defer gfx.compiled_graph_destroy(&plan)
        submission,gpu_error,packet_error:=ops.submit(renderer,token,&graph,&plan,prepared.buffers[:],prepared.textures[:]);if gpu_error!=.None || packet_error!=.None { fmt.println("Coverage submit",cycle,gpu_error,packet_error) };assert(gpu_error==.None&&packet_error==.None);render.native_scene_prepared_accept(&prepared,submission)
        depth:=read(renderer,captures,submission,scene.graph.depth,.Depth);defer gfx.readback_data_destroy(&depth)
        stencil:=read(renderer,captures,submission,scene.graph.depth,.Stencil);defer gfx.readback_data_destroy(&stencil)
        shadow:=read(renderer,captures,submission,scene.graph.features.atlas,.Depth);defer gfx.readback_data_destroy(&shadow)
        ids:=read(renderer,captures,submission,id);defer gfx.readback_data_destroy(&ids)
        pick_depth_data:=read(renderer,captures,submission,pick_depth,.Depth);defer gfx.readback_data_destroy(&pick_depth_data)
        indicator:=read(renderer,captures,submission,scene.graph.features.indicator);defer gfx.readback_data_destroy(&indicator)
        color:=read(renderer,captures,submission,scene.graph.output);defer gfx.readback_data_destroy(&color)
        if cycle==2 || cycle==6 { for offset:=0;offset<len(color.bytes);offset+=4 { assert(mem.compare(color.bytes[offset:offset+4],color.bytes[:4])==0,"rejected coverage changed clear color") } }
        if cycle<7 && cycle!=3 {
            for value,i in mem.slice_data_cast([]f32,depth.bytes) { assert((value<1)==(mem.slice_data_cast([]u32,ids.bytes)[i]!=0),"scene and independent picking disagree on pixel coverage") }
        }
        if cycle==7 || cycle==9 { occluded_colors[0 if cycle==7 else 1]=color;color={} }
        counts:=&results[cycle]
        for value in mem.slice_data_cast([]f32,depth.bytes) { if value<1 { counts.scene_depth+=1 } }
        for value in stencil.bytes { if value!=0 { counts.stencil+=1 } }
        for value in mem.slice_data_cast([]f32,shadow.bytes) { if value<1 { counts.shadow+=1 } }
        for value,i in mem.slice_data_cast([]u32,ids.bytes) { if value!=0 { counts.pick+=1;mapped:=false;for entry in input.entries { if entry.encoded==value { assert(entry.entity==(entity if cycle<7 else occluder_id));mapped=true;break } };assert(mapped);assert(mem.slice_data_cast([]f32,pick_depth_data.bytes)[i]<1) } else { assert(mem.slice_data_cast([]f32,pick_depth_data.bytes)[i]==1) } }
        for value in indicator.bytes { if value>127 { counts.indicator+=1 } }
        assert(ops.wait(renderer,submission)==.None)
        fmt.println("Native coverage",name,cycle,counts^)
    }
    assert(results[0].scene_depth>1000&&results[0].shadow>0&&results[0].pick>1000&&results[0].stencil>1000)
    assert(results[1].scene_depth>100&&results[1].scene_depth<results[0].scene_depth&&results[1].shadow>0&&results[1].shadow<results[0].shadow)
    assert(results[1].pick==results[1].scene_depth&&results[1].stencil>=results[1].scene_depth)
    assert(results[2]==Counts{},"mask cutoff>1 did not reject all depth/shadow/stencil/picking fragments")
    assert(results[3].scene_depth==0&&results[3].shadow==0&&results[3].pick==results[0].pick&&results[3].stencil==results[0].stencil,"positive blend alpha lost independent picking or wrote scene/shadow depth")
    assert(results[4].scene_depth==results[1].scene_depth&&results[4].pick==results[1].pick&&results[4].stencil>=results[4].pick&&results[4].shadow>0,"mirrored material lost front-face coverage")
    assert(results[5].scene_depth==results[5].pick&&results[5].pick>results[1].pick&&results[5].stencil>=results[5].pick&&results[5].shadow>0,"two-sided masked cube did not expose the actual back faces through front holes")
    assert(results[6]==Counts{},"zero Blend alpha did not discard all coverage")
    assert(results[7].indicator>1000&&results[8].indicator>100&&results[8].indicator<results[7].indicator,"actual occluded selection mask was not generated with alpha coverage")
    assert(results[7].shadow>results[8].shadow&&results[8].shadow>results[9].shadow&&results[9].shadow==results[10].shadow,"accepted occluder shadow concealed selected alpha coverage")
    assert(results[9].indicator==0&&results[10].indicator==0,"rejected coverage leaked into the actual occluded selection mask")
    assert(mem.compare(occluded_colors[0].bytes,occluded_colors[1].bytes)!=0,"occluded selection mask did not alter the actual final scene pixels")
    fmt.println("Native material coverage PASS",name,"Opaque/Mask/high-cutoff/positiveBlend/mirrored/two-sided shared scene+stencil+4cascade shadow+selection+independent picking")
}
main :: proc() {
    assert(len(os.args)==3,"Pass offline compiler and Vulkan loader")
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0&&len(tracker.bad_free_array)==0);mem.tracking_allocator_destroy(&tracker) }
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer assert(shader.compiler_destroy(&compiler)==.None)
    m:metal.Renderer;assert(metal.renderer_init(&m)==.None)
    exercise(&m,render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler},Capture(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},&compiler,false,"Metal");assert(metal.renderer_destroy(&m)==.None)
    v:vulkan.Renderer;assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[2])==.None)
    exercise(&v,render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler},Capture(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},&compiler,true,"Vulkan");assert(vulkan.validation_error_count(&v)==0);assert(vulkan.renderer_destroy(&v)==.None)
}
