//! Authored graphics state and draw packets contain no scene or editor policy.
package gfx

/// Graphics pipelines have identities separate from compute pipelines.
Graphics_Pipeline_Kind :: struct {}
Graphics_Pipeline_Handle :: Handle(Graphics_Pipeline_Kind)
/// Shader visibility belongs to the actual reflected stages.
Shader_Stage :: enum { Vertex, Fragment, Compute }
Shader_Stages :: bit_set[Shader_Stage]
/// Raster primitive topology is selected when creating a pipeline.
Primitive_Topology :: enum { Triangle_List, Triangle_Strip, Line_List, Point_List }
Cull_Mode :: enum { None, Front, Back }
Compare_Op :: enum { Never, Less, Equal, Less_Equal, Greater, Not_Equal, Greater_Equal, Always }
Blend_Factor :: enum { Zero, One, Source_Color, One_Minus_Source_Color, Destination_Color, One_Minus_Destination_Color, Source_Alpha, One_Minus_Source_Alpha, Destination_Alpha, One_Minus_Destination_Alpha }
Blend_Op :: enum { Add, Subtract, Reverse_Subtract, Min, Max }
Color_Component :: enum { Red, Green, Blue, Alpha }
Color_Components :: bit_set[Color_Component]
/// One color target has independent blending and channel writes.
Color_Target :: struct { format:Texture_Format, write_mask:Color_Components, blend_enabled:bool, source_color,destination_color,source_alpha,destination_alpha:Blend_Factor, color_op,alpha_op:Blend_Op }
/// Depth test state names its explicit attachment format.
Depth_State :: struct { enabled,test,write:bool, compare:Compare_Op, format:Texture_Format }
/// Native reflection verifies logical bindings against each stage's exact native index.
Shader_Stage_Buffer :: struct { group,slot:u32, stages:Shader_Stages, usage:Buffer_Usage, vertex_index,fragment_index,vertex_size_index,fragment_size_index:i32, mode:Access_Mode, minimum_size:u64 }
/// Distinguishes direct Metal texture slots from immutable argument-buffer slots.
Metal_Image_Binding :: enum { Texture,Argument_Buffer }

