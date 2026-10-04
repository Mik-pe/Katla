//! Prepared phases own effective constants and preserve ordered geometry without feature dispatch.
package gfx

import "core:mem"

@(private="package")
constants_destroy :: proc(constants:[]Constant_Binding,allocator:mem.Allocator) { for constant in constants { delete(constant.bytes,allocator) }; delete(constants,allocator) }
@(private="package")
constants_clone :: proc(constants:[]Constant_Binding,allocator:mem.Allocator)->[]Constant_Binding {
    result:=clone_slice(constants,allocator)
    for &constant in result { constant.bytes=clone_slice(constant.bytes,allocator) }
    return result
}
@(private="package")
phase_destroy :: proc(phase:^Render_Phase,allocator:mem.Allocator) {
    for &draw in phase.draws { draw_destroy(&draw,allocator) }
    image_bindings_destroy(phase.images,allocator);delete(phase.samplers,allocator); delete(phase.draws,allocator); constants_destroy(phase.constants,allocator); phase^={}
}
@(private="package")
phase_clone :: proc(phase:Render_Phase,allocator:mem.Allocator)->Render_Phase {
    result:=phase;result.images=image_bindings_clone(phase.images,allocator); result.samplers=clone_slice(phase.samplers,allocator); result.constants=constants_clone(phase.constants,allocator)
    result.draws=make([]Draw_Op,len(phase.draws),allocator)
    for draw,i in phase.draws { result.draws[i]=draw_clone(draw,allocator) }
    return result
}
@(private="package")
phase_effective_constants :: proc(shared,overrides:[]Constant_Binding,allocator:mem.Allocator)->[]Constant_Binding {
    result:=make([dynamic]Constant_Binding,allocator); defer delete(result)
    append(&result,..shared)
    for override in overrides {
        found:=false
        for &constant in result { if constant.group==override.group && constant.slot==override.slot { constant=override; found=true; break } }
        if !found { append(&result,override) }
    }
    return clone_slice(result[:],allocator)
}
@(private="package")
phase_effective_samplers :: proc(shared,overrides:[]Sampler_Binding,allocator:mem.Allocator)->[]Sampler_Binding {
    result:=make([dynamic]Sampler_Binding,allocator); defer delete(result)
    append(&result,..shared)
    for override in overrides {
        found:=false
        for &sampler in result { if sampler.group==override.group && sampler.slot==override.slot { sampler=override; found=true; break } }
        if !found { append(&result,override) }
    }
    return clone_slice(result[:],allocator)
}
@(private="package")
phase_effective_images :: proc(shared,overrides:[]Image_Binding,allocator:mem.Allocator)->[]Image_Binding {
    result:=make([dynamic]Image_Binding,allocator);defer delete(result)
    append(&result,..shared)
    for override in overrides {
        found:=false
        for &image in result { if image.group==override.group && image.slot==override.slot { image=override;found=true;break } }
        if !found { append(&result,override) }
    }
    return clone_slice(result[:],allocator)
}
@(private="package")
packet_prepare_clone :: proc(packet:Packet,allocator:mem.Allocator)->Packet {
    owned:=packet_clone(packet,allocator)
    #partial switch &render in owned {
    case Render:
        for &phase in render.phases {
            images:=phase_effective_images(render.images,phase.images,allocator)
            owned_images:=image_bindings_clone(images,allocator);delete(images,allocator)
            image_bindings_destroy(phase.images,allocator);phase.images=owned_images
            samplers:=phase_effective_samplers(render.samplers,phase.samplers,allocator)
            delete(phase.samplers,allocator);phase.samplers=samplers
            effective:=phase_effective_constants(render.constants,phase.constants,allocator)
            merged:=constants_clone(effective,allocator); delete(effective,allocator)
            constants_destroy(phase.constants,allocator); phase.constants=merged
        }
        constants_destroy(render.constants,allocator); render.constants=nil
        delete(render.samplers,allocator);render.samplers=nil
        image_bindings_destroy(render.images,allocator);render.images=nil
    }
    return owned
}
