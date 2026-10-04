#+build darwin, arm64
//! Genuine model worlds prove sampled material images and animated geometry through native pixels.
package main

import app "../../app"
import ecs "../../ecs"
import km "../../math"
import render "../../app/render"
import gfx "../../gfx"
import shader "../../gfx/shader"
import resources "../../resources"
import editor "../../editor"
import "core:os"
import "core:path/filepath"
import "core:mem"
import "core:fmt"
import "core:strings"

model_camera :: proc(batch:^render.Model_Batch)->render.Camera {
    low,high:=km.Vec3{max(f32),max(f32),max(f32)},km.Vec3{-max(f32),-max(f32),-max(f32)}
    for entry in batch.entries {
        for vertex in batch.vertices[entry.first_vertex:entry.first_vertex+entry.vertex_count] {
            world:=km.xyz(km.matrix_vector(batch.objects[entry.object_index].model,vertex.position))
            for axis in 0..<3 { low[axis]=min(low[axis],world[axis]); high[axis]=max(high[axis],world[axis]) }
        }
    }
    center:=(low+high)*0.5; radius:=max(km.length(high-low)*0.5,0.1)
    camera:=render.camera_default(); camera.target=center; camera.position=center+km.normalize(km.Vec3{0.5,0.2,1})*radius*3.6; camera.near=radius*0.001; camera.far=radius*20
    return camera
}
model_pixels :: proc(consumer:^render.Native_Consumer($R),capture:Capture_Ops(R),frame:render.Frame_Data)->gfx.Readback_Data {
    assert(render.native_consumer_refresh(consumer)=={})
    submission,error:=render_frame(consumer.active,frame,consumer.batch.objects,consumer.batch.draws); assert(error=={})
    pixels:=read_pixels(consumer.active,capture,submission); assert(render.native_scene_wait(consumer.active,submission)==.None); return pixels
}
fail_model_sampler :: proc(renderer:^$R,descriptor:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) { return {},.Allocation_Failed }
exercise_models :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),descriptor:render.Scene_Pipelines,compiler:^shader.Compiler,model_operations:render.Model_GPU_Ops(R),backend,output,resource_path:string) {
    compiled,compile_error:=render.model_shader_compile(compiler); assert(compile_error==.None); defer render.model_shader_destroy(&compiled)
    config:=render.Model_Config(R){&compiled,model_operations}
    for path in ([]string{"models/Box.gltf","models/DamagedHelmet.glb","models/Fox.glb","models/Tiger.glb"}) {
        project,project_error:=os.make_directory_temp("","katla-model-native-*",context.allocator); assert(project_error==nil); defer { os.remove_all(project); delete(project) }
        copied_resources:=strings.concatenate({project,"/resources"}); defer delete(copied_resources)
        assert(os.copy_directory_all(copied_resources,resource_path)==nil)
        owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
        assert(app.authoring_services_init(&owner)==.None)
        assert(app.asset_resources_init(&owner,project,copied_resources)==resources.Error.None)
        source,error:=app.scene_model_prepare(&owner,{path=path}); assert(error==.None)
        entity:=ecs.spawn(&owner.world,struct { source:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material,key:app.Scene_Key }{source,{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1},{1}})
        ecs.get_resource_mut(&owner.world,app.Scene_Identity).next_entity_id=2
        consumer:render.Native_Consumer(R)
        native_error:=render.native_consumer_init(&consumer,&owner,renderer,operations,descriptor,3,512,512,models=&config)
        fmt.println("Native model preparation:",backend,path,native_error)
        assert(native_error=={}); defer { assert(render.native_consumer_destroy(&consumer)==.None) }
        assert(consumer.active.models!=nil && len(consumer.active.models.batch.entries)>0)
        camera:=model_camera(&consumer.active.models.batch)
        frame,frame_error:=render.frame_data(camera,512,512,backend=="vulkan"); assert(frame_error==.None)
        submission,render_error:=render_frame(consumer.active,frame,consumer.batch.objects,consumer.batch.draws)
        fmt.println("Native model submission:",render_error); assert(render_error=={})
        before:=read_pixels(consumer.active,capture,submission); defer gfx.readback_data_destroy(&before)
        assert(render.native_scene_wait(consumer.active,submission)==.None)
        non_clear:int
        for i:=0;i<len(before.bytes);i+=4 { if before.bytes[i]!=9 || before.bytes[i+1]!=10 || before.bytes[i+2]!=13 { non_clear+=1 } }
        assert(non_clear>512,"source model produced no real visible geometry")
        filename:=fmt.aprintf("%s-%s.png",output,filepath.base(path)); defer delete(filename); save_pixels(&before,filename)
        args:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"base_color":[0.7,0.4,0.9,1],"metallic":0.7,"roughness":0.2}}`,u64(entity)); defer delete(args)
        action:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Application,tool_name="material",value=transmute([]byte)args},app.authoring_executor(&owner)); assert(action.result.error==.None)
        edited:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&edited); assert(mem.compare(before.bytes,edited.bytes)!=0)
        assert(app.authoring_undo_last(&owner)==.None)
        undone:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&undone); assert(mem.compare(before.bytes,undone.bytes)==0)
        assert(app.authoring_redo_last(&owner)==.None)
        redone:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&redone); assert(mem.compare(edited.bytes,redone.bytes)==0)
        assert(app.authoring_undo_last(&owner)==.None)
        baseline_again:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&baseline_again); assert(mem.compare(before.bytes,baseline_again.bytes)==0)
        saved,save_history:=app.scene_file_execute(&owner,{action=.Save,path="model.katla",has_path=true}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_history); assert(saved.error==.None)
        old_cache:=consumer.active
        primitive_count:=len(consumer.active.models.batch.entries)
        consumer.model_config.operations.create_sampler=fail_model_sampler
        rejected,rejected_history:=app.scene_file_execute(&owner,{action=.Load,path="model.katla",has_path=true}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_history)
        assert(rejected.error!=.None && consumer.last_error.gpu==.Allocation_Failed && consumer.active==old_cache && owner.world.live_count==1 && ecs.entity_exists(&owner.world,entity))
        consumer.model_config.operations.create_sampler=model_operations.create_sampler
        preserved:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&preserved); assert(mem.compare(before.bytes,preserved.bytes)==0)
        loaded,load_history:=app.scene_file_execute(&owner,{action=.Load,path="model.katla",has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_history)
        assert(loaded.error==.None && len(loaded.entities)==1+primitive_count && !ecs.entity_exists(&owner.world,entity) && consumer.active!=old_cache)
        controllers:=0
        for id in loaded.entities { controller,present:=ecs.get_component(&owner.world,id,app.Scene_Model);if present && controller.source.kind==.Group { entity=id;controllers+=1 } }
        assert(controllers==1)
        for child in loaded.entities { if child==entity {continue};selected,has_selected:=ecs.get_component(&owner.world,child,app.Scene_Model);parent,has_parent:=ecs.get_component(&owner.world,child,app.Scene_Parent);assert(has_selected && selected.source.kind==.Primitive && has_parent && parent.entity==entity) }
        restored:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&restored); assert(mem.compare(before.bytes,restored.bytes)==0)
        fmt.println("Native model material shared undo/redo and actual .katla staged load failure/success preserved every pixel:",path)
        if path=="models/Fox.glb" {
            model:=ecs.get_component_mut(&owner.world,entity,app.Scene_Model)
            player:=app.animation_player_stopped(); player.clip=strings.clone(model.model.animation.clips[0].name); player.time=model.model.animation.clips[0].duration*0.45
            ecs.add_component(&owner.world,entity,player)
            old_native:=consumer.active
            assert(render.native_consumer_refresh(&consumer)=={} && consumer.active==old_native,"animation unnecessarily rebuilt native pipelines")
            assert(consumer.active.models.batch.geometry_changed)
            animated,animated_error:=render_frame(consumer.active,frame,consumer.batch.objects,consumer.batch.draws); assert(animated_error=={})
            after:=read_pixels(consumer.active,capture,animated); defer gfx.readback_data_destroy(&after)
            assert(render.native_scene_wait(consumer.active,animated)==.None)
            assert(mem.compare(before.bytes,after.bytes)!=0,"actual animation did not change rendered model pixels")
            animated_path:=fmt.aprintf("%s-Fox-animated.png",output); defer delete(animated_path); save_pixels(&after,animated_path)
            fmt.println("Native Fox skin animation changed actual pixels without pipeline or scene replacement")
        }
        fmt.println("Native source model pixels:",backend,path,"draws",len(consumer.active.models.batch.entries),"sampled images",len(consumer.active.models.textures),"visible pixels",non_clear,filename)
    }
    exercise_model_correctness(renderer,operations,capture,descriptor,&config,backend,output,resource_path)
}
