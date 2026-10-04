#+build windows
//! Win32 surface descriptors carry an HWND view and HINSTANCE display.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
native_surface_extensions :: proc()->[2]cstring { return {"VK_KHR_surface","VK_KHR_win32_surface"} }
@(private="package")
native_surface_create :: proc(r:^Renderer,desc:gfx.Surface_Desc)->(vk.SurfaceKHR,rawptr,gfx.Gpu_Error) {
    if desc.view==nil || desc.display==nil { return 0,nil,.Invalid_Resource }
    create:=cast(vk.ProcCreateWin32SurfaceKHR)r.instance_api.GetInstanceProcAddr(r.instance,"vkCreateWin32SurfaceKHR")
    if create==nil { return 0,nil,.Unsupported }
    info:=vk.Win32SurfaceCreateInfoKHR{sType=.WIN32_SURFACE_CREATE_INFO_KHR,hinstance=vk.HINSTANCE(desc.display),hwnd=vk.HWND(desc.view)}
    surface:vk.SurfaceKHR
    if create(r.instance,&info,nil,&surface)!=.SUCCESS { return 0,nil,.Native_Failure }
    return surface,nil,.None
}
