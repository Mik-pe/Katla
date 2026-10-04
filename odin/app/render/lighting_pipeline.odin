//! Lighting ABI validation covers the selected fragment's complete scene inputs.
package render

import shader "../../gfx/shader"

LIGHTING_COMMON :: #load("shaders/lighting_common.wgsl",string)
lighting_binding_valid :: proc(binding:shader.Binding)->bool {
    if binding.array_count!=1 || binding.access!=.Read { return false }
    if binding.group==3 {
        if binding.kind!=.Buffer || binding.binding>3 { return false }
        expected:=u64(160) if binding.binding==0 else (u64(8192) if binding.binding==1 else u64(4))
        return binding.minimum_size==expected && binding.uniform==(binding.binding==0)
    }
    if binding.group==4 {
        switch binding.binding {
        case 0: return binding.kind==.Buffer && binding.minimum_size==352 && !binding.uniform
        case 1: return binding.kind==.Texture && binding.depth && !binding.arrayed && binding.dimension==.D2
        case 2: return binding.kind==.Sampler && binding.comparison
        }
    }
    return false
}
