#+build darwin, arm64
//! Actual scene/model/particle/feature packets prove explicit forward and reverse camera depth on both backends.
package main

import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import shader "../../gfx/shader"
import resources "../../resources"
import "core:os"
import "core:path/filepath"
import "core:mem"
import "core:fmt"

exercise_depth :: proc(renderer:^$R,ops:render.GPU_Ops(R),overlay_ops:render.UI_GPU_Ops(R),captures:Capture(R),pipelines:render.Scene_Pipelines,backend:string,compiler:^shader.Compiler,overlay_shader:^render.Overlay_Shader,fonts:^render.UI_Font_System,particle_ops:render.Particle_GPU_Ops(R)) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); assert(app.authoring_services_init(&owner)==.None)
    cwd,os_error:=os.get_working_directory(context.allocator); assert(os_error==nil); defer delete(cwd)
    assets,path_error:=filepath.join({cwd,"resources"}); assert(path_error==nil); defer delete(assets); assert(app.asset_resources_init(&owner,cwd,assets)==resources.Error.None)
    selected:=spawn_mesh(&owner,`{"kind":"cube","size":[1,1,1]}`,{.8,.5,0},{.8,.2,.1,1})
    _=spawn_mesh(&owner,`{"kind":"plane","width":8,"height":8}`,{0,0,0},{.15,.15,.15,1})
    source,source_error:=app.scene_model_prepare(&owner,{path="models/Box.gltf"}); assert(source_error==.None)
    _=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{source,{km.transform(position={-.8,.5,0},scale={.8,.8,.8})},{metallic=1,roughness=1,ao=1}})
    _=ecs.spawn(&owner.world,struct {sun:app.Scene_Directional_Light}{{{-1,-1,-1},{1,1,1},2}})
    particle_config:=app.particle_defaults(); particle_config.emit_rate=0; particle_config.gravity=0; particle_config.velocity_magnitude=0; particle_config.base_scale=.3; particle_config.scale_variation=0; particle_config.lifetime_variation=0; particle_config.color={.1,.8,.2,1}; particle_config.color_end=particle_config.color; particle_config.color_variation=0
    emitter:=ecs.spawn(&owner.world,struct {emitter:app.Particle_Emitter,transform:app.Scene_Transform}{{particle_config},{km.transform(position={0,1.3,0})}})
    model_shader,compile_error:=render.model_shader_compile(compiler); assert(compile_error==.None); defer render.model_shader_destroy(&model_shader)
    model_config:=render.Model_Config(R){&model_shader,{ops,ops.create_sampler,ops.destroy_sampler}}
    consumer:render.Native_Consumer(R); init_error:=render.native_consumer_init(&consumer,&owner,renderer,ops,pipelines,3,256,192,models=&model_config); assert(init_error=={}); defer assert(render.native_consumer_destroy(&consumer)==.None)
    scene:=consumer.active; scene.feature_settings.shadow_size=256; assert(render.native_scene_resize(scene,256,192)=={}); assert(render.native_scene_select(scene,{selected})=={})
    prove_shadow_receiver(scene,captures,selected,consumer.batch.objects,consumer.batch.draws,backend)
    particles:render.Particle_Consumer(R); particle_error,particle_shader_error:=render.particle_consumer_init(&particles,&owner,renderer,particle_ops,compiler,.RGBA16_Float,64,4,3,true); assert(particle_error=={} && particle_shader_error==.None); defer assert(render.particle_consumer_destroy(&particles)==.None)
    assert(render.particle_frame_delta(&particles,.25)=={}); assert(app.particle_burst(&owner.world,emitter,16)==.None); assert(render.native_consumer_compose(&consumer,render.particle_composition(&particles))=={})
    overlays:render.Overlay_Native(R); assert(render.overlay_native_init(&overlays,renderer,overlay_ops,overlay_shader,fonts)=={}); defer assert(render.overlay_native_destroy(&overlays)==.None)
    picking:render.Picking_Native(R); assert(render.picking_native_init(&picking,renderer,ops,compiler)=={}); defer assert(render.picking_native_destroy(&picking)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(ops.release_exports(renderer,&graph)==.None); gfx.graph_destroy(&graph) }
    images:[2]gfx.Readback_Data; depth:[2]gfx.Readback_Data; ids:[2]gfx.Readback_Data
    defer { for &image in images { gfx.readback_data_destroy(&image) }; for &image in depth { gfx.readback_data_destroy(&image) }; for &image in ids { gfx.readback_data_destroy(&image) } }
    camera:=render.camera_default(); camera.position={0,2.5,5}; camera.target={0,.6,0}; camera.far=1000
    for cycle in 0..<3 {
        sense:=render.Depth_Sense.Reverse if cycle==1 else .Forward
        assert(ops.release_exports(renderer,&graph)==.None); assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        token,acquire_error:=ops.acquire(renderer); assert(acquire_error==.None)
        frame,frame_error:=render.frame_data(camera,256,192,backend=="vulkan"); assert(frame_error==.None)
        if sense==.Reverse { view,valid:=km.inverse(km.mat4_lookat(camera.position,camera.target,camera.up)); assert(valid); frame.view_projection=km.matrix_mul(km.mat4_reverse_z(camera.fov_degrees,256.0/192.0,camera.near),view) }
        prepared,prepare_error:=render.native_scene_prepare(scene,token,frame,consumer.batch.objects,consumer.batch.draws,destination=&graph,namespace="Depth contract",depth_sense=sense); if prepare_error!={} { fmt.println("Depth preparation:",backend,sense,prepare_error) }; assert(prepare_error=={})
        mesh,mesh_error:=render.overlay_mesh_prepare(&owner,{billboards=true,gizmo=true,mode=.Translate,selected={selected},pivot={.8,.5,0},basis=km.identity(km.Mat4)},frame,256,192); assert(mesh_error==.None); defer render.overlay_mesh_destroy(&mesh)
        imports:render.Overlay_Graph; upload,overlay_error:=render.overlay_native_append(&overlays,&prepared,&mesh,frame,&imports); assert(overlay_error=={})
        chosen:=render.picking_native_pipelines(&picking,sense)
        draws,picking_draw_error:=render.picking_scene_draws(scene,token,chosen); assert(picking_draw_error=={}); defer delete(draws)
        id_desc:=gfx.Texture_Desc{width=256,height=192,depth=1,layers=1,mip_levels=1,format=.R32_Uint,usage={.Color_Attachment,.Transfer_Source}}
        pick_depth_desc:=gfx.Texture_Desc{width=256,height=192,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment}}
        id_handle,id_error:=ops.create_texture(renderer,id_desc); assert(id_error==.None); defer assert(ops.destroy_texture(renderer,id_handle)==.None)
        depth_handle,depth_error:=ops.create_texture(renderer,pick_depth_desc); assert(depth_error==.None); defer assert(ops.destroy_texture(renderer,depth_handle)==.None)
        id,id_graph_error:=gfx.graph_image(&graph,id_desc,{},false,true); assert(id_graph_error==.None)
        pick_depth,pick_depth_graph_error:=gfx.graph_image(&graph,pick_depth_desc,{},false,false); assert(pick_depth_graph_error==.None)
        pick_input,pick_append_error:=render.picking_graph_append(&graph,chosen.opaque[0],{scene.graph.frame,scene.slots[token.slot].frame,scene.graph.frame_desc},draws,id,pick_depth,depth_sense=sense); assert(pick_append_error=={}); defer render.picking_graph_input_destroy(&pick_input)
        for value in pick_input.buffers { found:=false; for previous in prepared.buffers { if previous.resource==value.resource { assert(previous.handle==value.handle); found=true; break } }; if !found { append(&prepared.buffers,value) } }
        for value in pick_input.textures { found:=false; for previous in prepared.textures { if previous.resource==value.resource { assert(previous.handle==value.handle); found=true; break } }; if !found { append(&prepared.textures,value) } }
        append(&prepared.textures,gfx.Texture_Input{id,id_handle},gfx.Texture_Input{pick_depth,depth_handle})
        graph.images[scene.graph.depth.index].exported=true; graph.revision+=1
        plan,graph_error:=gfx.graph_compile(&graph); assert(graph_error==.None); defer gfx.compiled_graph_destroy(&plan)
        submission,gpu_error,packet_error:=ops.submit(renderer,token,&graph,&plan,prepared.buffers[:],prepared.textures[:]); if gpu_error!=.None || packet_error!=.None { fmt.println("Depth submit:",backend,sense,gpu_error,packet_error) }; assert(gpu_error==.None && packet_error==.None)
        render.native_scene_prepared_accept(&prepared,submission); assert(render.particle_frame_delta(&particles,0)=={}); assert(render.overlay_frame_destroy(&overlays,&upload)==.None)
        index:=int(sense)
        identifiers:=capture(scene,captures,submission,id)
        pixels:=capture(scene,captures,submission,scene.graph.output); depths:=capture(scene,captures,submission,scene.graph.depth,.Depth)
        assert(ops.wait(renderer,submission)==.None)
        if cycle<2 { images[index]=pixels; depth[index]=depths; ids[index]=identifiers } else { assert(mem.compare(images[0].bytes,pixels.bytes)==0,"returning to forward depth changed the exact scene pixels"); assert(mem.compare(ids[0].bytes,identifiers.bytes)==0); gfx.readback_data_destroy(&identifiers); gfx.readback_data_destroy(&pixels); gfx.readback_data_destroy(&depths) }
    }
    assert(mem.compare(ids[0].bytes,ids[1].bytes)==0,"picking did not preserve exact nearest generational identity under reverse depth")
    different:=changed_pixels(&images[0],&images[1],3); fmt.println("Depth color differences:",backend,different); assert(different<128,"camera depth sense changed composed scene/model/grid/outline/particle/overlay pixels")
    forward:=mem.slice_data_cast([]f32,depth[0].bytes); reverse:=mem.slice_data_cast([]f32,depth[1].bytes); drawn,clear:int
    for value,i in forward {
        if value==1 { assert(reverse[i]==0,"reverse camera did not preserve clear0 outside geometry"); clear+=1 }
        else { expected:=1-value*(camera.far-camera.near)/camera.far; assert(abs(reverse[i]-expected)<2e-5,"column-major depth does not represent the same nearest visible geometry"); drawn+=1 }
    }
    assert(drawn>2000 && clear>1000)
    fmt.println("Native explicit depth PASS:",backend,"drawn",drawn,"clear",clear,"color differences",different,"scene+model+particles+grid+shadow+outline+overlays")
}
