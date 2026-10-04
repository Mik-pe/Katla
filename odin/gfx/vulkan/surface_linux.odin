#+build linux
//! Linux descriptors distinguish borrowed Xlib Window/Display and Wayland surface/display handles.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
native_surface_extensions :: proc()->[3]cstring { return {"VK_KHR_surface","VK_KHR_xlib_surface","VK_KHR_wayland_surface"} }
@(private="package")
native_surface_create :: proc(r:^Renderer,desc:gfx.Surface_Desc)->(vk.SurfaceKHR,rawptr,gfx.Gpu_Error) {
    if desc.view==nil || desc.display==nil { return 0,nil,.Invalid_Resource }
    surface:vk.SurfaceKHR
    if desc.kind==.Wayland {
        create:=cast(vk.ProcCreateWaylandSurfaceKHR)r.instance_api.GetInstanceProcAddr(r.instance,"vkCreateWaylandSurfaceKHR")
        if create==nil { return 0,nil,.Unsupported }
        info:=vk.WaylandSurfaceCreateInfoKHR{sType=.WAYLAND_SURFACE_CREATE_INFO_KHR,display=cast(^vk.wl_display)desc.display,surface=cast(^vk.wl_surface)desc.view}
        if create(r.instance,&info,nil,&surface)!=.SUCCESS { return 0,nil,.Native_Failure }
    } else if desc.kind==.Native || desc.kind==.Xlib {
        create:=cast(vk.ProcCreateXlibSurfaceKHR)r.instance_api.GetInstanceProcAddr(r.instance,"vkCreateXlibSurfaceKHR")
        if create==nil { return 0,nil,.Unsupported }
        info:=vk.XlibSurfaceCreateInfoKHR{sType=.XLIB_SURFACE_CREATE_INFO_KHR,dpy=cast(^vk.XlibDisplay)desc.display,window=vk.XlibWindow(uintptr(desc.view))}
        if create(r.instance,&info,nil,&surface)!=.SUCCESS { return 0,nil,.Native_Failure }
    } else { return 0,nil,.Unsupported }
    return surface,nil,.None
}
