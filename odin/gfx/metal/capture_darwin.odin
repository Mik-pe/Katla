#+build darwin, arm64
//! Passive facts are recorded beside the Metal calls that actually encode accepted work.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:sync"

/// Enables optional native observations without changing submission or GPU ownership.
capture_enable :: proc(r:^Renderer,enabled:bool)->gfx.Gpu_Error {
    if r.device==nil || r.failed { return .Native_Failure }
    r.capture.enabled=enabled
    if !enabled { gfx.capture_abandon(&r.capture) }
    return .None
}
/// Copies accepted facts and current completed feedback without waiting or retiring work.
capture_snapshot :: proc(r:^Renderer,submission:u64=0,allocator:=context.allocator)->(gfx.Capture_Snapshot,bool) {
    for &slot in r.slots { if slot.submission!=0 && slot.completion!=nil { capture_completion(r,&slot) } }
    return gfx.capture_snapshot(&r.capture,submission,allocator)
}
@(private="package")
capture_emit :: proc(r:^Renderer,slot:^Native_Frame,event:gfx.Capture_Event) {
    if !r.capture.recording { return }
    value:=event; value.pass_index=slot.capture_pass; value.phase_index=slot.capture_phase;value.encoder=slot.capture_encoder
    if value.binding_stages==0 { value.binding_stages=slot.capture_stages }
    if value.pipeline==0 { value.pipeline=slot.capture_pipeline }; if value.layout==0 { value.layout=slot.capture_layout }
    gfx.capture_record(&r.capture,value)
}
@(private="package")
capture_allocation :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,kind:gfx.Capture_Resource_Kind,index:int,heap:^Native_Heap=nil,label:string="native resource allocation") {
    if !r.capture.recording { return }
    event:=gfx.Capture_Event{kind=.Allocation,resource_kind=kind,resource_index=index,object=gfx.capture_object(&r.capture,object),size=u64(send(NS.UInteger,object,"allocatedSize")),memory_flags=u64(send(NS.UInteger,object,"storageMode")),emitted=true,label=label}
    if heap!=nil {
        _,known:=r.capture.identities[heap.object]
        event.heap=gfx.capture_object(&r.capture,heap.object);event.offset=u64(send(NS.UInteger,object,"heapOffset"))
        if !known { capture_emit(r,slot,{kind=.Allocation,resource_kind=.Auxiliary,resource_index= -1,object=event.heap,size=u64(send(NS.UInteger,heap.object,"size")),memory_flags=u64(send(NS.UInteger,heap.object,"storageMode")),emitted=true,label="observed native placement heap"}) }
    }
    capture_emit(r,slot,event)
}
@(private="package")
capture_residency :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object) {
    send(nil,slot.residency,"addAllocation:",object)
    capture_emit(r,slot,{kind=.Residency,resource_kind=.Auxiliary,resource_index= -1,object=gfx.capture_object(&r.capture,object),table=gfx.capture_object(&r.capture,slot.residency),emitted=true,label="residency addAllocation"})
}
@(private="package")
capture_auxiliary :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,label:string) {
    capture_allocation(r,slot,object,.Auxiliary,-1,nil,label)
}
@(private="package")
capture_encoder_begin :: proc(r:^Renderer,slot:^Native_Frame,encoder:^NS.Object,label:string) {
    slot.capture_encoder=gfx.capture_object(&r.capture,encoder)
    capture_emit(r,slot,{kind=.Encoder_Begin,resource_index= -1,object=slot.capture_encoder,emitted=true,label=label})
}
@(private="package")
capture_encoder_end :: proc(r:^Renderer,slot:^Native_Frame) {
    capture_emit(r,slot,{kind=.Encoder_End,resource_index= -1,object=slot.capture_encoder,emitted=true,label="endEncoding"})
    slot.capture_encoder=0;slot.capture_pipeline=0;slot.capture_layout=0
}
@(private="package")
capture_barrier_event :: proc(pass:int,visibility:NS.UInteger)->gfx.Capture_Event {
    return {kind=.Global_Barrier,pass_index=pass,phase_index= -1,resource_index= -1,source_stages=u64(max(int)),destination_stages=u64(max(int)),native_visibility=u64(visibility),emitted=true,label="Metal queue-stage visibility",reason="global resource and optional heap-alias scope"}
}
@(private="package")
capture_alias_event :: proc(alias:gfx.Alias_Handoff)->gfx.Capture_Event {
    value:=gfx.Capture_Event{kind=.Alias,pass_index=alias.after.index,previous_pass_index=alias.before.index,source_stages=u64(max(int)),destination_stages=u64(max(int)),phase_index= -1,native_visibility=3,emitted=true,label="placement alias handoff",reason="covered by the emitted global heap-visibility barrier"}
    switch next in alias.next {
    case gfx.Resource_Id:value.resource_kind=.Buffer;value.resource_index=next.index
    case gfx.Image_Id:value.resource_kind=.Image;value.resource_index=next.index
    }
    switch previous in alias.previous {
    case gfx.Resource_Id:value.alias_previous_kind=.Buffer;value.alias_previous_index=previous.index
    case gfx.Image_Id:value.alias_previous_kind=.Image;value.alias_previous_index=previous.index
    }
    return value
}
@(private="package")
capture_barrier :: proc(r:^Renderer,slot:^Native_Frame,visibility:NS.UInteger,prepared:^gfx.Prepared_Graph) {
    capture_emit(r,slot,capture_barrier_event(slot.capture_pass,visibility))
    if !r.capture.recording { return }
    for alias in prepared.aliases { if alias.after.index==slot.capture_pass {
        value:=capture_alias_event(alias)
        switch next in alias.next {
        case gfx.Resource_Id: buffer,ok:=resolve_buffer(r,prepared,next);if ok { value.object=gfx.capture_object(&r.capture,buffer.object);if buffer.heap!=nil { value.heap=gfx.capture_object(&r.capture,buffer.heap.object) } }
        case gfx.Image_Id: texture,ok:=resolve_texture(r,prepared,next);if ok { value.object=gfx.capture_object(&r.capture,texture.object);if texture.heap!=nil { value.heap=gfx.capture_object(&r.capture,texture.heap.object) } }
        };capture_emit(r,slot,value)
    } }
}
@(private="package")
capture_table :: proc(r:^Renderer,slot:^Native_Frame,table:^NS.Object,stage:gfx.Shader_Stage) {
    slot.capture_stages=u64(1)<<u32(stage)
    capture_emit(r,slot,{kind=.Argument_Table,resource_index= -1,object=gfx.capture_object(&r.capture,table),table=gfx.capture_object(&r.capture,table),native_index=u32(stage),emitted=true,label="immutable argument table"})
}
@(private="package")
capture_buffer_binding :: proc(r:^Renderer,slot:^Native_Frame,table,object:^NS.Object,access:gfx.Buffer_Access,group,binding,index:u32,path:gfx.Capture_Binding_Path=.Descriptor) {
    capture_emit(r,slot,{kind=.Bind_Buffer,binding_path=path,resource_kind=.Buffer,resource_index=access.resource.index,object=gfx.capture_object(&r.capture,object),table=gfx.capture_object(&r.capture,table),group=group,binding=binding,native_index=index,buffer_range=access.range,emitted=true,label="argument buffer address"})
}
@(private="package")
capture_constant_binding :: proc(r:^Renderer,slot:^Native_Frame,table,object:^NS.Object,group,binding,index:u32,size:u64,label:string) {
    capture_auxiliary(r,slot,object,label)
    capture_emit(r,slot,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,object=gfx.capture_object(&r.capture,object),table=gfx.capture_object(&r.capture,table),group=group,binding=binding,native_index=index,buffer_range={0,size},emitted=true,label=label})
}
@(private="package")
capture_image_binding :: proc(r:^Renderer,slot:^Native_Frame,table,object:^NS.Object,access:gfx.Image_Access,group,binding,index,array_index:u32) {
    capture_emit(r,slot,{kind=.Bind_Image,resource_kind=.Image,resource_index=access.resource.index,object=gfx.capture_object(&r.capture,object),table=gfx.capture_object(&r.capture,table),group=group,binding=binding,native_index=index,array_index=array_index,image_range=access.range,emitted=true,label="argument texture resource"})
}
@(private="package")
capture_sampler_binding :: proc(r:^Renderer,slot:^Native_Frame,table,object:^NS.Object,group,binding,index:u32) {
    capture_emit(r,slot,{kind=.Bind_Sampler,resource_index= -1,object=gfx.capture_object(&r.capture,object),table=gfx.capture_object(&r.capture,table),group=group,binding=binding,native_index=index,emitted=true,label="argument sampler resource"})
}

