#+build darwin,arm64
//! Native GPU particle state, indirect commands and rendered pixels on both production adapters.
package main
import app "../../app"
import render "../../app/render"
import scene_ops "../../agent/scene"
import editor "../../editor"
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
import "core:math"

Capture_Ops :: struct($R:typeid) { source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) }
mode :: proc(owner:^app.Authoring,action:scene_ops.Simulation_Op) {
    result,undo:=app.simulation_execute(owner,action); assert(result.error==.None)
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
}
particle_snapshot :: proc(renderer:^$R,operations:render.Particle_GPU_Ops(R),particles:^render.Particle_Consumer(R),token:gfx.Frame_Token,alive:u32,lifetime:f32)->[]byte {
    bytes:=make([]byte,32+int(particles.capacity)*68)
    assert(operations.read_buffer(renderer,particles.slots[token.slot].readback,0,bytes)==.None)
    words:=mem.slice_data_cast([]u32,bytes)
    assert(words[0]==alive && words[1]==particles.capacity-alive)
    assert(words[4]==alive*6 && words[5]==1 && words[6]==0 && words[7]==0)
    seen:=make([]bool,int(particles.capacity)); defer delete(seen)
    for i in 0..<int(alive) {
        particle_index:=words[8+i]; assert(particle_index<particles.capacity && !seen[particle_index]); seen[particle_index]=true
        offset:=32+int(particles.capacity)*4+int(particle_index)*64
        particle:=mem.slice_data_cast([]render.Particle_Data,bytes[offset:offset+64])[0]
        assert(particle.lifetime==lifetime && particle.position==([3]f32{}) && particle.color==([4]f32{0,1,0,1}) && particle.scale==0.75)
    }
    return bytes
}
exercise :: proc(renderer:^$R,operations:render.GPU_Ops(R),particle_ops:render.Particle_GPU_Ops(R),capture:Capture_Ops(R),compiler:^shader.Compiler,descriptor:render.Scene_Pipelines,backend:string) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None)
    resource_root:=fmt.aprintf("%s/resources",os.args[5]); defer delete(resource_root)
    assert(app.asset_resources_init(&owner,os.args[5],resource_root)==.None)
    assert(app.script_native_init(&owner,os.args[3])==.None)
    assert(app.physics_select_box3d(&owner,os.args[4])==.None)
    descriptor_particle:=app.particle_defaults(); descriptor_particle.emit_rate=0; descriptor_particle.velocity_magnitude=0; descriptor_particle.velocity_cone_angle=0; descriptor_particle.lifetime_variation=0; descriptor_particle.base_scale=0.75; descriptor_particle.scale_variation=0; descriptor_particle.color={0,1,0,1}; descriptor_particle.color_end=descriptor_particle.color; descriptor_particle.color_variation=0; descriptor_particle.gravity=0
    descriptor_particle.active=false
    actor:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,body:app.Physics_Body}{{km.transform(position={3,0,0})},app.physics_body({kind=.Sphere,radius=0.25},.Kinematic)})
    actions:=[1]scene_ops.Event_Action{{kind=.Emit,name="prefab_activated"}}
    rules:=[1]scene_ops.Trigger_Rule{{phase=.Enter,other=actor,has_other=true,once=true,actions=actions[:]}}
    trigger,undo:=app.trigger_execute(&owner,{action=.Create_Box,name="Particle entrance",half_extents={1,1,1},rules=rules[:]}); assert(trigger.error==.None && len(trigger.entities)==1)
    entity:=trigger.entities[0]; editor.tool_result_destroy(&trigger); editor.undo_group_destroy(&undo)
    ecs.add_component(&owner.world,entity,app.Particle_Emitter{descriptor_particle})
    attached,attached_undo:=app.behavior_execute(&owner,{action=.Set_Script,entity=entity,path="scripts/prefab-effect.luau"}); assert(attached.error==.None)
    editor.tool_result_destroy(&attached); editor.undo_group_destroy(&attached_undo)
    mode(&owner,.Play); defer mode(&owner,.Stop)
    assert(app.simulation_step(&owner,0.1)==.None)
    ecs.get_component_mut(&owner.world,actor,app.Scene_Transform).local.position={0,0,0}
    assert(app.simulation_step(&owner,0.1)==.None)
    queued:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter)
    assert(queued.descriptor.active && len(queued.descriptor.burst_queue)==1 && queued.descriptor.burst_queue[0]==32)
    queued.descriptor.has_timed_emission=true; queued.descriptor.timed_emission=.5
    scene:render.Native_Scene(R); geometry:=render.Geometry{}
    assert(render.native_scene_init(&scene,renderer,operations,descriptor,&geometry,0,3,128,128)=={}); defer { assert(render.native_scene_destroy(&scene)==.None) }
    particles:render.Particle_Consumer(R)
    error,shader_error:=render.particle_consumer_init(&particles,&owner,renderer,particle_ops,compiler,.RGBA16_Float,64,4,3,true)
    fmt.println("Particle native preparation",backend,error,shader_error); assert(error=={} && shader_error==.None)
    defer { assert(render.particle_consumer_destroy(&particles)==.None) }
    scene.feature_settings.sky=false; scene.feature_settings.grid=false; scene.feature_settings.shadows=false; scene.feature_settings.outline=false
    scene.composition=render.particle_composition(&particles); assert(render.particle_frame_delta(&particles,0.25)=={})
    frame:=render.Frame_Data{view_projection=km.identity(km.Mat4),ambient={0,0,0,1}}
    token,acquire_error:=render.native_scene_acquire(&scene); assert(acquire_error==.None)
    submission,submit_error:=render.native_scene_render(&scene,token,frame,nil,nil); fmt.println("Particle submit",submit_error); assert(submit_error=={})
    emitter:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter); assert(len(emitter.descriptor.burst_queue)==0)
    status,found_status:=render.particle_emitter_status(&particles,entity); assert(found_status && status.timed && status.remaining_duration==.25 && status.sequence==particles.sequence)
    assert(emitter.descriptor.has_timed_emission && emitter.descriptor.timed_emission==.5 && emitter.descriptor.active,"accepted native clock mutated authored duration/active")
    source,source_error:=capture.source(renderer,submission,scene.graph.output); assert(source_error==.None)
    ticket,ticket_error:=capture.queue(renderer,source,{width=128,height=128,depth=1,aspect=.Color}); assert(ticket_error==.None)
    assert(render.native_scene_wait(&scene,submission)==.None)
    bytes:=particle_snapshot(renderer,particle_ops,&particles,token,32,4.75); defer delete(bytes)
    started:=time.tick_now(); captured:=false
    for !captured {
        data,done,poll_error:=capture.poll(renderer,ticket); assert(poll_error==.None)
        if done {
            center:=int(64*data.row_pitch+64*4)
            linear:=scene.feature_settings.postprocess.exposure
            aces:=clamp(linear*(2.51*linear+.03)/(linear*(2.43*linear+.59)+.14),0,1)
            expected:=(12.92*aces if aces<=.0031308 else 1.055*math.pow(aces,1/f32(2.4))-.055)*255
            assert(data.bytes[center]==0 && abs(f32(data.bytes[center+1])-expected)<1.6 && data.bytes[center+2]==0 && data.bytes[center+3]==255,"authored green particle must pass through linear HDR, ACES and exactly one sRGB transfer")
            corner:=int(4*data.row_pitch+4*4); assert(f32(data.bytes[corner+1])<expected*.25,"particle rasterization covered a background corner")
            gfx.readback_data_destroy(&data); captured=true
        } else { assert(time.tick_since(started)<10*time.Second); time.sleep(time.Millisecond) }
    }
    assert(render.particle_observe(&particles)=={} && particles.alive_upper==32)
    assert(app.particle_burst(&owner.world,entity,33)==.None)
    failed_token,failed_acquire:=render.native_scene_acquire(&scene); assert(failed_acquire==.None)
    _,failed_submit:=render.native_scene_render(&scene,failed_token,render.Frame_Data{ambient={0,0,0,1}},nil,nil)
    assert(failed_submit!={} && len(emitter.descriptor.burst_queue)==1 && particles.sequence==1 && particles.previous_slot==token.slot)
    assert(operations.abort(renderer,failed_token)==.None)
    next_token,next_acquire:=render.native_scene_acquire(&scene); assert(next_acquire==.None)
    next_submission,next_error:=render.native_scene_render(&scene,next_token,frame,nil,nil); fmt.println("Particle deferred submit",next_error); assert(next_error=={})
    assert(len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==33)
    assert(render.native_scene_wait(&scene,next_submission)==.None)
    next_bytes:=particle_snapshot(renderer,particle_ops,&particles,next_token,32,4.5); delete(next_bytes)
    old_bytes:=make([]byte,len(bytes)); defer delete(old_bytes)
    assert(particle_ops.read_buffer(renderer,particles.slots[token.slot].readback,0,old_bytes)==.None)
    for value,i in bytes { assert(old_bytes[i]==value,"later frames overwrote the previous capture owner") }
    assert(render.particle_frame_delta(&particles,5)=={})
    dead_token,dead_acquire:=render.native_scene_acquire(&scene); assert(dead_acquire==.None)
    dead_submission,dead_error:=render.native_scene_render(&scene,dead_token,frame,nil,nil); assert(dead_error=={})
    assert(render.native_scene_wait(&scene,dead_submission)==.None)
    dead_bytes:=particle_snapshot(renderer,particle_ops,&particles,dead_token,0,0); delete(dead_bytes)
    assert(len(emitter.descriptor.burst_queue)==1 && render.particle_observe(&particles)=={} && particles.alive_upper==0)
    assert(render.native_scene_resize(&scene,96,112)=={})
    assert(render.particle_frame_delta(&particles,0.25)=={})
    reuse_token,reuse_acquire:=render.native_scene_acquire(&scene); assert(reuse_acquire==.None && reuse_token.slot==token.slot)
    reuse_submission,reuse_error:=render.native_scene_render(&scene,reuse_token,frame,nil,nil); assert(reuse_error=={})
    assert(len(emitter.descriptor.burst_queue)==0 && render.native_scene_wait(&scene,reuse_submission)==.None)
    reuse_bytes:=particle_snapshot(renderer,particle_ops,&particles,reuse_token,33,4.75); delete(reuse_bytes)
    emitter.descriptor.kill_on_destroy=true
    assert(render.particle_frame_delta(&particles,0)=={})
    configured_token,configured_acquire:=render.native_scene_acquire(&scene); assert(configured_acquire==.None)
    configured_submission,configured_error:=render.native_scene_render(&scene,configured_token,frame,nil,nil); assert(configured_error=={})
    assert(render.native_scene_wait(&scene,configured_submission)==.None)
    emitter.descriptor.active=false
    killed_token,killed_acquire:=render.native_scene_acquire(&scene); assert(killed_acquire==.None)
    killed_submission,killed_error:=render.native_scene_render(&scene,killed_token,frame,nil,nil); assert(killed_error=={})
    assert(render.native_scene_wait(&scene,killed_submission)==.None)
    killed_bytes:=particle_snapshot(renderer,particle_ops,&particles,killed_token,0,0); delete(killed_bytes)
    fmt.println("Particle GPU PASS",backend,"actual Box3D enter → Luau 32 → GPU 192 vertices/pixels; failed-frame queue retention, deferred 33 burst, survivor rollover, death, slot reuse, resize and kill-on-disable")
}
exercise_queued :: proc(renderer:^$R,operations:render.GPU_Ops(R),particle_ops:render.Particle_GPU_Ops(R),compiler:^shader.Compiler,descriptor:render.Scene_Pipelines,backend:string) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    config:=app.particle_defaults(); config.emit_rate=0; config.velocity_direction={1,0,0}; config.velocity_magnitude=1; config.velocity_cone_angle=0; config.lifetime_variation=0; config.base_scale=0.75; config.scale_variation=0; config.color={1,0,0,1}; config.color_end=config.color; config.color_variation=0; config.gravity=-2
    first:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform(position={-0.5,0,0})},{config}})
    config.color={0,1,0,1}; config.color_end=config.color
    second:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform(position={0.5,0,0})},{config}})
    assert(app.particle_burst(&owner.world,first,300)==.None && app.particle_burst(&owner.world,second,150)==.None)
    scene:render.Native_Scene(R); geometry:=render.Geometry{}
    assert(render.native_scene_init(&scene,renderer,operations,descriptor,&geometry,0,3,64,64)=={}); defer { assert(render.native_scene_destroy(&scene)==.None) }
    particles:render.Particle_Consumer(R)
    error,shader_error:=render.particle_consumer_init(&particles,&owner,renderer,particle_ops,compiler,.RGBA16_Float,512,4,3,true)
    assert(error=={} && shader_error==.None); defer { assert(render.particle_consumer_destroy(&particles)==.None) }
    scene.feature_settings.sky=false; scene.feature_settings.grid=false; scene.feature_settings.shadows=false; scene.feature_settings.outline=false
    scene.composition=render.particle_composition(&particles); assert(render.particle_frame_delta(&particles,0.25)=={})
    frame:=render.Frame_Data{view_projection=km.identity(km.Mat4),ambient={0,0,0,1}}
    submissions:[3]gfx.Submission
    for &submission,index in submissions {
        if index==1 { assert(app.particle_burst(&owner.world,first,62)==.None) }
        token,acquire_error:=render.native_scene_acquire(&scene); assert(acquire_error==.None)
        submit_error:render.Native_Error; submission,submit_error=render.native_scene_render(&scene,token,frame,nil,nil); assert(submit_error=={})
    }
    assert(len(scene.pending)==3 && particles.alive_upper==512)
    assert(render.native_scene_wait(&scene,submissions[2])==.None)
    assert(render.native_scene_wait(&scene,submissions[0])==.None && render.native_scene_wait(&scene,submissions[1])==.None)
    for submission,frame_index in submissions {
        bytes:=make([]byte,32+512*68)
        assert(particle_ops.read_buffer(renderer,particles.slots[submission.token.slot].readback,0,bytes)==.None)
        words:=mem.slice_data_cast([]u32,bytes); expected:=u32(450) if frame_index==0 else u32(512)
        assert(words[0]==expected && words[1]==512-expected && words[4]==expected*6 && words[5]==1)
        seen:[512]bool; emitter_counts:[2]u32; old_count,new_count:u32
        for i in 0..<int(expected) {
            particle_index:=words[8+i]; assert(particle_index<512 && !seen[particle_index]); seen[particle_index]=true
            offset:=32+512*4+int(particle_index)*64
            particle:=mem.slice_data_cast([]render.Particle_Data,bytes[offset:offset+64])[0]
            assert(particle.emitter_index<2); emitter_counts[particle.emitter_index]+=1
            old_lifetime:=5-f32(frame_index+1)*0.25
            is_old:=particle.lifetime==old_lifetime
            if is_old { old_count+=1 } else { assert(frame_index>0 && particle.emitter_index==0 && particle.lifetime==old_lifetime+0.25); new_count+=1 }
            steps:=frame_index+1 if is_old else frame_index
            position_x:=(-f32(0.5) if particle.emitter_index==0 else f32(0.5))+f32(steps)*0.25*particle.velocity[0]
            position_y:=-f32(steps*(steps-1))*0.0625
            assert(particle.velocity[0]>=0.5 && particle.velocity[0]<=1.5)
            assert(math.abs(particle.position[0]-position_x)<0.000002 && particle.position[1]==position_y && particle.position[2]==0)
            assert(particle.velocity[1]==-f32(steps)*0.5 && particle.velocity[2]==0)
            color:=([4]f32{1,0,0,1}) if particle.emitter_index==0 else ([4]f32{0,1,0,1})
            assert(particle.color==color && particle.scale==0.75)
        }
        assert(old_count==450 && new_count==(0 if frame_index==0 else 62))
        assert(emitter_counts[0]==(300 if frame_index==0 else 362) && emitter_counts[1]==150)
        delete(bytes)
    }
    assert(render.particle_observe(&particles)=={} && particles.alive_upper==512 && particles.observed_alive==512)
    assert(ecs.destroy_entity(&owner.world,first))
    config.color={0,0,1,1}; config.color_end=config.color
    replacement:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{config}})
    assert(replacement!=first && app.particle_burst(&owner.world,replacement,1)==.None)
    retained_token,retained_acquire:=render.native_scene_acquire(&scene); assert(retained_acquire==.None)
    retained_submission,retained_error:=render.native_scene_render(&scene,retained_token,frame,nil,nil); assert(retained_error=={})
    assert(render.native_scene_wait(&scene,retained_submission)==.None)
    retired_bytes:=make([]byte,32+512*68); defer delete(retired_bytes)
    assert(particle_ops.read_buffer(renderer,particles.slots[retained_token.slot].readback,0,retired_bytes)==.None)
    retired_words:=mem.slice_data_cast([]u32,retired_bytes); assert(retired_words[0]==512 && retired_words[1]==0)
    retained_counts:[2]u32
    for i in 0..<512 {
        offset:=32+512*4+int(retired_words[8+i])*64
        particle:=mem.slice_data_cast([]render.Particle_Data,retired_bytes[offset:offset+64])[0]
        assert(particle.emitter_index<2); retained_counts[particle.emitter_index]+=1
        color:=([4]f32{1,0,0,1}) if particle.emitter_index==0 else ([4]f32{0,1,0,1})
        assert(particle.color==color,"replacement emitter overwrote a surviving generation's configuration")
    }
    assert(retained_counts==([2]u32{362,150}))
    replacement_emitter:=ecs.get_component_mut(&owner.world,replacement,app.Particle_Emitter)
    assert(len(replacement_emitter.descriptor.burst_queue)==1)
    assert(render.particle_frame_delta(&particles,5)=={})
    empty_token,empty_acquire:=render.native_scene_acquire(&scene); assert(empty_acquire==.None)
    empty_submission,empty_error:=render.native_scene_render(&scene,empty_token,frame,nil,nil); assert(empty_error=={})
    assert(render.native_scene_wait(&scene,empty_submission)==.None)
    empty_bytes:=particle_snapshot(renderer,particle_ops,&particles,empty_token,0,0); delete(empty_bytes)
    assert(render.particle_observe(&particles)=={} && particles.alive_upper==0 && len(replacement_emitter.descriptor.burst_queue)==1)
    assert(render.particle_frame_delta(&particles,0.25)=={})
    replacement_token,replacement_acquire:=render.native_scene_acquire(&scene); assert(replacement_acquire==.None)
    replacement_submission,replacement_error:=render.native_scene_render(&scene,replacement_token,frame,nil,nil); assert(replacement_error=={})
    assert(render.native_scene_wait(&scene,replacement_submission)==.None && len(replacement_emitter.descriptor.burst_queue)==0)
    assert(particle_ops.read_buffer(renderer,particles.slots[replacement_token.slot].readback,0,retired_bytes)==.None)
    retired_words=mem.slice_data_cast([]u32,retired_bytes); assert(retired_words[0]==1 && retired_words[1]==511 && retired_words[4]==6)
    offset:=32+512*4+int(retired_words[8])*64
    replacement_particle:=mem.slice_data_cast([]render.Particle_Data,retired_bytes[offset:offset+64])[0]
    assert(replacement_particle.color==([4]f32{0,0,1,1}) && replacement_particle.emitter_index==1 && replacement_particle.lifetime==4.75)
    fmt.println("Queued particle GPU PASS",backend,"three concurrent slots; exact 300+150+62 spawns, multiple native workgroups, previous-frame captures, gravity/bounds at 512 capacity; delete/recreate preserves old red configuration until GPU death then safely reclaims")
}
main :: proc() {
    assert(len(os.args)==6,"usage: particles_render <shader-compiler> <vulkan-loader> <luau-library> <box3d-library> <project-root>")
    _=NS.scoped_autoreleasepool()
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0,"particle owner leaked"); mem.tracking_allocator_destroy(&tracker) }; context.allocator=mem.tracking_allocator(&tracker)
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer { assert(shader.compiler_destroy(&compiler)==.None) }
    surface,error:=render.surface_shader_compile(&compiler,.RGBA8_Unorm); assert(error==.None); defer render.surface_shader_destroy(&surface)
    {
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer { assert(metal.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
        particle_ops:=render.Particle_GPU_Ops(metal.Renderer){metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer}
        exercise(&renderer,operations,particle_ops,Capture_Ops(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},&compiler,render.surface_pipelines(&surface),"Metal4")
        exercise_queued(&renderer,operations,particle_ops,&compiler,render.surface_pipelines(&surface),"Metal4")
    }
    {
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None); defer { assert(vulkan.validation_error_count(&renderer)==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
        particle_ops:=render.Particle_GPU_Ops(vulkan.Renderer){vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer}
        exercise(&renderer,operations,particle_ops,Capture_Ops(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},&compiler,render.surface_pipelines(&surface),"Vulkan1.3")
        exercise_queued(&renderer,operations,particle_ops,&compiler,render.surface_pipelines(&surface),"Vulkan1.3")
    }
}
