//! Capability receipts gate only optional descriptor-indexing breadth; baseline GPU failures remain failures.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:encoding/json"
import "core:fmt"

Array_Capabilities :: struct {
    baseline,sampled_array_4096,storage_array_2,supported:bool,
    native_error:gfx.Gpu_Error,
    sampled_nonuniform_update_after_bind,storage_nonuniform_update_after_bind:bool,
    sampled_per_stage,sampled_per_set,storage_per_stage,storage_per_set:u32,
}
array_capabilities :: proc(renderer:^gpu.Renderer)->Array_Capabilities {
    result:=Array_Capabilities{
        baseline=renderer.device!=nil,
        sampled_nonuniform_update_after_bind=renderer.sampled_arrays,
        storage_nonuniform_update_after_bind=renderer.storage_arrays,
        sampled_per_stage=renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindSampledImages,
        sampled_per_set=renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindSampledImages,
        storage_per_stage=renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindStorageImages,
        storage_per_set=renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindStorageImages,
    }
    result.sampled_array_4096=result.baseline && renderer.sampled_arrays && result.sampled_per_stage>=4096 && result.sampled_per_set>=4096
    result.storage_array_2=result.baseline && renderer.storage_arrays && result.storage_per_stage>=2 && result.storage_per_set>=2
    result.supported=result.sampled_array_4096 && result.storage_array_2
    result.native_error=.None if result.supported else .Unsupported
    return result
}
print_array_capabilities :: proc(capabilities:Array_Capabilities) {
    bytes,error:=json.marshal(capabilities,opt={spec=.JSON,use_enum_names=true,sort_maps_by_key=true});assert(error==nil);defer delete(bytes)
    fmt.println(string(bytes))
}
probe_array_capabilities :: proc(loader:string)->int {
    renderer:gpu.Renderer
    error:=gpu.renderer_init(&renderer,validation=true,loader_path=loader)
    if error!=.None { print_array_capabilities({native_error=error});return 1 }
    defer assert(gpu.renderer_destroy(&renderer)==.None)
    result:=array_capabilities(&renderer)
    if gpu.validation_error_count(&renderer)!=0 { result.supported=false;result.native_error=.Native_Failure;print_array_capabilities(result);return 1 }
    print_array_capabilities(result)
    return 0 if result.supported else 77
}