@(private="package")
capture_completion :: proc(r:^Renderer,slot:^Native_Frame) {
    sync.mutex_lock(&slot.completion.mutex)
    done,failed:=slot.completion.done,slot.completion.failed
    sync.mutex_unlock(&slot.completion.mutex)
    if done { gfx.capture_feedback(&r.capture,slot.submission,.Failed if failed else .Completed) }
}

@(private="package")
capture_attachment :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,access:gfx.Image_Access,load:gfx.Load_Op,store:gfx.Store_Op,index:u32,color:[4]f64,depth:f64,stencil:u32,label:string) {
    if !r.capture.recording { return }
    texture,ok:=resolve_texture(r,prepared,access.resource);if !ok { return }
    clear_color,clear_depth,clear_stencil:=color,depth,stencil
    if load!=.Clear { clear_color={};clear_depth=0;clear_stencil=0 }
    capture_emit(r,slot,{kind=.Attachment,resource_kind=.Image,resource_index=access.resource.index,object=gfx.capture_object(&r.capture,texture.object),native_index=index,image_range=access.range,native_load=u64(load_op(load)),native_store=u64(1 if store==.Store else 0),clear_color=clear_color,clear_depth=clear_depth,clear_stencil=clear_stencil,emitted=true,label=label})
}

@(private="package")
capture_direct_binding :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,access:gfx.Buffer_Access,path:gfx.Capture_Binding_Path,index:u32,stages:u64) {
    capture_emit(r,slot,{kind=.Bind_Buffer,binding_path=path,resource_kind=.Buffer,resource_index=access.resource.index,object=gfx.capture_object(&r.capture,object),buffer_range=access.range,native_index=index,binding_stages=stages,emitted=true,label="native direct command buffer"})
}
