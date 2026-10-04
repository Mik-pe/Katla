//! Images extend the ordinary graph with explicit mip/layer/aspect ownership.
package gfx

@(private="package")
Graph_Image :: struct { desc:Texture_Desc, contract:Image_Import, imported,exported:bool }
/// Declares an image without allocating native storage or installing scene resources.
graph_image :: proc(g:^Graph,desc:Texture_Desc,contract:Image_Import,imported,exported:bool)->(Image_Id,Graph_Error) {
    if !texture_desc_valid(desc) { return {},.Invalid_Resource }
    if !imported && (contract.initial!=.Undefined || contract.initialized) { return {},.Invalid_Resource }
    if contract.initial==.Present || contract.final==.Present {
        if !(.Present in desc.usage) { return {},.Invalid_Usage }
    }
    if contract.initial==.Undefined && contract.initialized { return {},.Invalid_Resource }
    id:=Image_Id{g,len(g.images)}; append(&g.images,Graph_Image{desc,contract,imported,exported}); g.revision+=1
    return id,.None
}
/// Replaces an imported declaration while retaining its logical identity and arrival contract.
/// Callers replace affected commands and compile before recording; failure can restore the old descriptor.
/// Prepared recordings already own their immutable descriptors and native input snapshots.
graph_replace_image :: proc(g:^Graph,id:Image_Id,desc:Texture_Desc)->Graph_Error {
    if g==nil || id.owner!=g || id.index<0 || id.index>=len(g.images) || !texture_desc_valid(desc) { return .Invalid_Resource }
    image:=&g.images[id.index]
    if !image.imported { return .Invalid_Resource }
    if (image.contract.initial==.Present || image.contract.final==.Present) && .Present not_in desc.usage { return .Invalid_Usage }
    if image.desc!=desc { image.desc=desc; g.revision+=1 }
    return .None
}
@(private="package")
validate_image_accesses :: proc(g:^Graph,kind:Pass_Kind,accesses:[]Image_Access)->Graph_Error {
    for access,i in accesses {
        id:=access.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.images) { return .Invalid_Resource }
        desc:=g.images[id.index].desc
        if !image_range_valid(access.range,desc) { return .Invalid_Range }
        if !(access.usage in desc.usage) { return .Invalid_Usage }
        switch kind {
        case .Transfer:
            if !((access.mode==.Read && access.usage==.Transfer_Source) || (access_writes(access.mode) && access.usage==.Transfer_Destination)) { return .Invalid_Usage }
        case .Compute:
            if access.usage!=.Storage && !(access.usage==.Sampled && (access.mode==.Read || access.mode==.None)) { return .Invalid_Usage }
        case .Graphics:
            if access.usage==.Sampled && access_writes(access.mode) { return .Invalid_Usage }
            if access.usage!=.Sampled && access.usage!=.Storage && access.usage!=.Color_Attachment && access.usage!=.Depth_Attachment { return .Invalid_Usage }
            if (access.usage==.Color_Attachment || access.usage==.Depth_Attachment) && (!access_writes(access.mode) || access.range.mip_count!=1 || access.range.layer_count!=1) { return .Invalid_Usage }
        }
        for previous in accesses[:i] {
            if previous.resource==id && image_ranges_overlap(previous.range,access.range) { return .Duplicate_Access }
        }
    }
    return .None
}
@(private="package")
image_access_stores_content :: proc(pass:Graph_Pass,access:Image_Access)->bool {
    if !access_writes(access.mode) { return false }
    if pass.has_packet {
        #partial switch render in pass.packet {
        case Render:
            for color in render.colors {
                if color.access==access { return color.load!=.Discard && color.store==.Store }
            }
            if render.depth.enabled && render.depth.access==access { return render.depth.load!=.Discard && render.depth.store==.Store }
        }
    }
    return true
}
@(private="package")
image_initialized :: proc(g:^Graph,pass_index:int,access:Image_Access)->bool {
    image:=g.images[access.resource.index]
    for aspect in access.range.aspects {
        for mip in access.range.base_mip..<access.range.base_mip+access.range.mip_count {
            for layer in access.range.base_layer..<access.range.base_layer+access.range.layer_count {
                initialized:=image.imported && image.contract.initialized
                found:=false
                for i:=pass_index-1; i>=0; i-=1 {
                    pass:=g.passes[i]
                    for previous in pass.images {
                        r:=previous.range
                        if previous.resource==access.resource && access_writes(previous.mode) && aspect in r.aspects && mip>=r.base_mip && mip-r.base_mip<r.mip_count && layer>=r.base_layer && layer-r.base_layer<r.layer_count {
                            initialized=image_access_stores_content(pass,previous); found=true; break
                        }
                    }
                    if found { break }
                }
                if !initialized { return false }
            }
        }
    }
    return true
}
