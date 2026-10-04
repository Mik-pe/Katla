//! Genuine native mesh preparation participates in authored scene commit and rollback.
package render

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import "core:mem"
import "core:log"

/// Exclusive application owner of a stationary native scene and its current CPU upload revision.
Native_Consumer :: struct($R:typeid) {
    authoring:^app.Authoring,
    renderer:^R,
    operations:GPU_Ops(R),
    descriptor:Scene_Pipelines,
    composition:Scene_Composition(R),
    model_config:Model_Config(R),
    active:^Native_Scene(R),
    batch:^Scene_Batch,
    slots:int,
    width,height:u32,
    last_error:Native_Error,
    allocator:mem.Allocator,
}
@(private="package")
consumer_cleanup_error :: proc(error:gfx.Gpu_Error) { if error!=.None { log.error("Native staged scene cleanup failed",error) } }
@(private="package")
Consumer_Preparation :: struct($R:typeid) { scene:^Native_Scene(R), batch:^Scene_Batch, allocator:mem.Allocator }
@(private="package")
consumer_prepare :: proc(consumer:^Native_Consumer($R),owner:^app.Authoring,entities:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    if consumer.authoring!=owner { return nil,.Invalid_Operation }
    token:=new(Consumer_Preparation(R),consumer.allocator); token.allocator=consumer.allocator
    token.batch=new(Scene_Batch,consumer.allocator); token.scene=new(Native_Scene(R),consumer.allocator)
    success:=false; defer { if !success { consumer_preparation_destroy(token) } }
    all:=ecs.entity_ids(&owner.world); defer delete(all)
    selected:=all[:]
    remaining:=make([dynamic]ecs.Entity_Id,0,len(all),consumer.allocator); defer delete(remaining)
    switch mode {
    case .Replace: selected=entities
    case .Insert:
    case .Remove:
        for id,i in entities {
            if !ecs.entity_exists(&owner.world,id) { return nil,.Entity_Not_Found }
            for previous in entities[:i] { if previous==id { return nil,.Invalid_Operation } }
        }
        for id in all { excluded:=false; for removed in entities { if id==removed { excluded=true; break } }; if !excluded { append(&remaining,id) } }
        selected=remaining[:]
    }
    _,light_error:=lighting_collect_entities(owner,selected)
    if light_error!=.None { consumer.last_error={scene=light_error}; return nil,.Invalid_Operation }
    if consumer.composition.validate!=nil {
        error:=consumer.composition.validate(consumer.composition.state,owner,selected)
        if error!={} { consumer.last_error=error; return nil,.Invalid_Operation }
    }
    mesh_ids:=make([dynamic]ecs.Entity_Id,0,len(selected),consumer.allocator); defer delete(mesh_ids)
    for id in selected {
        if _,model:=ecs.get_component(&owner.world,id,app.Scene_Model); model {
            if _,hidden:=ecs.get_component(&owner.world,id,app.Editor_Hidden); !hidden && consumer.model_config.shader==nil { consumer.last_error={scene=.Invalid_Geometry}; return nil,.Invalid_Operation }
        } else { append(&mesh_ids,id) }
    }
    batch_error:Batch_Error
    token.batch^,batch_error=scene_batch_prepare_entities(owner,mesh_ids[:],consumer.allocator)
    token.batch.models_supported=consumer.model_config.shader!=nil
    if batch_error!={} { consumer.last_error={scene=.Invalid_Geometry}; if batch_error.scene!=.None { return nil,batch_error.scene }; return nil,.Invalid_Operation }
    settings:=FEATURE_SETTINGS_DEFAULT
    if consumer.active!=nil { settings=consumer.active.feature_settings }
    error:=native_scene_init(token.scene,consumer.renderer,consumer.operations,consumer.descriptor,&token.batch.geometry,len(token.batch.objects),consumer.slots,consumer.width,consumer.height,consumer.allocator,settings)
    if error!={} { consumer.last_error=error; return nil,.Invalid_Operation }
    token.scene.composition=consumer.composition; token.scene.authoring=owner; token.scene.batch=token.batch
    if consumer.active!=nil {
        retained:=make([dynamic]ecs.Entity_Id,consumer.allocator); defer delete(retained)
        for id in consumer.active.selected { for candidate in selected { if id==candidate { append(&retained,id); break } } }
        selection_error:=native_scene_select(token.scene,retained[:]); if selection_error!={} { consumer.last_error=selection_error; return nil,.Invalid_Operation }
    }
    if consumer.model_config.shader!=nil {
        token.scene.models=new(Native_Model(R),consumer.allocator)
        model_error:=model_native_init(token.scene.models,owner,selected,consumer.renderer,consumer.model_config,consumer.slots,consumer.allocator)
        if model_error=={} { model_error=model_native_bind(token.scene.models,&token.scene.graph) }
        if model_error!={} { consumer.last_error=model_error; return nil,.Invalid_Operation }
    }
    consumer.last_error={}; success=true; return token,.None
}
@(private="package")
consumer_preparation_destroy :: proc(token:^Consumer_Preparation($R)) {
    if token==nil { return }
    allocator:=token.allocator
    if token.scene!=nil { error:=native_scene_destroy(token.scene); consumer_cleanup_error(error); free(token.scene,allocator) }
    if token.batch!=nil { scene_batch_destroy(token.batch); free(token.batch,allocator) }
    free(token,allocator)
}
@(private="package")
consumer_finish :: proc(consumer:^Native_Consumer($R),token:^Consumer_Preparation(R),commit:bool) {
    if token==nil { return }
    if commit {
        old_scene,old_batch:=consumer.active,consumer.batch
        consumer.active,consumer.batch=token.scene,token.batch
        if consumer.active.models!=nil { consumer.active.models.batch.all_entities=true }
        token.scene,token.batch=old_scene,old_batch
    }
    consumer_preparation_destroy(token)
}
@(private="package")
consumer_prepare_callback :: proc($R:typeid)->proc(rawptr,^app.Authoring,[]ecs.Entity_Id,app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    return proc(state:rawptr,owner:^app.Authoring,entities:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
        return consumer_prepare(cast(^Native_Consumer(R))state,owner,entities,mode)
    }
}
@(private="package")
consumer_finish_callback :: proc($R:typeid)->proc(rawptr,rawptr,bool) {
    return proc(state,token:rawptr,commit:bool) { consumer_finish(cast(^Native_Consumer(R))state,cast(^Consumer_Preparation(R))token,commit) }
}
/// Prepares the existing world before installing genuine GPU staging on the stationary owner.
native_consumer_init :: proc(consumer:^Native_Consumer($R),owner:^app.Authoring,renderer:^R,operations:GPU_Ops(R),descriptor:Scene_Pipelines,slots:int,width,height:u32,allocator:mem.Allocator=context.allocator,models:^Model_Config(R)=nil,install_participant:bool=true)->Native_Error {
    if consumer.authoring!=nil || owner==nil || renderer==nil { return {scene=.Invalid_Geometry} }
    if install_participant { if _,installed:=ecs.get_resource(&owner.world,app.Scene_Participant); installed { return {scene=.Invalid_Geometry} } }
    consumer^={authoring=owner,renderer=renderer,operations=operations,descriptor=descriptor,slots=slots,width=width,height=height,allocator=allocator}
    if models!=nil { consumer.model_config=models^ }
    token,error:=consumer_prepare(consumer,owner,nil,.Insert)
    if error!=.None { result:=consumer.last_error; consumer^={}; return result }
    consumer_finish(consumer,cast(^Consumer_Preparation(R))token,true)
    if install_participant { ecs.insert_resource(&owner.world,app.Scene_Participant{consumer,consumer_prepare_callback(R),consumer_finish_callback(R)}) }
    return {}
}
/// Refreshes placement/material or rebuilds native storage after actual mesh revision changes.
native_consumer_refresh :: proc(consumer:^Native_Consumer($R))->Native_Error {
    if consumer.authoring==nil || consumer.batch==nil { return {scene=.Invalid_Geometry} }
    batch_error:=scene_batch_refresh(consumer.batch,consumer.authoring)
    model_error:Model_Batch_Error
    if consumer.active.models!=nil { model_error=model_batch_refresh(&consumer.active.models.batch,consumer.authoring) }
    if batch_error.kind==.Rebuild_Required || model_error.kind==.Rebuild_Required {
        token,error:=consumer_prepare(consumer,consumer.authoring,nil,.Insert)
        if error!=.None { return consumer.last_error }
        consumer_finish(consumer,cast(^Consumer_Preparation(R))token,true)
    } else if batch_error!={} || model_error.kind!=.None {
        log.error("Native scene refresh rejected current geometry",batch_error,model_error)
        return {scene=.Invalid_Geometry}
    }
    return {}
}
/// Installs one application composition before its first accepted scene work.
native_consumer_compose :: proc(consumer:^Native_Consumer($R),composition:Scene_Composition(R))->Native_Error {
    if consumer.active==nil || consumer.composition.prepare!=nil || len(consumer.active.pending)!=0 || composition.state==nil || composition.prepare==nil || composition.accepted==nil || composition.aborted==nil { return {scene=.Invalid_Geometry} }
    if composition.validate!=nil { ids:=ecs.entity_ids(&consumer.authoring.world); defer delete(ids); error:=composition.validate(composition.state,consumer.authoring,ids[:]); if error!={} { return error } }
    consumer.composition=composition; consumer.active.composition=composition
    return {}
}
/// Applies actual backing extent before staging later asset publications.
native_consumer_resize :: proc(consumer:^Native_Consumer($R),width,height:u32)->Native_Error {
    if consumer.active==nil { return {scene=.Invalid_Geometry} }
    error:=native_scene_resize(consumer.active,width,height)
    if error=={} { consumer.width,consumer.height=width,height }
    return error
}
/// Detaches the participant before releasing graphics, while the CPU authoring owner is still alive.
native_consumer_destroy :: proc(consumer:^Native_Consumer($R))->gfx.Gpu_Error {
    if consumer.authoring!=nil {
        participant,installed:=ecs.get_resource(&consumer.authoring.world,app.Scene_Participant)
        if installed && participant.state==consumer { ecs.remove_resource(&consumer.authoring.world,app.Scene_Participant) }
    }
    error:=gfx.Gpu_Error.None
    if consumer.active!=nil { error=native_scene_destroy(consumer.active); free(consumer.active,consumer.allocator) }
    if consumer.batch!=nil { scene_batch_destroy(consumer.batch); free(consumer.batch,consumer.allocator) }
    consumer^={}; return error
}

/// Prepares a genuine candidate without installing a participant or publishing its resources.
native_consumer_prepare :: proc(consumer:^Native_Consumer($R),owner:^app.Authoring,entities:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { return consumer_prepare(consumer,owner,entities,mode) }
/// Consumes the candidate after the host's entire participant set accepts or rejects the transaction.
native_consumer_finish :: proc(consumer:^Native_Consumer($R),token:rawptr,commit:bool) { consumer_finish(consumer,cast(^Consumer_Preparation(R))token,commit) }
