//! Compute textures and samplers use the same immutable native preflight as graphics.
package gfx

@(private="package")
validate_compute_bindings :: proc(dispatch:Dispatch)->Packet_Error {
    for image,i in dispatch.images {
        if image.stages!={.Compute} { return .Invalid_Binding }
        for previous in dispatch.images[:i] { if previous.group==image.group && previous.slot==image.slot { return .Invalid_Binding } }
        for buffer in dispatch.bindings { if buffer.group==image.group && buffer.slot==image.slot { return .Invalid_Binding } }
    }
    for sampler,i in dispatch.samplers {
        if sampler.stages!={.Compute} { return .Invalid_Binding }
        for previous in dispatch.samplers[:i] { if previous.group==sampler.group && previous.slot==sampler.slot { return .Invalid_Binding } }
        for buffer in dispatch.bindings { if buffer.group==sampler.group && buffer.slot==sampler.slot { return .Invalid_Binding } }
        for image in dispatch.images { if image.group==sampler.group && image.slot==sampler.slot { return .Invalid_Binding } }
    }
    return .None
}
@(private="package")
preflight_image_bindings :: proc(bindings:[]Image_Binding,requirements:[]Image_Binding_Requirement,textures:[]Texture_Input,query:Graphics_Query)->Packet_Error {
    for requirement in requirements {
        found:=false
        for binding in bindings {
            if binding.group!=requirement.group || binding.slot!=requirement.slot { continue }; found=true
            if binding.stages&requirement.stages!=requirement.stages || binding.access.usage!=requirement.usage || !access_covers(binding.access.mode,requirement.mode) { return .Invalid_Binding }
            texture,ok:=input_texture(textures,binding.access.resource,query); if !ok { return .Missing_Resource }
            range:=binding.access.range
            if (requirement.dimension==.D3)!=(texture.desc.depth>1) || (requirement.dimension==.D3 && requirement.arrayed) { return .Invalid_Binding }
            if !requirement.arrayed && range.layer_count!=1 { return .Invalid_Binding }
            if requirement.depth!=!(.Color in texture_aspects(texture.desc.format)) { return .Invalid_Binding }
            if requirement.sample_type==.Sint || (requirement.sample_type==.Uint)!=(texture.desc.format==.R32_Uint) { return .Invalid_Binding }
            if requirement.usage==.Storage && texture.desc.format!=requirement.storage_format { return .Invalid_Binding }
        }
        if !found { return .Missing_Binding }
    }
    return .None
}
@(private="package")
preflight_sampler_bindings :: proc(bindings:[]Sampler_Binding,requirements:[]Sampler_Requirement,query:Graphics_Query)->Packet_Error {
    for requirement in requirements {
        found:=false
        for binding in bindings {
            if binding.group!=requirement.group || binding.slot!=requirement.slot { continue }; found=true
            if query.sampler==nil { return .Unsupported_Query }
            sampler,ok:=query.sampler(query.state,binding.handle)
            if !ok || sampler.identity==nil || binding.stages&requirement.stages!=requirement.stages || sampler.desc.comparison!=requirement.comparison { return .Invalid_Binding }
        }
        if !found { return .Missing_Binding }
    }
    return .None
}
