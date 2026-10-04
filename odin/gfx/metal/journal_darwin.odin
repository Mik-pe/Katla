#+build darwin, arm64
//! Native initialized contents follow accepted queue order and exact subresource ownership.
package metal

import gfx ".."

@(private="package")
Texture_Journal :: struct { texture:^Native_Texture, initialized:[]bool }
@(private="package")
texture_content_index :: proc(desc:gfx.Texture_Desc,mip,layer:u32,aspect:gfx.Image_Aspect)->int { return (int(aspect)*int(desc.layers)+int(layer))*int(desc.mip_levels)+int(mip) }
@(private="package")
set_content :: proc(bits:[]bool,desc:gfx.Texture_Desc,range:gfx.Image_Range,value:bool) {
    for aspect in range.aspects { for layer in range.base_layer..<range.base_layer+range.layer_count { for mip in range.base_mip..<range.base_mip+range.mip_count { bits[texture_content_index(desc,mip,layer,aspect)]=value } } }
}
@(private="package")
content_known :: proc(bits:[]bool,desc:gfx.Texture_Desc,range:gfx.Image_Range)->bool {
    for aspect in range.aspects { for layer in range.base_layer..<range.base_layer+range.layer_count { for mip in range.base_mip..<range.base_mip+range.mip_count { if !bits[texture_content_index(desc,mip,layer,aspect)] { return false } } } }
    return true
}
@(private="package")
new_texture_content :: proc(r:^Renderer,texture:^Native_Texture) { texture.initialized=make([]bool,int(texture.desc.mip_levels)*int(texture.desc.layers)*3,r.allocator) }
@(private="package")
journal_destroy :: proc(r:^Renderer,journals:[]Texture_Journal) { for journal in journals { delete(journal.initialized,r.allocator) }; delete(journals,r.allocator) }
@(private="package")
journal_access :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,journals:[]Texture_Journal,access:gfx.Image_Access,store:=true)->bool {
    texture,ok:=resolve_texture(r,prepared,access.resource); if !ok { return false }
    for journal in journals {
        if journal.texture!=texture { continue }
        if gfx.access_reads(access.mode) && !content_known(journal.initialized,texture.desc,access.range) { return false }
        if gfx.access_writes(access.mode) { set_content(journal.initialized,texture.desc,access.range,store) }
        return true
    }
    return false
}
@(private="package")
prepare_texture_journals :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph)->([]Texture_Journal,gfx.Gpu_Error) {
    journals:=make([]Texture_Journal,len(prepared.images),r.allocator)
    success:=false; defer { if !success { journal_destroy(r,journals) } }
    for image,i in prepared.images {
        texture,ok:=resolve_texture(r,prepared,image.input.resource); if !ok { return nil,.Invalid_Resource }
        duplicate:=false
        for previous in journals[:i] { if previous.texture==texture { duplicate=true; break } }
        journals[i].texture=texture
        if !duplicate { journals[i].initialized=clone_slice(texture.initialized,r) }
        if image.imported && image.contract.initialized && !content_known(texture.initialized,texture.desc,gfx.image_full_range(texture.desc)) { return nil,.Invalid_Graph }
    }
    for pass in prepared.passes {
        for alias in prepared.aliases {
            if alias.after!=pass.id { continue }
            heap:^Native_Heap
            #partial switch resource in alias.next {
            case gfx.Image_Id: texture,ok:=resolve_texture(r,prepared,resource); if ok { heap=texture.heap }
            case gfx.Resource_Id: buffer,ok:=resolve_buffer(r,prepared,resource); if ok { heap=buffer.heap }
            }
            if heap!=nil { for journal in journals { if journal.texture.heap==heap && len(journal.initialized)>0 { set_content(journal.initialized,journal.texture.desc,gfx.image_full_range(journal.texture.desc),false) } } }
        }
        #partial switch packet in pass.packet {
        case gfx.Render:
            for phase in packet.phases {
                entry,ok:=gfx.storage_get(&r.graphics,phase.pipeline); if !ok { return nil,.Invalid_Resource }
                state:=entry^.desc
                if state.stencil.enabled && (!packet.depth.enabled || !(.Stencil in packet.depth.access.range.aspects)) { return nil,.Invalid_Graph }
                if state.depth.enabled && (state.depth.test || state.depth.write) && (!packet.depth.enabled || !(.Depth in packet.depth.access.range.aspects)) { return nil,.Invalid_Graph }
            }
            for attachment in packet.colors {
                access:=attachment.access; if attachment.load!=.Load { access.mode=.Write }
                if !journal_access(r,prepared,journals,access,attachment.store==.Store && attachment.load!=.Discard) { return nil,.Invalid_Graph }
            }
            if packet.depth.enabled {
                access:=packet.depth.access; if packet.depth.load!=.Load { access.mode=.Write }
                if !journal_access(r,prepared,journals,access,packet.depth.store==.Store && packet.depth.load!=.Discard) { return nil,.Invalid_Graph }
            }
            for binding in packet.images { for access in binding.accesses { if !journal_access(r,prepared,journals,access) { return nil,.Invalid_Graph } } }
        case gfx.Dispatch:
            for binding in packet.images { for access in binding.accesses { if !journal_access(r,prepared,journals,access) { return nil,.Invalid_Graph } } }
        case gfx.Generate_Mips:
            base:=packet.range; base.mip_count=1
            if !journal_access(r,prepared,journals,{packet.resource,base,.Read,.Transfer_Source}) { return nil,.Invalid_Graph }
            remaining:=packet.range; remaining.base_mip+=1; remaining.mip_count-=1
            if !journal_access(r,prepared,journals,{packet.resource,remaining,.Write,.Transfer_Destination}) { return nil,.Invalid_Graph }
        case gfx.Copy_Image_Buffer:
            if !journal_access(r,prepared,journals,{packet.source,gfx.image_region_range(packet.region),.Read,.Transfer_Source}) { return nil,.Invalid_Graph }
        case gfx.Copy_Buffer_Image:
            texture,ok:=resolve_texture(r,prepared,packet.destination); if !ok { return nil,.Invalid_Resource }
            width,height,depth:=gfx.texture_mip_volume(texture.desc,packet.region.mip)
            mode:=gfx.Access_Mode.Read_Write
            if packet.region.x==0 && packet.region.y==0 && packet.region.width==width && packet.region.height==height && packet.region.z==0 && packet.region.depth==depth { mode=.Write }
            if !journal_access(r,prepared,journals,{packet.destination,gfx.image_region_range(packet.region),mode,.Transfer_Destination}) { return nil,.Invalid_Graph }
        }
    }
    success=true; return journals,.None
}

