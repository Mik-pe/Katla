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

exercise_particle_reset :: proc(owner:^app.Authoring,batch:^render.Scene_Batch,renderer:^$R,ops:render.GPU_Ops(R),captures:Capture(R),pipelines:render.Scene_Pipelines,backend:string,particle_ops:render.Particle_GPU_Ops(R),compiler:^shader.Compiler) {
    views:[4]render.Native_Scene(R)
    for &view in views {
        assert(render.native_scene_init(&view,renderer,ops,pipelines,&batch.geometry,len(batch.objects),3,128,96)=={})
        view.authoring=owner; view.batch=batch
    }
    defer { for &view in views { assert(render.native_scene_destroy(&view)==.None) } }
    config:=app.particle_defaults(); config.emit_rate=0; config.velocity_magnitude=0; config.velocity_cone_angle=0; config.lifetime_variation=0; config.base_scale=0.7; config.scale_variation=0; config.color={0.1,0.8,0.2,1}; config.color_end=config.color; config.color_variation=0; config.gravity=0; config.has_timed_emission=true; config.timed_emission=1
    emitter:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,emitter:app.Particle_Emitter}{{km.transform(position={0,2.5,0})},{config}})
    defer ecs.destroy_entity(&owner.world,emitter)
    particles:render.Particle_Consumer(R)
    particle_error,shader_error:=render.particle_consumer_init(&particles,owner,renderer,particle_ops,compiler,.RGBA16_Float,64,4,3,true); assert(particle_error=={} && shader_error==.None)
    defer assert(render.particle_consumer_destroy(&particles)==.None)
    followers:[3]render.Particle_View(R)
    for &follower in followers { assert(render.particle_view_init(&follower,&particles)==.None) }
    defer { for &follower in followers { assert(render.particle_view_destroy(&follower)==.None) } }
    baseline,recovered:[4]u64
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    names:=[4]string{"Perspective","Front","Side","Top"}
    positions:=[4]km.Vec3{{0,3,5},{0,1,5},{5,1,0},{0,5,0.01}}
    frames:[4]render.Frame_Data
    for position,i in positions {
        camera:=render.camera_default(); camera.position=position; camera.target={0,0.5,0}; camera.far=150
        error:render.Scene_Error; frames[i],error=render.frame_data(camera,128,96,backend=="vulkan"); assert(error==.None)
    }
    scratch_graph:gfx.Graph; gfx.graph_init(&scratch_graph); defer gfx.graph_destroy(&scratch_graph)
    scratch_desc:=gfx.Buffer_Desc{size=16,usage={.Storage,.Transfer_Destination},memory=.CPU_Visible}
    zero:[16]byte; scratch_handle,scratch_error:=ops.create_buffer(renderer,scratch_desc,zero[:]); assert(scratch_error==.None); defer assert(ops.destroy_buffer(renderer,scratch_handle)==.None)
    scratch,scratch_graph_error:=gfx.graph_buffer(&scratch_graph,scratch_desc,true,false); assert(scratch_graph_error==.None)
    scratch_pass,scratch_pass_error:=gfx.graph_pass(&scratch_graph,"Independent upload acquisition",.Transfer,{{scratch,{0,16},.Write,.Transfer_Destination}},true); assert(scratch_pass_error==.None)
    assert(gfx.graph_set_packet(&scratch_graph,scratch_pass,gfx.Fill_Buffer{scratch,0,16,0x12345678})==.None)
    scratch_plan,scratch_plan_error:=gfx.graph_compile(&scratch_graph); assert(scratch_plan_error==.None); defer gfx.compiled_graph_destroy(&scratch_plan)
    checked:=0
    for cycle in 0..<5 {
        assert(ops.release_exports(renderer,&graph)==.None)
        assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        if cycle==1 {
            assert(app.particle_burst(&owner.world,emitter,16)==.None)
            views[0].composition=render.particle_composition(&particles)
            for &follower,i in followers { views[i+1].composition=render.particle_view_composition(&follower) }
        }
        if cycle>0 { assert(render.particle_frame_delta(&particles,.25 if cycle==1 || cycle==3 else 0)=={}) }
        if cycle==2 {
            assert(app.particle_burst(&owner.world,emitter,8)==.None)
            for _ in 0..<4 { assert(render.particle_reset_all(&particles)=={}) }
        }
        token,acquire_error:=ops.acquire(renderer); assert(acquire_error==.None)
        if cycle>=3 {
            for token.slot!=particles.previous_slot {
                uploaded,upload_error,upload_packet_error:=ops.submit(renderer,token,&scratch_graph,&scratch_plan,{{scratch,scratch_handle}},nil); assert(upload_error==.None && upload_packet_error==.None)
                assert(ops.wait(renderer,uploaded)==.None)
                token,acquire_error=ops.acquire(renderer); assert(acquire_error==.None)
            }
            assert(token.slot==particles.previous_slot,"native fixture did not exercise repeated particle frame-slot acquisition")
        }
        preparations:[4]render.Native_Prepared(R)
        buffers:=make([dynamic]gfx.Buffer_Input); defer delete(buffers)
        textures:=make([dynamic]gfx.Texture_Input); defer delete(textures)
        for &view,i in views {
            error:render.Native_Error
            preparations[i],error=render.native_scene_prepare(&view,token,frames[i],batch.objects,batch.draws,destination=&graph,namespace=names[i]); if error!={} { fmt.println("Combined preparation failed:",backend,cycle,i,error) }; assert(error=={})
            append(&buffers,..preparations[i].buffers[:]); append(&textures,..preparations[i].textures[:])
        }
        if cycle==1 || cycle==2 {
            old_data,old_dead:=particles.data,particles.dead
            if cycle==2 { assert(render.particle_reset_all(&particles).gpu==.Busy && particles.reset_requested && particles.observed_alive==16) }
            for i:=3;i>=0;i-=1 { render.native_scene_prepared_abort(&preparations[i]) }
            assert(!particles.pending.ready && particles.sequence==u64(cycle-1) && len(ecs.get_component_mut(&owner.world,emitter,app.Particle_Emitter).descriptor.burst_queue)==1,"aborted combined frame consumed its authored burst")
            assert(particles.data==old_data && particles.dead==old_dead,"aborted reset replaced accepted GPU pool parents")
            if cycle==2 {
                assert(particles.reset_requested)
                old:[16]byte; assert(particle_ops.read_buffer(renderer,particles.slots[particles.previous_slot].readback,0,old[:])==.None)
                assert(mem.slice_data_cast([]u32,old[:])[0]==16,"aborted reset cleared accepted live GPU counters")
            }
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
        if cycle==0 { baseline=hashes }
        else {
            for hash,i in hashes {
                if cycle==2 { assert(hash==baseline[i],"accepted global reset left visible particles in a follower view") }
                else { assert(hash!=baseline[i],"accepted burst did not reach every independent camera") }
            }
            if cycle==3 { recovered=hashes }; if cycle==4 { assert(hashes==recovered,"stationary recovered particles changed without accepted simulation time") }
            emitter_live:=ecs.get_component_mut(&owner.world,emitter,app.Particle_Emitter)
            assert(particles.sequence==u64(cycle) && emitter_live.descriptor.active && emitter_live.descriptor.emit_rate==0 && emitter_live.descriptor.timed_emission==1)
            assert(len(emitter_live.descriptor.burst_queue)==(1 if cycle==2 else 0),"reset consumed pending authored burst or a follower committed it twice")
        }
        assert(ops.wait(renderer,submission)==.None)
        if cycle>0 {
            bytes:=[32+64*68]byte{}
            assert(particle_ops.read_buffer(renderer,particles.slots[token.slot].readback,0,bytes[:])==.None)
            words:=mem.slice_data_cast([]u32,bytes[:]); expected:=u32(16 if cycle==1 else (0 if cycle==2 else 8))
            assert(words[0]==expected && words[1]==64-expected && words[4]==expected*6 && words[5]==1)
            assert(render.particle_observe(&particles)=={} && particles.observed_alive==expected)
            status,present:=render.particle_emitter_status(&particles,emitter); assert(present && status.timed && abs(status.remaining_duration-(.75 if cycle<3 else .5))<.00001,"global reset changed authored/runtime emission clocks")
            if cycle==2 {
                assert(!particles.reset_requested)
                storage:=mem.slice_data_cast([]render.Particle_Data,bytes[32+64*4:]); for particle in storage { assert(particle.lifetime==0,"accepted reset retained live storage lifetime") }
            }
        }
        fmt.println("Global reset four views:",backend,cycle,hashes)
    }
    assert(ops.release_exports(renderer,&graph)==.None)
    fmt.println("Native global particle reset PASS:",backend,"rendered pixels",checked)
}
