#+build darwin
//! Cocoa views host a backend-owned Metal layer for Vulkan presentation.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import NS "core:sys/darwin/Foundation"
import "base:intrinsics"

@(private="package")
native_surface_extensions :: proc()->[2]cstring { return {"VK_KHR_surface","VK_EXT_metal_surface"} }
@(private="package")
native_surface_create :: proc(r:^Renderer,desc:gfx.Surface_Desc)->(vk.SurfaceKHR,rawptr,gfx.Gpu_Error) {
    if desc.view==nil { return 0,nil,.Invalid_Resource }
    layer_class:=NS.objc_lookUpClass("CAMetalLayer")
    if layer_class==nil { return 0,nil,.Unsupported }
    layer:=intrinsics.objc_send(^NS.Object,cast(^NS.Object)layer_class,"new")
    if layer==nil { return 0,nil,.Allocation_Failed }
    create:=cast(vk.ProcCreateMetalSurfaceEXT)r.instance_api.GetInstanceProcAddr(r.instance,"vkCreateMetalSurfaceEXT")
    if create==nil { layer->release(); return 0,nil,.Unsupported }
    info:=vk.MetalSurfaceCreateInfoEXT{sType=.METAL_SURFACE_CREATE_INFO_EXT,pLayer=cast(^vk.CAMetalLayer)layer}
    surface:vk.SurfaceKHR
    if create(r.instance,&info,nil,&surface)!=.SUCCESS { layer->release(); return 0,nil,.Native_Failure }
    view:=cast(^NS.Object)desc.view
    intrinsics.objc_send(nil,view,"setWantsLayer:",bool(true))
    intrinsics.objc_send(nil,view,"setLayer:",layer)
    window:=intrinsics.objc_send(^NS.Object,view,"window")
    scale:=NS.Float(1)
    if window!=nil { scale=intrinsics.objc_send(NS.Float,window,"backingScaleFactor") }
    intrinsics.objc_send(nil,layer,"setContentsScale:",scale)
    size:=NS.Size{NS.Float(desc.width),NS.Float(desc.height)}
    intrinsics.objc_send(nil,layer,"setDrawableSize:",size)
    return surface,layer,.None
}
@(private="package")
native_surface_owner_release :: proc(owner:rawptr) { if owner!=nil { layer:=cast(^NS.Object)owner; layer->release() } }
