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
    descriptor:gfx.Graphics_Desc,
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
    batch_error:Batch_Error
    if mode==.Replace { token.batch^,batch_error=scene_batch_prepare_entities(owner,entities,consumer.allocator) }
    else { token.batch^,batch_error=scene_batch_prepare(owner,consumer.allocator) }
    if batch_error!={} { consumer.last_error={scene=.Invalid_Geometry}; if batch_error.scene!=.None { return nil,batch_error.scene }; return nil,.Invalid_Operation }
    error:=native_scene_init(token.scene,consumer.renderer,consumer.operations,consumer.descriptor,&token.batch.geometry,len(token.batch.objects),consumer.slots,consumer.width,consumer.height,consumer.allocator)
    if error!={} { consumer.last_error=error; return nil,.Invalid_Operation }
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
native_consumer_init :: proc(consumer:^Native_Consumer($R),owner:^app.Authoring,renderer:^R,operations:GPU_Ops(R),descriptor:gfx.Graphics_Desc,slots:int,width,height:u32,allocator:mem.Allocator=context.allocator)->Native_Error {
    if consumer.authoring!=nil || owner==nil || renderer==nil { return {scene=.Invalid_Geometry} }
    if _,installed:=ecs.get_resource(&owner.world,app.Scene_Participant); installed { return {scene=.Invalid_Geometry} }
    consumer^={authoring=owner,renderer=renderer,operations=operations,descriptor=descriptor,slots=slots,width=width,height=height,allocator=allocator}
    token,error:=consumer_prepare(consumer,owner,nil,.Insert)
    if error!=.None { result:=consumer.last_error; consumer^={}; return result }
    consumer_finish(consumer,cast(^Consumer_Preparation(R))token,true)
    ecs.insert_resource(&owner.world,app.Scene_Participant{consumer,consumer_prepare_callback(R),consumer_finish_callback(R)})
    return {}
}
/// Refreshes placement/material or rebuilds native storage after actual mesh revision changes.
native_consumer_refresh :: proc(consumer:^Native_Consumer($R))->Native_Error {
    if consumer.authoring==nil || consumer.batch==nil { return {scene=.Invalid_Geometry} }
    batch_error:=scene_batch_refresh(consumer.batch,consumer.authoring)
    if batch_error.kind==.Rebuild_Required {
        token,error:=consumer_prepare(consumer,consumer.authoring,nil,.Insert)
        if error!=.None { return consumer.last_error }
        consumer_finish(consumer,cast(^Consumer_Preparation(R))token,true)
    } else if batch_error!={} { return {scene=.Invalid_Geometry} }
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