@(private="package")
invalidate_heap_content :: proc(r:^Renderer,heap:^Native_Heap,except:^Native_Texture) {
    if heap==nil { return }
    for slot in r.textures.slots { if slot.occupied && slot.value.heap==heap && slot.value!=except { set_content(slot.value.initialized,slot.value.desc,gfx.image_full_range(slot.value.desc),false) } }
    for source in r.exports { if source.texture.heap==heap && source.texture!=except { set_content(source.texture.initialized,source.texture.desc,gfx.image_full_range(source.texture.desc),false) } }
}

@(private="package")
texture_epoch :: proc(texture:^Native_Texture)->u64 { return texture.heap.epoch if texture.heap!=nil else texture.content_epoch }
@(private="package")
mark_texture_written :: proc(r:^Renderer,texture:^Native_Texture) {
    r.content_epoch+=1
    texture.content_epoch=r.content_epoch
    for source in r.exports { if source.texture.object==texture.object { source.texture.content_epoch=r.content_epoch } }
    for slot in r.textures.slots { if slot.occupied && slot.value.object==texture.object { slot.value.content_epoch=r.content_epoch } }
    if texture.heap!=nil { texture.heap.epoch=r.content_epoch; invalidate_heap_content(r,texture.heap,texture) }
}
@(private="package")
mark_buffer_written :: proc(r:^Renderer,buffer:^Native_Buffer) {
    if buffer.heap!=nil { r.content_epoch+=1; buffer.heap.epoch=r.content_epoch; invalidate_heap_content(r,buffer.heap,nil) }
}
@(private="package")
commit_content_epochs :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph) {
    for pass in prepared.passes {
        #partial switch packet in pass.packet {
        case gfx.Render:
            for attachment in packet.colors { texture,_:=resolve_texture(r,prepared,attachment.access.resource); mark_texture_written(r,texture) }
            if packet.depth.enabled { texture,_:=resolve_texture(r,prepared,packet.depth.access.resource); mark_texture_written(r,texture) }
            for binding in packet.images { for access in binding.accesses { if gfx.access_writes(access.mode) { texture,_:=resolve_texture(r,prepared,access.resource); mark_texture_written(r,texture) } } }
        case gfx.Dispatch:
            for binding in packet.bindings { if gfx.access_writes(binding.access.mode) { buffer,_:=resolve_buffer(r,prepared,binding.access.resource); mark_buffer_written(r,buffer) } }
            for binding in packet.images { for access in binding.accesses { if gfx.access_writes(access.mode) { texture,_:=resolve_texture(r,prepared,access.resource); mark_texture_written(r,texture) } } }
        case gfx.Fill_Buffer:
            buffer,_:=resolve_buffer(r,prepared,packet.destination); mark_buffer_written(r,buffer)
        case gfx.Generate_Mips:
            texture,_:=resolve_texture(r,prepared,packet.resource); mark_texture_written(r,texture)
        case gfx.Copy_Buffer:
            buffer,_:=resolve_buffer(r,prepared,packet.destination); mark_buffer_written(r,buffer)
        case gfx.Copy_Buffer_Image:
            texture,_:=resolve_texture(r,prepared,packet.destination); mark_texture_written(r,texture)
        }
    }
}

@(private="package")
content_epochs_available :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph)->bool {
    budget:u64
    for pass in prepared.passes {
        count:u64=1
        #partial switch packet in pass.packet {
        case gfx.Render: count=u64(len(packet.colors))+1; for binding in packet.images { count+=u64(len(binding.accesses)) }
        case gfx.Dispatch: count=u64(len(packet.bindings)); for binding in packet.images { count+=u64(len(binding.accesses)) }
        }
        if count>max(u64)-budget { return false }
        budget+=count
    }
    return budget<=max(u64)-r.content_epoch
}
