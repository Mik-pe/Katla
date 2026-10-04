#+build linux
//! Xlib descriptors carry a Window integer as view and the borrowed Display pointer.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
native_surface_extensions :: proc()->[2]cstring { return {"VK_KHR_surface","VK_KHR_xlib_surface"} }
@(private="package")
native_surface_create :: proc(r:^Renderer,desc:gfx.Surface_Desc)->(vk.SurfaceKHR,rawptr,gfx.Gpu_Error) {
    if desc.view==nil || desc.display==nil { return 0,nil,.Invalid_Resource }
    create:=cast(vk.ProcCreateXlibSurfaceKHR)r.instance_api.GetInstanceProcAddr(r.instance,"vkCreateXlibSurfaceKHR")
    if create==nil { return 0,nil,.Unsupported }
    info:=vk.XlibSurfaceCreateInfoKHR{sType=.XLIB_SURFACE_CREATE_INFO_KHR,dpy=cast(^vk.XlibDisplay)desc.display,window=vk.XlibWindow(uintptr(desc.view))}
    surface:vk.SurfaceKHR
    if create(r.instance,&info,nil,&surface)!=.SUCCESS { return 0,nil,.Native_Failure }
    return surface,nil,.None
}
