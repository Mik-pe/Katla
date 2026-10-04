#+test
package app
import "core:testing"
import "core:mem"
import "core:os"
import "core:strings"
import ecs "../ecs"
import script "../script"
import audio "../audio"
import resources "../resources"
import editor "../editor"
import km "../math"

@(private="file")
native_script_commands_queries_audio_variables_and_input :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-direct-script-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_dir:=strings.concatenate({directory,"/resources"}); defer delete(resource_dir); testing.expect(t,os.make_directory(resource_dir)==nil)
    script_dir:=strings.concatenate({directory,"/resources/scripts"}); defer delete(script_dir); testing.expect(t,os.make_directory(script_dir)==nil)
    source:=`speed=2
        local ray,overlaps,done=false
        function on_spawn(entity,world)
            ray=world:raycast(Vec3.new(2.5,0,0),Vec3.new(1,0,0),10)
            overlaps=world:query_trigger_overlaps(world:find_entity("Sensor"))
            world:set_velocity(entity,Vec3.new(2,0,0))
            world:apply_force(entity,Vec3.new(1,0,0))
            world:apply_impulse(entity,Vec3.new(1,0,0))
            world:play_sound("tone.wav",.4,false)
            world:play_sound_at("tone.wav",Vec3.new(1,0,0),.4,false)
            world:play_sound_cue("step")
            print("native",entity:id())
        end
        function on_update(entity,world,dt)
            if world:is_action_pressed("move_forward") then
                local dx,dy=world:get_mouse_delta(); assert(dx==3 and dy==4)
                local transform=world:get_transform(entity); transform.position=Vec3.new(speed,0,0)
                world:set_transform(entity,transform)
            end
            local result=world:get_raycast_result(ray)
            if result then
                assert(result.entity==world:find_entity("Wall") and math.abs(result.distance-1)<.001)
                local values=world:get_trigger_overlaps(overlaps); assert(#values>=1)
                if not done then world:spawn_entity(); done=true end
            end
        end`
    file:=strings.concatenate({script_dir,"/commands.luau"}); defer delete(file); testing.expect(t,os.write_entire_file(file,source)==nil)
    tone,tone_error:=os.read_entire_file("odin/audio/testdata/tone.wav",context.allocator); testing.expect(t,tone_error==nil); defer delete(tone)
    tone_file:=strings.concatenate({resource_dir,"/tone.wav"}); defer delete(tone_file); testing.expect(t,os.write_entire_file(tone_file,tone)==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_dir),resources.Error.None); testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None); testing.expect_value(t,script_native_init(&owner,LUAU_APP_LIBRARY),editor.Scene_Error.None)
    testing.expect(t,audio_runtime_init(&owner,false)==.None); audio_owner:=ecs.get_resource_mut(&owner.world,Audio_Runtime)
    testing.expect(t,audio_service_register_cue(audio_owner.service,"step",{Audio_Source{"tone.wav",.Resource}})==.None)
    actor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,actor,Scene_Transform{km.TRANSFORM_IDENTITY}); body:=physics_body({kind=.Sphere,radius=.25}); body.gravity_scale=0; ecs.add_component(&owner.world,actor,body); ecs.add_component(&owner.world,actor,Scene_Name{strings.clone("Actor")}); ecs.add_component(&owner.world,actor,Script_Component{path=strings.clone("scripts/commands.luau")})
    wall:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,wall,Scene_Transform{km.transform(position={4,0,0})}); ecs.add_component(&owner.world,wall,Scene_Name{strings.clone("Wall")}); ecs.add_component(&owner.world,wall,physics_body({kind=.Box,half_extents={.5,.5,.5}},.Fixed))
    sensor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,sensor,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,sensor,Scene_Name{strings.clone("Sensor")}); ecs.add_component(&owner.world,sensor,physics_body({kind=.Sphere,radius=2},.Fixed,true)); ecs.add_component(&owner.world,sensor,Trigger_Volume{})
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    values,handle,inspect_error:=script_inspect(&owner,actor); testing.expect(t,inspect_error==.None); script.variables_destroy(values,owner.world.allocator)
    testing.expect(t,script_set_variable(&owner,handle,"speed",f64(1))==.None)
    testing.expect(t,script_input_set(&owner,{actions={"move_forward"},keys={"W"},mouse_delta={3,4},focused=true})==.None)
    testing.expect(t,simulation_step(&owner,.1)==.None)
    component:=ecs.get_component_mut(&owner.world,actor,Script_Component); testing.expect(t,len(component.last_errors)==0 && component.consecutive_errors==0)
    testing.expect(t,ecs.get_component_mut(&owner.world,actor,Scene_Transform).local.position[0]==1)
    input,_:=ecs.get_resource(&owner.world,Script_Input); testing.expect(t,input.mouse_delta==[2]f32{} )
    testing.expect(t,len(audio_owner.service.script_voices)==3)
    block:[1024]f32; audio.engine_render(audio_owner.service.engine,block[:]); testing.expect(t,audio_owner.service.engine.levels.sfx.peak>0)
    logs:=script_logs_drain(&owner); testing.expect(t,len(logs)==1 && strings.has_prefix(logs[0].message,"native\t")); script_logs_destroy(logs,owner.world.allocator)
    testing.expect(t,script_input_set(&owner,{focused=true})==.None)
    before:=ecs.entity_ids(&owner.world); before_count:=len(before); delete(before)
    testing.expect(t,simulation_step(&owner,.1)==.None && len(component.last_errors)==0)
    after:=ecs.entity_ids(&owner.world); testing.expect(t,len(after)==before_count+1); delete(after)
    testing.expect(t,ecs.get_component_mut(&owner.world,actor,Physics_Body).linear_velocity[0]>2)
    native:=ecs.get_resource_mut(&owner.world,Script_Native_Runtime); previous,present:=script.handle(native.runtime,u64(actor)); testing.expect(t,present)
    testing.expect(t,os.write_entire_file(file,"function broken(")==nil)
    testing.expect(t,script_reload(&owner,actor)==.Invalid_Operation)
    testing.expect(t,simulation_step(&owner,.1)==.None)
    old_values,old_handle,old_error:=script_inspect(&owner,actor); testing.expect(t,old_error==.None && old_handle==previous); script.variables_destroy(old_values,owner.world.allocator)
    retained,still_present:=script.handle(native.runtime,u64(actor)); testing.expect(t,still_present && retained==previous)
    testing.expect(t,os.write_entire_file(file,source)==nil && script_reload(&owner,actor)==.None)
    replaced,_:=script.handle(native.runtime,u64(actor)); testing.expect(t,replaced.serial!=previous.serial)
    preserved,reload_error:=script.inspect(native.runtime,replaced); testing.expect(t,reload_error==""); delete(reload_error)
    for value in preserved { if value.name=="speed" { speed,valid:=value.value.(f64); testing.expect(t,valid && speed==1) } }; script.variables_destroy(preserved,owner.world.allocator)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None && len(audio_owner.service.script_voices)==0 && !ecs.entity_exists(&owner.world,actor))
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LUAU_APP_LIBRARY!="" && BOX3D_LIBRARY!="" {
@(test)
test_script_native_commands_queries_audio_variables_and_input :: proc(t:^testing.T) { native_script_commands_queries_audio_variables_and_input(t) }
}
