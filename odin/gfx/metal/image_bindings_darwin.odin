#+build darwin, arm64
//! Tier-two texture arrays use immutable resource-ID tables owned by the recording frame.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:mem"

@(private="package")
image_view :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,access:gfx.Image_Access,dimension:gfx.Texture_Dimension,arrayed:bool)->(^MTL.Texture,gfx.Gpu_Error) {
    texture,ok:=resolve_texture(r,prepared,access.resource); if !ok { return nil,.Invalid_Resource }
    object:=texture.object
    texture_type:=shader_texture_type(dimension,arrayed)
    if access.range!=gfx.image_full_range(texture.desc) || object->textureType()!=texture_type {
        range:=access.range
        view:=send(^MTL.Texture,object,"newTextureViewWithPixelFormat:textureType:levels:slices:",pixel_format(texture.desc.format),texture_type,NS.Range{NS.UInteger(range.base_mip),NS.UInteger(range.mip_count)},NS.Range{NS.UInteger(range.base_layer),NS.UInteger(range.layer_count)})
        if view==nil { return nil,.Invalid_Range }
        append(&slot.auxiliary,cast(^NS.Object)view); send(nil,slot.residency,"addAllocation:",view); object=view
    }
    return object,.None
}

@(private="package")
encode_image_binding :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,table:^NS.Object,binding:gfx.Image_Binding,index:i32,count:u32,kind:gfx.Metal_Image_Binding,dimension:gfx.Texture_Dimension,arrayed:bool)->gfx.Gpu_Error {
    if count==0 || len(binding.accesses)!=int(count) { return .Invalid_Range }
    if kind==.Texture {
        if count!=1 { return .Invalid_Shader }
        texture,err:=image_view(r,slot,prepared,binding.accesses[0],dimension,arrayed); if err!=.None { return err }
        send(nil,table,"setTexture:atIndex:",texture->gpuResourceID(),NS.UInteger(index))
        return .None
    }
    if r.device->argumentBuffersSupport()!=.Tier2 { return .Unsupported }
    object:=r.device->newBufferWithLength(NS.UInteger(count)*8,MTL.ResourceOptions{.HazardTrackingModeUntracked})
    if object==nil { return .Allocation_Failed }
    append(&slot.auxiliary,cast(^NS.Object)object); send(nil,slot.residency,"addAllocation:",object)
    ids:=mem.slice_ptr(cast(^MTL.ResourceID)raw_data(object->contents()),int(count))
    objects:=make(map[gfx.Image_Access]^MTL.Texture,r.allocator); defer delete(objects)
    for access,i in binding.accesses {
        texture,found:=objects[access]
        if !found {
            err:gfx.Gpu_Error; texture,err=image_view(r,slot,prepared,access,dimension,arrayed); if err!=.None { return err }
            objects[access]=texture
        }
        ids[i]=texture->gpuResourceID()
    }
    send(nil,table,"setAddress:atIndex:",object->gpuAddress(),NS.UInteger(index))
    return .None
}

// The selected WGSL ABI owns the fixed count; Metal reflects only the pointer element.
@(private="package")
reflect_image_array :: proc(binding:^MTL.BufferBinding,count:u32,dimension:gfx.Texture_Dimension,arrayed,depth:bool,sample_type:gfx.Texture_Sample_Type,mode:gfx.Access_Mode)->bool {
    if count==0 || binding->access()!=.ReadOnly || binding->bufferDataSize()!=8 { return false }
    pointer:=binding->bufferPointerType()
    if pointer==nil || pointer->elementType()!=.Struct || pointer->dataSize()!=8 || pointer->alignment()!=8 { return false }
    native:=pointer->elementStructType(); if native==nil { return false }
    members:=native->members(); if members==nil || members->count()!=1 { return false }
    member:=members->objectAs(0,^MTL.StructMember)
    if member->offset()!=0 || member->dataType()!=.Texture { return false }
    texture:=member->textureReferenceType(); if texture==nil { return false }
    expected:=MTL.DataType.Float
    if sample_type==.Sint { expected=.Int }; if sample_type==.Uint { expected=.UInt }
    return texture->textureType()==shader_texture_type(dimension,arrayed) && bool(texture->isDepthTexture())==depth && texture->textureDataType()==expected && binding_access_valid(texture->access(),mode)
}
