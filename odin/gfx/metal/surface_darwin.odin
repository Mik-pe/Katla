#+build darwin, arm64
//! Main-thread Metal drawable ownership, independent of application window policy.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import QC "vendor:darwin/QuartzCore"
import NS "core:sys/darwin/Foundation"
import CF "core:sys/darwin/CoreFoundation"
import "core:sync"

@(require)
foreign import "system:QuartzCore.framework"

@(private="package")
Surface :: struct {
    view:^NS.Object,
    layer:^QC.MetalLayer,
    drawable:^QC.MetalDrawable,
    width,height:u32,
    generation:u64,
    submitted:gfx.Submission,
}

@(private="package")
surface_main_thread :: proc()->bool {
    cls:=NS.objc_lookUpClass("NSThread")
    return cls!=nil && bool(send(NS.BOOL,cast(^NS.Object)cls,"isMainThread"))
}

/// Attaches a three-drawable layer to the application's main-thread NSView.
attach_surface :: proc(r:^Renderer,desc:gfx.Surface_Desc)->gfx.Gpu_Error {
    if !surface_main_thread() { return .Invalid_Resource }
    ns_view,width,height:=desc.view,desc.width,desc.height
    if r.device==nil || r.failed { return .Native_Failure }
    if ns_view==nil || r.surface.layer!=nil || width==0 || height==0 { return .Invalid_Range }
    view:=cast(^NS.Object)ns_view
    layer:=cast(^QC.MetalLayer)new_object("CAMetalLayer")
    if layer==nil { return .Allocation_Failed }
    layer->setDevice(r.device)
    layer->setPixelFormat(.BGRA8Unorm)
    layer->setMaximumDrawableCount(NS.UInteger(len(r.slots)))
    layer->setFramebufferOnly(false)
    layer->setDisplaySyncEnabled(true)
    layer->setDrawableSize(NS.Size{CF.CGFloat(width),CF.CGFloat(height)})
    send(nil,layer,"setContentsScale:",send(f64,view,"backingScaleFactor"))
    send(nil,view,"setWantsLayer:",NS.BOOL(true))
    send(nil,view,"setLayer:",layer)
    view->retain()
    r.surface={view=view,layer=layer,width=width,height=height}
    return .None
}

/// Resizes in physical pixels; an outstanding acquisition must first be submitted or discarded.
resize_surface :: proc(r:^Renderer,width,height:u32)->gfx.Gpu_Error {
    if !surface_main_thread() { return .Invalid_Resource }
    if r.surface.layer==nil { return .Invalid_Resource }
    if r.surface.drawable!=nil { return .Busy }
    r.surface.width=width; r.surface.height=height
    r.surface.layer->setDrawableSize(NS.Size{CF.CGFloat(width),CF.CGFloat(height)})
    return .None
}

/// Releases the layer before the owning application's NSView/window is destroyed.
detach_surface :: proc(r:^Renderer)->gfx.Gpu_Error {
    if r.surface.layer!=nil && !surface_main_thread() { return .Invalid_Resource }
    outcome:=gfx.Gpu_Error.None
    if r.surface.submitted.id!=0 {
        slot,pending:=submission_slot(r,r.surface.submitted)
        if pending { sync.wait_group_wait(&slot.completion.wait_group); outcome=retire_slot(r,slot) }
    }
    if r.surface.drawable!=nil {
        surface_discard(&r.surface)
        if r.surface_texture.owner!=nil { destroy_texture(r,r.surface_texture); r.surface_texture={} }
    }
    if r.surface.view!=nil {
        if send(^NS.Object,r.surface.view,"layer")==cast(^NS.Object)r.surface.layer {
            send(nil,r.surface.view,"setLayer:",cast(^NS.Object)nil)
        }
        r.surface.view->release()
    }
    if r.surface.layer!=nil { r.surface.layer->release() }
    r.surface={}
    return outcome
}

@(private="package")
surface_discard :: proc(s:^Surface) {
    if s.drawable!=nil { s.drawable->release(); s.drawable=nil }
}

@(private="package")
surface_acquire_native :: proc(s:^Surface)->(^MTL.Texture,gfx.Gpu_Error) {
    if s.layer==nil { return nil,.Invalid_Resource }
    if s.drawable!=nil { return nil,.Busy }
    if s.width==0 || s.height==0 { return nil,.Busy }
    drawable:=s.layer->nextDrawable()
    if drawable==nil { return nil,.Busy }
    drawable->retain(); s.drawable=drawable
    return drawable->texture(),.None
}

/// Acquires one drawable generation with a backend-neutral texture identity.
acquire_surface :: proc(r:^Renderer)->(gfx.Surface_Frame,gfx.Surface_Result,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Fatal,.Native_Failure }
    if r.surface.layer==nil { return {},.Fatal,.Invalid_Resource }
    if r.frames.slots[r.next_slot].state!=.Acquired { return {},.Fatal,.Invalid_Resource }
    if r.surface.drawable!=nil { return {},.Unavailable,.Busy }
    if r.surface.width==0 || r.surface.height==0 { return {},.Unavailable,.None }
    if r.surface.generation==max(u64) { return {},.Fatal,.Native_Failure }
    object,err:=surface_acquire_native(&r.surface)
    if err==.Busy { return {},.Unavailable,.None }
    if err!=.None { return {},.Fatal,err }
    object->retain()
    texture:=new(Native_Texture,r.allocator)
    texture^={object=object,desc={r.surface.width,r.surface.height,1,1,.BGRA8_Unorm,{.Color_Attachment,.Transfer_Source,.Present},1},refs=1}
    new_texture_content(r,texture)
    r.surface_texture=gfx.storage_insert(&r.textures,texture)
    r.surface.generation+=1; r.surface.submitted={}
    return {r,r.surface.generation,r.surface_texture,r.surface.width,r.surface.height},.Presented,.None
}

@(private="package")
surface_frame_valid :: proc(r:^Renderer,frame:gfx.Surface_Frame)->bool {
    return frame.owner==r && frame.generation==r.surface.generation && frame.texture==r.surface_texture && r.surface.drawable!=nil
}

/// Abandons an unsubmitted acquisition without publishing a surface result.
abort_surface :: proc(r:^Renderer,frame:gfx.Surface_Frame)->gfx.Gpu_Error {
    if !surface_frame_valid(r,frame) { return .Invalid_Resource }
    if r.surface.submitted.id!=0 { return .Busy }
    surface_discard(&r.surface)
    err:=destroy_texture(r,r.surface_texture); r.surface_texture={}
    return err
}

/// Presents only the accepted submission that actually consumed this acquisition.
present_surface :: proc(r:^Renderer,frame:gfx.Surface_Frame,submission:gfx.Submission)->(gfx.Present_Outcome,gfx.Gpu_Error) {
    if submission.owner!=r || submission.id==0 { return {},.Invalid_Resource }
    if !surface_frame_valid(r,frame) || r.surface.submitted!=submission { return {submission,.Fatal},.None }
    send(nil,r.queue,"signalDrawable:",r.surface.drawable)
    r.surface.drawable->present()
    surface_discard(&r.surface)
    destroy_texture(r,r.surface_texture); r.surface_texture={}; r.surface.submitted={}
    return {submission,.Presented},.None
}
