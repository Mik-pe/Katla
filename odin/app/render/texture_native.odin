//! Immutable sampled images upload through ordinary transfer graphs before scene publication.
package render

import gfx "../../gfx"
import app ".."
import "core:mem"
import "core:math"
import image "../../image"
import km "../../math"

/// Material textures retain native allocation ownership independently of source image bytes.
Native_Texture :: struct { texture:gfx.Texture_Handle, desc:gfx.Texture_Desc }
/// Samples glTF filtering and wrapping through explicit generic sampler state.
model_sampler_desc :: proc(source:app.Gltf_Sampler,mip_levels:u32)->gfx.Sampler_Desc {
    result:=gfx.Sampler_Desc{min_filter=.Linear,mag_filter=.Linear,mip_filter=.Linear,address_u=.Repeat,address_v=.Repeat,address_w=.Repeat,max_lod=f32(mip_levels-1),max_anisotropy=1}
    if source.mag_filter==9728 { result.mag_filter=.Nearest }
    if source.min_filter==9728 || source.min_filter==9984 || source.min_filter==9986 { result.min_filter=.Nearest }
    if source.min_filter==9984 || source.min_filter==9985 { result.mip_filter=.Nearest }
    if source.min_filter==9728 || source.min_filter==9729 { result.mip_filter=.None;result.max_lod=0 }
    switch source.wrap_s {
    case 33071: result.address_u=.Clamp_Edge
    case 33648: result.address_u=.Mirror_Repeat
    case:
    }
    switch source.wrap_t {
    case 33071: result.address_v=.Clamp_Edge
    case 33648: result.address_v=.Mirror_Repeat
    case:
    }
    return result
}
/// Prepares role-aware native samples without reducing authored precision.
native_texture_samples :: proc(source:^Texture_Image,srgb:bool,allocator:mem.Allocator=context.allocator)->(gfx.Texture_Format,[]byte,bool,Native_Error) {
    if source==nil || source.width==0 || source.height==0 { return .RGBA8_Unorm,nil,false,{scene=.Invalid_Geometry} }
    bytes_per_channel:u64=1
    switch source.format {
    case .RGBA8: bytes_per_channel=1
    case .RGBA16: bytes_per_channel=2
    case .RGBA32_Float: bytes_per_channel=4
    case: return .RGBA8_Unorm,nil,false,{scene=.Invalid_Material}
    }
    count:=u64(source.width)*u64(source.height)
    if source.width>image.TEXTURE_IMAGE_MAX_DIMENSION || source.height>image.TEXTURE_IMAGE_MAX_DIMENSION || count>image.TEXTURE_IMAGE_MAX_PIXELS || u64(len(source.pixels))>image.TEXTURE_IMAGE_MAX_BYTES || u64(len(source.pixels))!=count*4*bytes_per_channel { return .RGBA8_Unorm,nil,false,{scene=.Invalid_Geometry} }
    if source.format==.RGBA8 { return .RGBA8_Srgb if srgb else .RGBA8_Unorm,source.pixels,false,{} }
    if source.format==.RGBA16 && !srgb { return .RGBA16_Unorm,source.pixels,false,{} }
    samples,allocation_error:=mem.make([]f16,int(count)*4,allocator)
    if allocation_error!=nil || len(samples)!=int(count)*4 || raw_data(samples)==nil { return .RGBA16_Float,nil,false,{gpu=.Allocation_Failed} }
    accepted:=false; defer { if !accepted { delete(samples,allocator) } }
    for index in 0..<int(count) {
        value:=image.texture_image_sample(source,index)
        for channel in 0..<4 {
            scalar:=value[channel]
            if source.format==.RGBA16 && srgb && channel<3 { scalar=km.color_to_linear({scalar,scalar,scalar,1}).r }
            if math.is_nan(scalar) || math.is_inf(scalar) || abs(scalar)>65504 { return .RGBA16_Float,nil,false,{scene=.Invalid_Material} }
            samples[index*4+channel]=f16(scalar)
        }
    }
    accepted=true; return .RGBA16_Float,mem.slice_to_bytes(samples),true,{}
}
/// Uploads accepted integer or HDR samples and generates the real mip chain before publication.
native_texture_upload :: proc(renderer:^$R,operations:GPU_Ops(R),image:^Texture_Image,srgb:bool,allocator:mem.Allocator=context.allocator)->(Native_Texture,Native_Error) {
    format,pixels,owned,error:=native_texture_samples(image,srgb,allocator)
    if error!={} { return {},error }; defer { if owned { delete(pixels,allocator) } }
    return native_texture_upload_pixels(renderer,operations,image.width,image.height,format,pixels,allocator)
}
/// Uploads exact RGBA pixels; neutral tangent normals use representable half-float values.
native_texture_upload_pixels :: proc(renderer:^$R,operations:GPU_Ops(R),width,height:u32,format:gfx.Texture_Format,pixels:[]byte,allocator:mem.Allocator=context.allocator)->(Native_Texture,Native_Error) {
    if renderer==nil || operations.create_texture==nil || operations.destroy_texture==nil || operations.create_buffer==nil || operations.destroy_buffer==nil || operations.acquire==nil || operations.abort==nil || operations.submit==nil || operations.wait==nil || operations.release_exports==nil { return {},{gpu=.Unsupported} }
    if format not_in (bit_set[gfx.Texture_Format]{.RGBA8_Unorm,.RGBA8_Srgb,.RGBA16_Float,.RGBA16_Unorm}) { return {},{gpu=.Unsupported} }
    bytes_per_pixel:u64=8 if format==.RGBA16_Float || format==.RGBA16_Unorm else 4
    if width==0 || height==0 || u64(len(pixels))!=u64(width)*u64(height)*bytes_per_pixel { return {},{scene=.Invalid_Geometry} }
    mips:u32=1; extent:=max(width,height)
    for extent>1 { extent>>=1; mips+=1 }
    desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=mips,format=format,usage={.Sampled,.Transfer_Source,.Transfer_Destination}}
    texture,error:=operations.create_texture(renderer,desc); if error!=.None { return {},{gpu=error} }
    success:=false; defer { if !success { operations.destroy_texture(renderer,texture) } }
    buffer_desc:=gfx.Buffer_Desc{size=u64(len(pixels)),usage={.Transfer_Source},memory=.CPU_Visible}
    buffer:gfx.Buffer_Handle
    buffer,error=operations.create_buffer(renderer,buffer_desc,pixels); if error!=.None { return {},{gpu=error} }
    defer operations.destroy_buffer(renderer,buffer)
    graph:gfx.Graph; gfx.graph_init(&graph,allocator)
    defer { operations.release_exports(renderer,&graph); gfx.graph_destroy(&graph) }
    source,graph_error:=gfx.graph_buffer(&graph,buffer_desc,true,false); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
    target:gfx.Image_Id
    target,graph_error=gfx.graph_image(&graph,desc,{initial=.Undefined,final=.Shader_Read,initialized=false},true,true)
    if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
    base:=gfx.Image_Range{0,1,0,1,{.Color}}
    upload:gfx.Pass_Id
    upload,graph_error=gfx.graph_pass(&graph,"Model image upload",.Transfer,{{source,{0,buffer_desc.size},.Read,.Transfer_Source}},images={{target,base,.Write,.Transfer_Destination}})
    if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
    packet_error:=gfx.graph_set_packet(&graph,upload,gfx.Copy_Buffer_Image{source,0,target,{width=width,height=height,aspect=.Color,depth=1}})
    if packet_error!=.None { return {},{packet=packet_error} }
    if mips>1 {
        lower:=gfx.Image_Range{1,mips-1,0,1,{.Color}}
        generate:gfx.Pass_Id
        generate,graph_error=gfx.graph_pass(&graph,"Model image mip chain",.Transfer,nil,images={{target,base,.Read,.Transfer_Source},{target,lower,.Write,.Transfer_Destination}})
        if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
        packet_error=gfx.graph_set_packet(&graph,generate,gfx.Generate_Mips{target,gfx.image_full_range(desc)})
        if packet_error!=.None { return {},{packet=packet_error} }
    }
    plan:gfx.Compiled_Graph
    plan,graph_error=gfx.graph_compile(&graph); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }; defer gfx.compiled_graph_destroy(&plan)
    token:gfx.Frame_Token
    token,error=operations.acquire(renderer); if error!=.None { return {},{gpu=error} }
    consumed:=false; defer { if !consumed { operations.abort(renderer,token) } }
    submission:gfx.Submission
    submission,error,packet_error=operations.submit(renderer,token,&graph,&plan,{{source,buffer}},{{target,texture}})
    if error!=.None || packet_error!=.None { return {},{gpu=error,packet=packet_error} }
    consumed=true
    error=operations.wait(renderer,submission); if error!=.None { return {},{gpu=error} }
    success=true; return {texture,desc},{}
}
