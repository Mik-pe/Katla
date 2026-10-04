//! Instance and device entry points stay with their loader and native owner.
package katla_vulkan

import vk "vendor:vulkan"
import "core:reflect"
import "core:strings"
import "core:mem"

@(private="package")
Instance_API :: struct {
    GetInstanceProcAddr:vk.ProcGetInstanceProcAddr,
    CreateDebugUtilsMessengerEXT:vk.ProcCreateDebugUtilsMessengerEXT,
    CreateDevice:vk.ProcCreateDevice,
    DestroyDebugUtilsMessengerEXT:vk.ProcDestroyDebugUtilsMessengerEXT,
    DestroyInstance:vk.ProcDestroyInstance,
    EnumerateDeviceExtensionProperties:vk.ProcEnumerateDeviceExtensionProperties,
    EnumeratePhysicalDevices:vk.ProcEnumeratePhysicalDevices,
    GetPhysicalDeviceFeatures2:vk.ProcGetPhysicalDeviceFeatures2,
    GetPhysicalDeviceImageFormatProperties:vk.ProcGetPhysicalDeviceImageFormatProperties,
    GetPhysicalDeviceFormatProperties:vk.ProcGetPhysicalDeviceFormatProperties,
    GetPhysicalDeviceMemoryProperties:vk.ProcGetPhysicalDeviceMemoryProperties,
    GetPhysicalDeviceProperties:vk.ProcGetPhysicalDeviceProperties,
    GetPhysicalDeviceQueueFamilyProperties:vk.ProcGetPhysicalDeviceQueueFamilyProperties,
    DestroySurfaceKHR:vk.ProcDestroySurfaceKHR,
    GetPhysicalDeviceSurfaceSupportKHR:vk.ProcGetPhysicalDeviceSurfaceSupportKHR,
    GetPhysicalDeviceSurfaceCapabilitiesKHR:vk.ProcGetPhysicalDeviceSurfaceCapabilitiesKHR,
    GetPhysicalDeviceSurfaceFormatsKHR:vk.ProcGetPhysicalDeviceSurfaceFormatsKHR,
    GetPhysicalDeviceSurfacePresentModesKHR:vk.ProcGetPhysicalDeviceSurfacePresentModesKHR,
    GetDeviceProcAddr:vk.ProcGetDeviceProcAddr,
}

@(private="package")
load_instance_api :: proc(api:^Instance_API,instance:vk.Instance,get:vk.ProcGetInstanceProcAddr) {
    api.GetInstanceProcAddr=get
    api.DestroySurfaceKHR=cast(vk.ProcDestroySurfaceKHR)get(instance,"vkDestroySurfaceKHR")
    api.GetPhysicalDeviceSurfaceSupportKHR=cast(vk.ProcGetPhysicalDeviceSurfaceSupportKHR)get(instance,"vkGetPhysicalDeviceSurfaceSupportKHR")
    api.GetPhysicalDeviceSurfaceCapabilitiesKHR=cast(vk.ProcGetPhysicalDeviceSurfaceCapabilitiesKHR)get(instance,"vkGetPhysicalDeviceSurfaceCapabilitiesKHR")
    api.GetPhysicalDeviceSurfaceFormatsKHR=cast(vk.ProcGetPhysicalDeviceSurfaceFormatsKHR)get(instance,"vkGetPhysicalDeviceSurfaceFormatsKHR")
    api.GetPhysicalDeviceSurfacePresentModesKHR=cast(vk.ProcGetPhysicalDeviceSurfacePresentModesKHR)get(instance,"vkGetPhysicalDeviceSurfacePresentModesKHR")
    api.CreateDebugUtilsMessengerEXT=cast(vk.ProcCreateDebugUtilsMessengerEXT)get(instance,"vkCreateDebugUtilsMessengerEXT")
    api.CreateDevice=cast(vk.ProcCreateDevice)get(instance,"vkCreateDevice")
    api.DestroyDebugUtilsMessengerEXT=cast(vk.ProcDestroyDebugUtilsMessengerEXT)get(instance,"vkDestroyDebugUtilsMessengerEXT")
    api.DestroyInstance=cast(vk.ProcDestroyInstance)get(instance,"vkDestroyInstance")
    api.EnumerateDeviceExtensionProperties=cast(vk.ProcEnumerateDeviceExtensionProperties)get(instance,"vkEnumerateDeviceExtensionProperties")
    api.EnumeratePhysicalDevices=cast(vk.ProcEnumeratePhysicalDevices)get(instance,"vkEnumeratePhysicalDevices")
    api.GetPhysicalDeviceImageFormatProperties=cast(vk.ProcGetPhysicalDeviceImageFormatProperties)get(instance,"vkGetPhysicalDeviceImageFormatProperties")
    api.GetPhysicalDeviceFormatProperties=cast(vk.ProcGetPhysicalDeviceFormatProperties)get(instance,"vkGetPhysicalDeviceFormatProperties")
    api.GetPhysicalDeviceFeatures2=cast(vk.ProcGetPhysicalDeviceFeatures2)get(instance,"vkGetPhysicalDeviceFeatures2")
    api.GetPhysicalDeviceMemoryProperties=cast(vk.ProcGetPhysicalDeviceMemoryProperties)get(instance,"vkGetPhysicalDeviceMemoryProperties")
    api.GetPhysicalDeviceProperties=cast(vk.ProcGetPhysicalDeviceProperties)get(instance,"vkGetPhysicalDeviceProperties")
    api.GetPhysicalDeviceQueueFamilyProperties=cast(vk.ProcGetPhysicalDeviceQueueFamilyProperties)get(instance,"vkGetPhysicalDeviceQueueFamilyProperties")
    api.GetDeviceProcAddr=cast(vk.ProcGetDeviceProcAddr)get(instance,"vkGetDeviceProcAddr")
}

@(private="package")
load_device_api :: proc(api:^vk.Device_VTable,device:vk.Device,get:vk.ProcGetDeviceProcAddr,allocator:mem.Allocator) {
    names:=reflect.struct_field_names(vk.Device_VTable)
    offsets:=reflect.struct_field_offsets(vk.Device_VTable)
    for name,i in names {
        symbol:=strings.concatenate({"vk",name},allocator)
        c_name:=strings.clone_to_cstring(symbol,allocator)
        location:=cast(^vk.ProcVoidFunction)(cast(uintptr)api+offsets[i])
        location^=get(device,c_name)
        delete(c_name,allocator); delete(symbol,allocator)
    }
}
