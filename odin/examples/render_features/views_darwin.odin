#+build darwin, arm64
//! Four independent camera owners append into one authored graph and consume one acquired token.
package main

import app "../../app"
import render "../../app/render"
import gfx "../../gfx"
import ecs "../../ecs"
import shader "../../gfx/shader"
import "core:mem"
import km "../../math"
import "core:fmt"

exercise_views :: proc(owner:^app.Authoring,batch:^render.Scene_Batch,renderer:^$R,ops:render.GPU_Ops(R),captures:Capture(R),pipelines:render.Scene_Pipelines,backend:string,particle_ops:render.Particle_GPU_Ops(R),compiler:^shader.Compiler) {
    views:[4]render.Native_Scene(R)
    for &view in views {
        assert(render.native_scene_init(&view,renderer,ops,pipelines,&batch.geometry,len(batch.objects),3,128,96)=={})
        view.authoring=owner; view.batch=batch
    }
    defer { for &view in views { assert(render.native_scene_destroy(&view)==.None) } }
    config:=app.particle_defaults(); config.emit_rate=0; config.velocity_magnitude=0; config.velocity_cone_angle=0; config.lifetime_variation=0; config.base_scale=0.7; config.scale_variation=0; config.color={0.1,0.8,0.2,1}; config.color_end=config.color; config.color_variation=0; config.gravity=0
    emitter:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,emitter:app.Particle_Emitter}{{km.transform(position={0,2.5,0})},{config}})
    defer ecs.destroy_entity(&owner.world,emitter)
    particles:render.Particle_Consumer(R)
    particle_error,shader_error:=render.particle_consumer_init(&particles,owner,renderer,particle_ops,compiler,.RGBA16_Float,64,4,3,true); assert(particle_error=={} && shader_error==.None)
    defer assert(render.particle_consumer_destroy(&particles)==.None)
    followers:[3]render.Particle_View(R)
    for &follower in followers { assert(render.particle_view_init(&follower,&particles)==.None) }
    defer { for &follower in followers { assert(render.particle_view_destroy(&follower)==.None) } }
    baseline:[4]u64
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    names:=[4]string{"Perspective","Front","Side","Top"}
    positions:=[4]km.Vec3{{0,3,5},{0,1,5},{5,1,0},{0,5,0.01}}
    frames:[4]render.Frame_Data
    for position,i in positions {
        camera:=render.camera_default(); camera.position=position; camera.target={0,0.5,0}; camera.far=150
        error:render.Scene_Error; frames[i],error=render.frame_data(camera,128,96,backend=="vulkan"); assert(error==.None)
    }
    checked:=0
    for cycle in 0..<4 {
        assert(ops.release_exports(renderer,&graph)==.None)
        assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        if cycle==1 {
            assert(app.particle_burst(&owner.world,emitter,32)==.None)
            views[0].composition=render.particle_composition(&particles)
            for &follower,i in followers { views[i+1].composition=render.particle_view_composition(&follower) }
        }
        if cycle>0 { assert(render.particle_frame_delta(&particles,0.1)=={}) }
        token,acquire_error:=ops.acquire(renderer); assert(acquire_error==.None)
        preparations:[4]render.Native_Prepared(R)
        buffers:=make([dynamic]gfx.Buffer_Input); defer delete(buffers)
        textures:=make([dynamic]gfx.Texture_Input); defer delete(textures)
        for &view,i in views {
            error:render.Native_Error
            preparations[i],error=render.native_scene_prepare(&view,token,frames[i],batch.objects,batch.draws,destination=&graph,namespace=names[i]); if error!={} { fmt.println("Combined preparation failed:",backend,cycle,i,error) }; assert(error=={})
            append(&buffers,..preparations[i].buffers[:]); append(&textures,..preparations[i].textures[:])
        }
        if cycle==1 {
            for i:=3;i>=0;i-=1 { render.native_scene_prepared_abort(&preparations[i]) }
            assert(!particles.pending.ready && particles.sequence==0 && len(ecs.get_component_mut(&owner.world,emitter,app.Particle_Emitter).descriptor.burst_queue)==1,"aborted combined frame consumed its authored burst")
            assert(gfx.graph_truncate(&graph,0,0,0)==.None)
            clear(&buffers); clear(&textures)
            for &view,i in views {
                error:render.Native_Error
                preparations[i],error=render.native_scene_prepare(&view,token,frames[i],batch.objects,batch.draws,destination=&graph,namespace=names[i]); assert(error=={})
                append(&buffers,..preparations[i].buffers[:]); append(&textures,..preparations[i].textures[:])
            }
        }
        plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
        submission,gpu_error,packet_error:=ops.submit(renderer,token,&graph,&plan,buffers[:],textures[:]); assert(gpu_error==.None && packet_error==.None)
        for &preparation in preparations { render.native_scene_prepared_accept(&preparation,submission) }
        hashes:[4]u64
        for &view,i in views {
            pixels:=capture(&view,captures,submission,view.graph.output)
            assert(len(pixels.bytes)==128*96*4)
            hash:=u64(14695981039346656037)
            for byte in pixels.bytes { hash=(hash~u64(byte))*1099511628211 }
            hashes[i]=hash; checked+=len(pixels.bytes)/4
            gfx.readback_data_destroy(&pixels)
        }
        for a in 0..<4 { for b in 0..<a { assert(hashes[a]!=hashes[b],"independent viewport cameras produced the same output") } }
        if cycle==0 { baseline=hashes } else {
            for hash,i in hashes { assert(hash!=baseline[i],"same simulated particles did not render through the independent view camera") }
            assert(particles.sequence==u64(cycle) && len(ecs.get_component_mut(&owner.world,emitter,app.Particle_Emitter).descriptor.burst_queue)==0,"shared views committed simulation or consumed the burst more than once")
        }
        assert(ops.wait(renderer,submission)==.None)
        if cycle>0 {
            bytes:=[32+64*68]byte{}
            assert(particle_ops.read_buffer(renderer,particles.slots[token.slot].readback,0,bytes[:])==.None)
            words:=mem.slice_data_cast([]u32,bytes[:]); fmt.println("Shared particle counters:",backend,cycle,words[:8]); assert(words[0]==32 && words[1]==32 && words[2]==32 && words[4]==192 && words[5]==1)
            offset:=32+64*4+int(words[8])*64
            particle:=mem.slice_data_cast([]render.Particle_Data,bytes[offset:offset+64])[0]
            assert(abs(particle.lifetime-(config.base_lifetime-f32(cycle)*0.1))<0.00001,"particles simulated more than once in the shared four-view submission")
        }
        fmt.println("Combined view cameras:",backend,cycle,hashes)
    }
    assert(ops.release_exports(renderer,&graph)==.None)
    fmt.println("Native combined four-view PASS:",backend,"rendered pixels",checked)
}
