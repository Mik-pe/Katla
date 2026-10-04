#+test
package main

import gpu "../gfx/vulkan"
import "core:testing"

@(test)
test_array_capability_gate_checks_feature_and_both_descriptor_limits :: proc(t:^testing.T) {
    renderer:gpu.Renderer
    renderer.device=cast(type_of(renderer.device))cast(rawptr)uintptr(1)
    renderer.sampled_arrays=true;renderer.storage_arrays=true
    renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindSampledImages=4096
    renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindSampledImages=4096
    renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindStorageImages=2
    renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindStorageImages=2
    result:=array_capabilities(&renderer);testing.expect(t,result.baseline && result.supported && result.native_error==.None)
    renderer.sampled_arrays=false;result=array_capabilities(&renderer)
    testing.expect(t,result.baseline && !result.sampled_array_4096 && result.storage_array_2 && result.native_error==.Unsupported)
    renderer.sampled_arrays=true;renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindSampledImages=4095
    testing.expect(t,!array_capabilities(&renderer).sampled_array_4096)
    renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindSampledImages=4096
    renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindSampledImages=4095
    testing.expect(t,!array_capabilities(&renderer).sampled_array_4096)
    renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindSampledImages=4096
    renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindStorageImages=1
    testing.expect(t,!array_capabilities(&renderer).storage_array_2)
    renderer.descriptor_limits.maxPerStageDescriptorUpdateAfterBindStorageImages=2
    renderer.descriptor_limits.maxDescriptorSetUpdateAfterBindStorageImages=1
    testing.expect(t,!array_capabilities(&renderer).storage_array_2)
    renderer.device=nil;testing.expect(t,!array_capabilities(&renderer).baseline)
}
