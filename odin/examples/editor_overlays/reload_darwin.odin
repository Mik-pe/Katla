#+build darwin, arm64
//! Real native shader family rollback and publication preserve resource owners and future scene rebuild behavior.
package main

import app "../../app"
import render "../../app/render"
import gfx "../../gfx"
import shader "../../gfx/shader"
import ecs "../../ecs"
import km "../../math"
import resources "../../resources"
import "core:os"
import "core:path/filepath"
import "core:mem"
import "core:fmt"
import "core:strings"

reload_draw :: proc(consumer:^render.Native_Consumer($R),captures:Capture(R),frame:render.Frame_Data)->gfx.Readback_Data {
    scene:=consumer.active; token,error:=render.native_scene_acquire(scene); assert(error==.None)
    submission,submit_error:=render.native_scene_render(scene,token,frame,consumer.batch.objects,consumer.batch.draws); assert(submit_error=={})
    result:=capture(scene,captures,submission,scene.graph.output); assert(render.native_scene_wait(scene,submission)==.None); return result
}
reload_draw_overlay :: proc(consumer:^render.Native_Consumer($R),overlays:^render.Overlay_Native(R),captures:Capture(R),frame:render.Frame_Data,graph:^gfx.Graph)->gfx.Readback_Data {
    scene:=consumer.active; assert(scene.operations.release_exports(scene.renderer,graph)==.None); assert(gfx.graph_truncate(graph,0,0,0)==.None); token,error:=render.native_scene_acquire(scene); assert(error==.None)
    prepared,prepare_error:=render.native_scene_prepare(scene,token,frame,consumer.batch.objects,consumer.batch.draws,destination=graph,namespace="Reload effects"); assert(prepare_error=={}); defer render.native_scene_prepared_abort(&prepared)
    mesh,mesh_error:=render.overlay_mesh_prepare(consumer.authoring,{billboards=true},frame,128,128); assert(mesh_error==.None); defer render.overlay_mesh_destroy(&mesh)
    imports:render.Overlay_Graph; upload,overlay_error:=render.overlay_native_append(overlays,&prepared,&mesh,frame,&imports); if overlay_error!={} { fmt.println("Reload overlay append:",overlay_error) }; assert(overlay_error=={}); defer assert(render.overlay_frame_destroy(overlays,&upload)==.None)
    plan,graph_error:=gfx.graph_compile(graph); assert(graph_error==.None); defer gfx.compiled_graph_destroy(&plan)
    submission,gpu_error,packet_error:=scene.operations.submit(scene.renderer,token,graph,&plan,prepared.buffers[:],prepared.textures[:]); assert(gpu_error==.None && packet_error==.None)
    render.native_scene_prepared_accept(&prepared,submission)
    result:=capture(scene,captures,submission,scene.graph.output); assert(scene.operations.wait(scene.renderer,submission)==.None); return result
}
reload_broken_entry :: proc(entry:^shader.Entry) {
    delete(entry.metal_name); entry.metal_name=strings.clone("missing_reload_entry")
    offset:=5
    for offset<len(entry.spirv) { word:=entry.spirv[offset]; count:=int(word>>16); assert(count>0 && count<=len(entry.spirv)-offset); if word&0xffff==15 { entry.spirv[offset+3]~=1; return }; offset+=count }
    panic("actual selected SPIR-V had no entry instruction")
}
exercise_reload :: proc(renderer:^$R,ops:render.GPU_Ops(R),overlay_ops:render.UI_GPU_Ops(R),captures:Capture(R),surface:^render.Surface_Shader,backend:string,compiler:^shader.Compiler,overlay_reference:^render.Overlay_Shader,fonts:^render.UI_Font_System,particle_ops:render.Particle_GPU_Ops(R)) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); assert(app.authoring_services_init(&owner)==.None)
    _=spawn_mesh(&owner,`{"kind":"cube","size":[1,1,1]}`,{0,0,0},{.9,.2,.1,1})
    cwd,cwd_error:=os.get_working_directory(context.allocator); assert(cwd_error==nil); defer delete(cwd)
    resources_path,path_error:=filepath.join({cwd,"resources"}); assert(path_error==nil); defer delete(resources_path); assert(app.asset_resources_init(&owner,cwd,resources_path)==resources.Error.None)
    actual_model,actual_model_error:=app.scene_model_prepare(&owner,{path="models/Box.gltf"}); assert(actual_model_error==.None)
    _=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{actual_model,{km.transform(position={-.9,0,0},scale={.5,.5,.5})},{metallic=1,roughness=1,ao=1}})
    model,error:=render.model_shader_compile(compiler); assert(error==.None); defer render.model_shader_destroy(&model)
    model_config:=render.Model_Config(R){&model,{ops,ops.create_sampler,ops.destroy_sampler}}
    consumer:render.Native_Consumer(R); assert(render.native_consumer_init(&consumer,&owner,renderer,ops,render.surface_pipelines(surface),3,128,128,models=&model_config)=={}); defer assert(render.native_consumer_destroy(&consumer)==.None)
    consumer.active.feature_settings.shadows=false; consumer.active.feature_settings.sky=false; consumer.active.feature_settings.grid=false; consumer.active.feature_settings.outline=false
    camera:=render.camera_default(); camera.position={0,0,3}; frame,frame_error:=render.frame_data(camera,128,128,backend=="vulkan"); assert(frame_error==.None)
    baseline:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&baseline)
    original:[8]shader.Compiled
    references:=[8]^shader.Compiled{&surface.compiled,&surface.postprocess.compiled,&surface.features.compiled[0],&surface.features.compiled[1],&surface.features.compiled[2],&surface.features.compiled[3],&surface.features.compiled[4],&model.compiled}
    for reference,index in references { original[index]=render.shader_reload_snapshot(reference) }
    defer { for &compiled in original { shader.compiled_destroy(&compiled) } }
    consumers:=[1]^render.Native_Consumer(R){&consumer}
    broken:=[8]shader.Compiled{}
    for &artifact,index in original { broken[index]=render.shader_reload_snapshot(&artifact) }
    reload_broken_entry(&broken[2].entries[0])
    live:=consumer.active.pipeline
    failed,failed_error:=render.scene_shader_reload_prepare(consumers[:],surface,&model,broken[:]); assert(failed!=nil && failed_error==.Prepare); render.scene_shader_reload_destroy(failed)
    for &artifact in broken { shader.compiled_destroy(&artifact) }
    assert(consumer.active.pipeline==live,"partial native candidate failure published live pipeline handles")
    kept:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&kept); assert(mem.compare(baseline.bytes,kept.bytes)==0)
    token,acquire_error:=render.native_scene_acquire(consumer.active); assert(acquire_error==.None)
    prepared,prepare_error:=render.native_scene_prepare(consumer.active,token,frame,consumer.batch.objects,consumer.batch.draws); assert(prepare_error=={})
    busy,busy_error:=render.scene_shader_reload_prepare(consumers[:],surface,&model,original[:]); assert(busy==nil && busy_error==.Busy)
    render.native_scene_prepared_abort(&prepared); assert(ops.abort(renderer,token)==.None)
    source,owned:=strings.replace_all(render.SURFACE_SOURCE,"// #include lighting_common",render.LIGHTING_COMMON); defer { if owned { delete(source) } }
    changed_source,changed_owned:=strings.replace_all(source,"return vec4<f32>(radiance, surface.linear_color.a);","return vec4<f32>(radiance * vec3<f32>(0.15, 1.0, 1.0), surface.linear_color.a);"); assert(changed_owned); defer delete(changed_source)
    changed,compile_error:=shader.compile(compiler,changed_source,{{"vs_main",.Vertex},{"fs_main",.Fragment}}); assert(compile_error==.None); defer shader.compiled_destroy(&changed)
    model_source,model_owned:=strings.replace_all(render.MODEL_SOURCE,"// #include lighting_common",render.LIGHTING_COMMON); defer { if model_owned { delete(model_source) } }
    model_changed_source,model_changed_owned:=strings.replace_all(model_source,"return vec4<f32>(radiance, alpha);","return vec4<f32>(radiance * vec3<f32>(1.0, 0.1, 1.0), alpha);"); assert(model_changed_owned); defer delete(model_changed_source)
    changed_model,model_compile_error:=shader.compile(compiler,model_changed_source,{{"vs_model",.Vertex},{"fs_model",.Fragment}}); assert(model_compile_error==.None); defer shader.compiled_destroy(&changed_model)
    artifacts:=original; artifacts[0]=changed; artifacts[7]=changed_model
    next,candidate_error:=render.scene_shader_reload_prepare(consumers[:],surface,&model,artifacts[:]); if candidate_error!=.None { fmt.println("Scene reload candidate:",backend,candidate_error) }; assert(candidate_error==.None)
    old_scene:=consumer.active; old_geometry:=old_scene.geometry; old_frame:=old_scene.slots[0].frame
    old_token,old_acquire_error:=render.native_scene_acquire(old_scene); assert(old_acquire_error==.None)
    accepted,accepted_error:=render.native_scene_render(old_scene,old_token,frame,consumer.batch.objects,consumer.batch.draws); assert(accepted_error=={})
    captured_source,source_error:=captures.source(renderer,accepted,old_scene.graph.output); assert(source_error==.None)
    ticket,ticket_error:=captures.queue(renderer,captured_source,{width=128,height=128,depth=1,aspect=.Color}); assert(ticket_error==.None)
    previous:=render.scene_shader_reload_publish(next); render.scene_shader_reload_destroy(previous)
    assert(consumer.active==old_scene && old_scene.geometry==old_geometry && old_scene.slots[0].frame==old_frame,"shader-only publication replaced application resource owners")
    recolored:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&recolored); assert(changed_pixels(&baseline,&recolored,6)>500,"accepted actual shader edit did not change scene pixels")
    model_pixels,primitive_pixels:int
    for y in 0..<128 { for x in 0..<128 { offset:=y*int(baseline.row_pitch)+x*4; changed_here:=false; for channel in 0..<3 { if abs(int(baseline.bytes[offset+channel])-int(recolored.bytes[offset+channel]))>6 { changed_here=true } }; if changed_here { if x<40 { model_pixels+=1 }; if x>55 { primitive_pixels+=1 } } } }
    assert(model_pixels>50 && primitive_pixels>50,"actual model and primitive shader families did not both change their separated geometry pixels")
    assert(render.native_scene_wait(old_scene,accepted)==.None)
    retained,ready,retained_error:=captures.poll(renderer,ticket); assert(ready && retained_error==.None); defer gfx.readback_data_destroy(&retained); assert(mem.compare(baseline.bytes,retained.bytes)==0,"queued old shader frame changed after public pipeline retirement")
    staging,stage_error:=render.native_consumer_prepare(&consumer,&owner,nil,.Insert); assert(stage_error==.None); render.native_consumer_finish(&consumer,staging,true)
    rebuilt:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&rebuilt); assert(mem.compare(recolored.bytes,rebuilt.bytes)==0,"future scene upload used a stale pre-reload descriptor snapshot")
    restored,restore_error:=render.scene_shader_reload_prepare(consumers[:],surface,&model,original[:]); assert(restore_error==.None); render.scene_shader_reload_destroy(render.scene_shader_reload_publish(restored))
    restored_pixels:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&restored_pixels); assert(mem.compare(baseline.bytes,restored_pixels.bytes)==0)
    particles:render.Particle_Consumer(R); particle_error,particle_compile_error:=render.particle_consumer_init(&particles,&owner,renderer,particle_ops,compiler,.RGBA16_Float,64,4,3); assert(particle_error=={} && particle_compile_error==.None); defer assert(render.particle_consumer_destroy(&particles)==.None)
    config:=app.particle_defaults(); config.emit_rate=0; config.gravity=0; config.velocity_magnitude=0; config.base_scale=.7; config.scale_variation=0; config.lifetime_variation=0; config.color={.1,.8,.2,1}; config.color_end=config.color; config.color_variation=0
    emitter:=ecs.spawn(&owner.world,struct {emitter:app.Particle_Emitter,transform:app.Scene_Transform}{{config},{km.transform(position={.75,.75,0})}})
    assert(render.particle_frame_delta(&particles,.25)=={}); assert(app.particle_burst(&owner.world,emitter,16)==.None); assert(render.native_consumer_compose(&consumer,render.particle_composition(&particles))=={})
    particle_before:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&particle_before); assert(render.particle_frame_delta(&particles,0)=={})
    particle_source,particle_owned:=strings.replace_all(render.PARTICLE_RENDER,"#include \"common.wgsl\"",render.PARTICLE_COMMON); defer { if particle_owned { delete(particle_source) } }
    particle_changed_source,particle_changed_owned:=strings.replace_all(particle_source,"return vec4f(rgb,vertex.color.a*falloff);","return vec4f(rgb * vec3f(1.0, 0.05, 1.0),vertex.color.a*falloff);"); assert(particle_changed_owned); defer delete(particle_changed_source)
    changed_particle,changed_particle_compile_error:=shader.compile(compiler,particle_changed_source,{{"vs_main",.Vertex},{"fs_main",.Fragment}}); assert(changed_particle_compile_error==.None); defer shader.compiled_destroy(&changed_particle)
    particle_artifacts:=[5]shader.Compiled{particles.shaders.compiled[0],particles.shaders.compiled[1],particles.shaders.compiled[2],particles.shaders.compiled[3],changed_particle}
    particle_candidate,particle_candidate_error:=render.particle_shader_reload_prepare(&particles,particle_artifacts[:]); assert(particle_candidate_error==.None); data,sequence:=particles.data,particles.sequence; render.particle_shader_reload_destroy(render.particle_shader_reload_publish(particle_candidate,render.scene_graph_target(&consumer.active.graph))); assert(particles.data==data && particles.sequence==sequence)
    particle_after:=reload_draw(&consumer,captures,frame); defer gfx.readback_data_destroy(&particle_after); particle_pixels:=changed_pixels(&particle_before,&particle_after,6); assert(render.particle_observe(&particles)=={}); fmt.println("Particle reload diagnostics:",backend,particle_pixels,particles.alive_upper,particles.observed_alive); assert(particle_pixels>40,"accepted particle shader did not change its actual indirect billboard pixels")
    _=ecs.spawn(&owner.world,struct {billboard:app.Scene_Billboard,transform:app.Scene_Transform}{{icon=.Lightbulb,color={.9,.2,.1,1},size=1},{km.transform(position={-.75,.9,0})}})
    overlays:render.Overlay_Native(R); assert(render.overlay_native_init(&overlays,renderer,overlay_ops,overlay_reference,fonts)=={}); defer assert(render.overlay_native_destroy(&overlays)==.None)
    assert(ops.release_exports(renderer,render.scene_graph_target(&consumer.active.graph))==.None)
    effects_graph:gfx.Graph; gfx.graph_init(&effects_graph); defer { assert(ops.release_exports(renderer,&effects_graph)==.None); gfx.graph_destroy(&effects_graph) }
    overlay_before:=reload_draw_overlay(&consumer,&overlays,captures,frame,&effects_graph); defer gfx.readback_data_destroy(&overlay_before)
    overlay_changed_source,overlay_changed_owned:=strings.replace_all(render.OVERLAY_SOURCE,"return vec4f(input.color.rgb,alpha);","return vec4f(input.color.rgb * vec3f(0.05, 1.0, 1.0),alpha);"); assert(overlay_changed_owned); defer delete(overlay_changed_source)
    changed_overlay,overlay_compile_error:=shader.compile(compiler,overlay_changed_source,{{"vs_overlay",.Vertex},{"fs_overlay",.Fragment}}); assert(overlay_compile_error==.None); defer shader.compiled_destroy(&changed_overlay)
    overlay_artifacts:=[1]shader.Compiled{changed_overlay}
    overlay_candidate,overlay_candidate_error:=render.overlay_shader_reload_prepare(&overlays,overlay_reference,overlay_artifacts[:]); assert(overlay_candidate_error==.None); glyphs:=overlays.glyphs; original_overlay:=render.shader_reload_snapshot(&overlay_reference.compiled); defer shader.compiled_destroy(&original_overlay); render.overlay_shader_reload_destroy(render.overlay_shader_reload_publish(overlay_candidate,render.scene_graph_target(&consumer.active.graph))); assert(overlays.glyphs==glyphs)
    overlay_after:=reload_draw_overlay(&consumer,&overlays,captures,frame,&effects_graph); defer gfx.readback_data_destroy(&overlay_after); overlay_pixels:=changed_pixels(&overlay_before,&overlay_after,6); assert(overlay_pixels>40,"accepted overlay shader did not change actual alpha-masked glyph pixels")
    restore_overlay,restore_overlay_error:=render.overlay_shader_reload_prepare(&overlays,overlay_reference,{original_overlay}); assert(restore_overlay_error==.None); render.overlay_shader_reload_destroy(render.overlay_shader_reload_publish(restore_overlay,render.scene_graph_target(&consumer.active.graph)))
    restored_overlay:=reload_draw_overlay(&consumer,&overlays,captures,frame,&effects_graph); defer gfx.readback_data_destroy(&restored_overlay); assert(mem.compare(overlay_before.bytes,restored_overlay.bytes)==0)
    fmt.println("Reload changed particle/overlay pixels:",backend,particle_pixels,overlay_pixels)
    fmt.println("Native shader family PASS:",backend,"partial native rollback, Busy, changed pixels, pending previous frame, future rebuild, restored source, particle/overlay variants and resource identity")
}