/// Fixed image descriptors preserve their logical count and exact Metal binding namespace.
Shader_Stage_Image :: struct { group,slot:u32, stages:Shader_Stages, usage:Texture_Usage, vertex_index,fragment_index:i32, arrayed,depth:bool, dimension:Texture_Dimension, sample_type:Texture_Sample_Type, storage_format:Texture_Format, mode:Access_Mode, array_count:u32, metal_kind:Metal_Image_Binding }
Shader_Stage_Sampler :: struct { group,slot:u32, stages:Shader_Stages, vertex_index,fragment_index:i32, comparison:bool }
/// Complete target binaries are prepared before encoding.
Graphics_Desc :: struct {
    vertex_entry,fragment_entry,vertex_metal_entry,fragment_metal_entry,vertex_metal_source,fragment_metal_source:string,
    buffers:[]Shader_Stage_Buffer,
    images:[]Shader_Stage_Image,
    samplers:[]Shader_Stage_Sampler,
    vertex_sizes_index,fragment_sizes_index:i32,
    vertex_sizes_words,fragment_sizes_words:u32,
    vertex_spirv,fragment_spirv:[]u32,
    vertex:Vertex_Layout,
    stencil:Stencil_State,
    depth_bias:Depth_Bias,
    wireframe:bool,
    colors:[]Color_Target,
    depth:Depth_State,
    topology:Primitive_Topology,
    cull:Cull_Mode,
    front_counter_clockwise:bool,
}
Load_Op :: enum { Load, Clear, Discard }
Store_Op :: enum { Store, Discard }
/// An explicit color attachment selects exactly one mip and layer.
Color_Attachment :: struct { access:Image_Access, load:Load_Op, store:Store_Op, clear:[4]f64 }
/// Depth/stencil attachment operations share one explicit image.
Depth_Attachment :: struct { enabled:bool, access:Image_Access, load:Load_Op, store:Store_Op, clear_depth:f64, clear_stencil:u32 }
/// Buffer bindings preserve group and stage visibility.
Stage_Buffer_Binding :: struct { group,slot:u32, stages:Shader_Stages, access:Buffer_Access }
/// Each image descriptor element names its explicit graph subresources.
Image_Binding :: struct { group,slot:u32, stages:Shader_Stages, accesses:[]Image_Access }
Sampler_Kind :: struct {}
Sampler_Handle :: Handle(Sampler_Kind)
Filter :: enum { Nearest, Linear }
Address_Mode :: enum { Repeat, Mirror_Repeat, Clamp_Edge, Clamp_Border }
Sampler_Desc :: struct { min_filter,mag_filter,mip_filter:Filter, address_u,address_v,address_w:Address_Mode, min_lod,max_lod:f32, comparison:bool, compare:Compare_Op, max_anisotropy:u32 }
Sampler_Binding :: struct { group,slot:u32, stages:Shader_Stages, handle:Sampler_Handle }
Sampler_Info :: struct { desc:Sampler_Desc, identity:rawptr }
Sampler_Requirement :: struct { group,slot:u32, stages:Shader_Stages, comparison:bool }
/// Generated vertices need no engine mesh or object storage.
Draw :: struct { vertex_count,instance_count,first_vertex,first_instance:u32 }
/// One phase selects pipeline, constants and ordered geometry in an ordinary pass.
Render_Phase :: struct { pipeline:Graphics_Pipeline_Handle, constants:[]Constant_Binding, viewport:Viewport, scissor:Scissor, draws:[]Draw_Op }
/// Shared resource bindings feed ordered phases in one native render pass.
Render :: struct { colors:[]Color_Attachment, depth:Depth_Attachment, buffers:[]Stage_Buffer_Binding, images:[]Image_Binding, samplers:[]Sampler_Binding, constants:[]Constant_Binding, phases:[]Render_Phase }
/// Image copy regions name one mip/layer and aspect with physical coordinates.
Image_Region :: struct { mip,layer,x,y,width,height:u32, aspect:Image_Aspect, z,depth:u32, bytes_per_row,bytes_per_image:u64 }
/// Copies an exact image region into a tightly packed buffer region.
Copy_Image_Buffer :: struct { source:Image_Id, region:Image_Region, destination:Resource_Id, destination_offset:u64 }
/// Native graphics reflection for one buffer binding.
Stage_Buffer_Requirement :: struct { group,slot:u32, stages:Shader_Stages, usage:Buffer_Usage, minimum_size,alignment,maximum_size:u64, mode:Access_Mode }
/// Native graphics reflection for a sampled/storage texture binding.
Texture_Dimension :: enum { D2, D3 }
Texture_Sample_Type :: enum { Float, Sint, Uint }
Image_Binding_Requirement :: struct { group,slot:u32, stages:Shader_Stages, usage:Texture_Usage, arrayed,depth:bool, dimension:Texture_Dimension, sample_type:Texture_Sample_Type, storage_format:Texture_Format, mode:Access_Mode, array_count:u32 }
/// Reflection and target formats remain borrowed from retained native pipelines.
Graphics_Info :: struct { buffers:[]Stage_Buffer_Requirement, images:[]Image_Binding_Requirement, samplers:[]Sampler_Requirement, vertex:Vertex_Layout, supported_draws:Draw_Kinds, colors:[]Texture_Format, depth:Depth_State, stencil:bool }
/// Physical image identity detects aliases before recording.
Texture_Info :: struct { desc:Texture_Desc, identity:rawptr }
Texture_Input :: struct { resource:Image_Id, handle:Texture_Handle }
/// Mandatory native queries for graph image and graphics work.
Graphics_Query :: struct { state:rawptr, texture:proc(rawptr,Texture_Handle)->(Texture_Info,bool), pipeline:proc(rawptr,Graphics_Pipeline_Handle)->(Graphics_Info,bool), sampler:proc(rawptr,Sampler_Handle)->(Sampler_Info,bool) }
/// Checks spatial dimensions and compressed block edge rules.
image_region_valid :: proc(region:Image_Region,desc:Texture_Desc)->bool {
    if !texture_desc_valid(desc) || region.mip>=desc.mip_levels || region.layer>=desc.layers || !(region.aspect in texture_aspects(desc.format)) || region.width==0 || region.height==0 || region.depth==0 { return false }
    width,height,depth:=texture_mip_volume(desc,region.mip)
    if region.x>width || region.width>width-region.x || region.y>height || region.height>height-region.y || region.z>depth || region.depth>depth-region.z { return false }
    bw,bh,_:=texture_block_layout(desc.format)
    return region.x%bw==0 && region.y%bh==0 && (region.width%bw==0 || region.width==width-region.x) && (region.height%bh==0 || region.height==height-region.y)
}
/// The graph access corresponding to one physical copy region.
image_region_range :: proc(region:Image_Region)->Image_Range { return {region.mip,1,region.layer,1,{region.aspect}} }

/// Copies tightly packed buffer texels into one image subresource.
Copy_Buffer_Image :: struct { source:Resource_Id, source_offset:u64, destination:Image_Id, region:Image_Region }
