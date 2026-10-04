//! Graphics preflight freezes attachment and stage bindings before native recording.
package gfx

import "core:mem"
import "core:math"

/// An image recording carries its actual declaration and arrival/final contract.
Prepared_Image :: struct { input:Texture_Input, desc:Texture_Desc, contract:Image_Import, imported,exported:bool }
@(private="package")
clone_slice :: proc(values:[]$T,allocator:mem.Allocator)->[]T { result:=make([]T,len(values),allocator); copy(result,values); return result }
@(private="package")
image_access_declared :: proc(pass:Graph_Pass,access:Image_Access)->bool {
    for declared in pass.images { if declared==access { return true } }
    return false
}
@(private="package")
packet_image_accesses :: proc(g:^Graph,packet:Packet,allocator:mem.Allocator)->[]Image_Access {
    #partial switch p in packet {
    case Dispatch:
        count:int; for binding in p.images { count+=len(binding.accesses) }
        result:=make([]Image_Access,count,allocator)
        index:int
        for binding in p.images { copy(result[index:],binding.accesses); index+=len(binding.accesses) }
        return result
    case Render:
        accesses:=make([dynamic]Image_Access,allocator);defer delete(accesses)
        for color in p.colors { append(&accesses,color.access) }
        if p.depth.enabled { append(&accesses,p.depth.access) }
        for phase in p.phases {
            effective:=phase_effective_images(p.images,phase.images,allocator)
            for binding in effective { for access in binding.accesses { append_image_access_unique(&accesses,access) } }
            delete(effective,allocator)
        }
        return clone_slice(accesses[:],allocator)
    case Generate_Mips:
        base:=p.range; base.mip_count=1
        rest:=p.range; rest.base_mip+=1; rest.mip_count-=1
        return clone_slice([]Image_Access{{p.resource,base,.Read,.Transfer_Source},{p.resource,rest,.Write,.Transfer_Destination}},allocator)
    case Copy_Buffer_Image:
        desc:=g.images[p.destination.index].desc
        width,height,depth:=texture_mip_volume(desc,p.region.mip)
        mode:Access_Mode=(p.region.x==0 && p.region.y==0 && p.region.z==0 && p.region.width==width && p.region.height==height && p.region.depth==depth) ? .Write : .Read_Write
        return clone_slice([]Image_Access{{p.destination,image_region_range(p.region),mode,.Transfer_Destination}},allocator)
    case Copy_Image_Buffer:
        return clone_slice([]Image_Access{{p.source,image_region_range(p.region),.Read,.Transfer_Source}},allocator)
    }
    return nil
}
@(private="package")
validate_attachment :: proc(g:^Graph,access:Image_Access,load:Load_Op,store:Store_Op,depth:bool)->Packet_Error {
    id:=access.resource
    if id.owner!=g || id.index<0 || id.index>=len(g.images) || !image_range_valid(access.range,g.images[id.index].desc) { return .Invalid_Packet }
    if g.images[id.index].desc.depth!=1 || access.range.aspects!=texture_aspects(g.images[id.index].desc.format) || access.range.mip_count!=1 || access.range.layer_count!=1 || access.usage!=(depth ? .Depth_Attachment : .Color_Attachment) { return .Invalid_Packet }
    // Partial raster coverage cannot initialize an image without a clear or load.
    if (load==.Load && access.mode!=.Read_Write) || (load!=.Load && access.mode!=.Write) { return .Invalid_Packet }
    return .None
}
@(private="package")
validate_render_packet :: proc(g:^Graph,pass:Graph_Pass,p:Render)->Packet_Error {
    if pass.kind!=.Graphics || (len(p.colors)==0 && !p.depth.enabled) || len(p.colors)>8 { return .Invalid_Packet }
    width,height:u32
    for color in p.colors {
        err:=validate_attachment(g,color.access,color.load,color.store,false); if err!=.None { return err }
        for value in color.clear { if math.is_nan(value) || math.is_inf(value) { return .Invalid_Packet } }
        w,h:=texture_mip_extent(g.images[color.access.resource.index].desc,color.access.range.base_mip)
        if width!=0 && (w!=width || h!=height) { return .Invalid_Packet }; width=w; height=h
    }
    if p.depth.enabled {
        err:=validate_attachment(g,p.depth.access,p.depth.load,p.depth.store,true); if err!=.None { return err }
        if math.is_nan(p.depth.clear_depth) || math.is_inf(p.depth.clear_depth) || p.depth.clear_depth<0 || p.depth.clear_depth>1 { return .Invalid_Packet }
        w,h:=texture_mip_extent(g.images[p.depth.access.resource.index].desc,p.depth.access.range.base_mip)
        if width!=0 && (w!=width || h!=height) { return .Invalid_Packet }; width=w; height=h
    }
    for phase in p.phases {
        if phase.viewport.enabled {
            values:=[6]f64{phase.viewport.x,phase.viewport.y,phase.viewport.width,phase.viewport.height,phase.viewport.min_depth,phase.viewport.max_depth}
            for value in values { if math.is_nan(value) || math.is_inf(value) { return .Invalid_Packet } }
            if phase.viewport.width<=0 || phase.viewport.height<=0 || phase.viewport.min_depth<0 || phase.viewport.max_depth>1 || phase.viewport.min_depth>phase.viewport.max_depth { return .Invalid_Packet }
        }
        if phase.scissor.enabled && (phase.scissor.width==0 || phase.scissor.height==0 || phase.scissor.x>width || phase.scissor.width>width-phase.scissor.x || phase.scissor.y>height || phase.scissor.height>height-phase.scissor.y) { return .Invalid_Packet }
        for draw in phase.draws { err:=validate_draw_packet(draw); if err!=.None { return err } }
        image_error:=validate_graphics_images(phase.images,p);if image_error!=.None { return image_error }
        view:=p;view.images=phase.images
        sampler_error:=validate_graphics_samplers(phase.samplers,view);if sampler_error!=.None { return sampler_error }
        view.samplers=phase.samplers
        err:=validate_constants(phase.constants,view); if err!=.None { return err }
        err=validate_constants(phase.constants,p);if err!=.None { return err }
        err=validate_constants(p.constants,view);if err!=.None { return err }
    }
    constant_error:=validate_constants(p.constants,p); if constant_error!=.None { return constant_error }
    for binding,i in p.buffers {
        if binding.stages=={} || .Compute in binding.stages { return .Invalid_Binding }
        for previous in p.buffers[:i] { if previous.group==binding.group && previous.slot==binding.slot { return .Invalid_Binding } }
    }
    image_error:=validate_graphics_images(p.images,p);if image_error!=.None { return image_error }
    sampler_error:=validate_graphics_samplers(p.samplers,p);if sampler_error!=.None { return sampler_error }
    if len(p.phases)==0 && (len(p.buffers)>0 || len(p.images)>0 || len(p.samplers)>0 || len(p.constants)>0) { return .Invalid_Packet }
    return .None
}
/// Resolves an image without interpreting a shader descriptor index.
prepared_texture :: proc(prepared:^Prepared_Graph,id:Image_Id)->(Texture_Handle,bool) {
    for input in prepared.textures { if input.resource==id { return input.handle,true } }
    return {},false
}
@(private="package")
input_texture :: proc(inputs:[]Texture_Input,id:Image_Id,query:Graphics_Query)->(Texture_Info,bool) {
    if query.texture==nil { return {},false }
    for input in inputs { if input.resource==id { return query.texture(query.state,input.handle) } }
    return {},false
}
@(private="package")
prepare_images :: proc(prepared:^Prepared_Graph,g:^Graph,plan:^Compiled_Graph,textures:[]Texture_Input,query:Graphics_Query)->Packet_Error {
    if len(textures)>0 && query.texture==nil { return .Unsupported_Query }
    for input,i in textures {
        id:=input.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.images) { return .Missing_Resource }
        info,ok:=query.texture(query.state,input.handle)
        if !ok || info.identity==nil { return .Invalid_Buffer }
        image:=g.images[id.index]; desc:=image.desc
        if info.desc.depth!=desc.depth || info.desc.width!=desc.width || info.desc.height!=desc.height || info.desc.mip_levels!=desc.mip_levels || info.desc.layers!=desc.layers || info.desc.format!=desc.format || desc.usage&info.desc.usage!=desc.usage { return .Invalid_Buffer }
        for previous in textures[:i] {
            if previous.resource==id { return .Invalid_Buffer }
            old,_:=query.texture(query.state,previous.handle)
            if old.identity==info.identity {
                err:=prepare_image_alias(prepared,g,plan,previous.resource,id); if err!=.None { return err }
            }
        }
    }
    for id in plan.order {
        for access in g.passes[id.index].images {
            _,ok:=input_texture(textures,access.resource,query); if !ok { return .Missing_Resource }
        }
    }
    prepared.textures=clone_slice(textures,g.allocator)
    prepared.images=make([]Prepared_Image,len(textures),g.allocator)
    for input,i in textures { image:=g.images[input.resource.index]; prepared.images[i]={input,image.desc,image.contract,image.imported,image.exported} }
    prepared.image_hazards=clone_slice(plan.image_hazards[:],g.allocator)
    return .None
}
@(private="package")
prepare_packet_bindings :: proc(g:^Graph,packet:Packet,buffers:[]Buffer_Input,buffer_query:Resource_Query,textures:[]Texture_Input,query:Graphics_Query)->Packet_Error {
    #partial switch p in packet {
    case Dispatch:
        if buffer_query.pipeline==nil { return .Unsupported_Query }
        info,ok:=buffer_query.pipeline(buffer_query.state,p.pipeline); if !ok { return .Invalid_Pipeline }
        if len(info.images)!=len(p.images) || len(info.samplers)!=len(p.samplers) { return .Invalid_Pipeline }
        err:=preflight_image_bindings(p.images,info.images,textures,query); if err!=.None { return err }
        return preflight_sampler_bindings(p.samplers,info.samplers,query)
    case Render:
        if len(p.phases)==0 { return .None }
        if query.pipeline==nil { return .Unsupported_Query }
        buffer_stages:=make([]Shader_Stages,len(p.buffers),g.allocator); defer delete(buffer_stages,g.allocator)
        image_stages:=make([]Shader_Stages,len(p.images),g.allocator); defer delete(image_stages,g.allocator)
        image_overridden:=make([]bool,len(p.images),g.allocator);defer delete(image_overridden,g.allocator)
        sampler_stages:=make([]Shader_Stages,len(p.samplers),g.allocator); defer delete(sampler_stages,g.allocator)
        sampler_overridden:=make([]bool,len(p.samplers),g.allocator);defer delete(sampler_overridden,g.allocator)
        for phase in p.phases {
            info,pipeline_ok:=query.pipeline(query.state,phase.pipeline); if !pipeline_ok { return .Invalid_Pipeline }
            if len(info.colors)!=len(p.colors) || info.depth.enabled!=p.depth.enabled { return .Invalid_Pipeline }
            for color,i in p.colors { if info.colors[i]!=g.images[color.access.resource.index].desc.format { return .Invalid_Pipeline } }
            if p.depth.enabled && info.depth.format!=g.images[p.depth.access.resource.index].desc.format { return .Invalid_Pipeline }
            if info.stencil && (!p.depth.enabled || !(.Stencil in p.depth.access.range.aspects)) { return .Invalid_Pipeline }
            for draw in phase.draws { err:=preflight_draw(draw,info); if err!=.None { return err } }
            effective:=phase_effective_constants(p.constants,phase.constants,g.allocator); defer delete(effective,g.allocator)
            for requirement in info.buffers {
                found:=false
                for binding,i in p.buffers {
                    if binding.group!=requirement.group || binding.slot!=requirement.slot { continue }; found=true
                    buffer_stages[i]|=requirement.stages
                    a:=binding.access
                    if binding.stages&requirement.stages!=requirement.stages || a.usage!=requirement.usage || !access_covers(a.mode,requirement.mode) || a.range.size<requirement.minimum_size || a.range.size>requirement.maximum_size || requirement.alignment==0 || a.range.offset%requirement.alignment!=0 { return .Invalid_Binding }
                    _,ok:=input_buffer(buffers,a.resource,buffer_query); if !ok { return .Missing_Resource }
                }
                for constant in effective {
                    if constant.group!=requirement.group || constant.slot!=requirement.slot { continue }; found=true
                    size:=u64(len(constant.bytes))
                    if constant.stages&requirement.stages!=requirement.stages || constant.usage!=requirement.usage || size<requirement.minimum_size || size>requirement.maximum_size { return .Invalid_Binding }
                }
                if !found { return .Missing_Binding }
            }
            effective_images:=phase_effective_images(p.images,phase.images,g.allocator);defer delete(effective_images,g.allocator)
            err:=preflight_image_bindings(effective_images,info.images,textures,query); if err!=.None { return err }
            for override in phase.images {
                stages:Shader_Stages
                for requirement in info.images { if requirement.group==override.group && requirement.slot==override.slot { stages|=requirement.stages } }
                if stages!=override.stages { return .Invalid_Binding }
            }
            effective_samplers:=phase_effective_samplers(p.samplers,phase.samplers,g.allocator);defer delete(effective_samplers,g.allocator)
            err=preflight_sampler_bindings(effective_samplers,info.samplers,query); if err!=.None { return err }
            for override in phase.samplers {
                stages:Shader_Stages
                for requirement in info.samplers { if requirement.group==override.group && requirement.slot==override.slot { stages|=requirement.stages } }
                if stages!=override.stages { return .Invalid_Binding }
            }
            for requirement in info.images { for binding,i in p.images {
                overridden:=false;for override in phase.images { if override.group==binding.group && override.slot==binding.slot { overridden=true;image_overridden[i]=true;break } }
                if !overridden && requirement.group==binding.group && requirement.slot==binding.slot { image_stages[i]|=requirement.stages }
            } }
            for requirement in info.samplers { for binding,i in p.samplers {
                overridden:=false;for override in phase.samplers { if override.group==binding.group && override.slot==binding.slot { overridden=true;sampler_overridden[i]=true;break } }
                if !overridden && requirement.group==binding.group && requirement.slot==binding.slot { sampler_stages[i]|=requirement.stages }
            } }
        }
        for binding,i in p.buffers { if buffer_stages[i]!=binding.stages { return .Invalid_Binding } }
        for binding,i in p.images { if (!image_overridden[i] || image_stages[i]!={}) && image_stages[i]!=binding.stages { return .Invalid_Binding } }
        for binding,i in p.samplers { if (!sampler_overridden[i] || sampler_stages[i]!={}) && sampler_stages[i]!=binding.stages { return .Invalid_Binding } }

    }
    return .None
}

@(private="package")
validate_constants :: proc(constants:[]Constant_Binding,render:Render)->Packet_Error {
    for constant,i in constants {
        if len(constant.bytes)==0 || constant.stages=={} || .Compute in constant.stages || (constant.usage!=.Uniform && constant.usage!=.Storage) { return .Invalid_Binding }
        for old in constants[:i] { if old.group==constant.group && old.slot==constant.slot { return .Invalid_Binding } }
        for buffer in render.buffers { if buffer.group==constant.group && buffer.slot==constant.slot { return .Invalid_Binding } }
        for image in render.images { if image.group==constant.group && image.slot==constant.slot { return .Invalid_Binding } }
        for sampler in render.samplers { if sampler.group==constant.group && sampler.slot==constant.slot { return .Invalid_Binding } }
    }
    return .None
}

@(private="package")
validate_graphics_samplers :: proc(bindings:[]Sampler_Binding,render:Render)->Packet_Error {
    for binding,i in bindings {
        if binding.stages=={} || .Compute in binding.stages { return .Invalid_Binding }
        for previous in bindings[:i] { if previous.group==binding.group && previous.slot==binding.slot { return .Invalid_Binding } }
        for buffer in render.buffers { if buffer.group==binding.group && buffer.slot==binding.slot { return .Invalid_Binding } }
        for image in render.images { if image.group==binding.group && image.slot==binding.slot { return .Invalid_Binding } }
        for constant in render.constants { if constant.group==binding.group && constant.slot==binding.slot { return .Invalid_Binding } }
    }
    return .None
}

@(private="package")
append_image_access_unique :: proc(accesses:^[dynamic]Image_Access,access:Image_Access) {
    for previous in accesses { if previous==access { return } };append(accesses,access)
}
@(private="package")
validate_graphics_images :: proc(bindings:[]Image_Binding,render:Render)->Packet_Error {
    for binding,i in bindings {
        if binding.stages=={} || .Compute in binding.stages || len(binding.accesses)==0 { return .Invalid_Binding }
        for previous in bindings[:i] { if previous.group==binding.group && previous.slot==binding.slot { return .Invalid_Binding } }
        for buffer in render.buffers { if buffer.group==binding.group && buffer.slot==binding.slot { return .Invalid_Binding } }
        for sampler in render.samplers { if sampler.group==binding.group && sampler.slot==binding.slot { return .Invalid_Binding } }
        for constant in render.constants { if constant.group==binding.group && constant.slot==binding.slot { return .Invalid_Binding } }
    }
    return .None
}
