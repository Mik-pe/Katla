//! Owned glTF/GLB model data uses canonical application geometry and animation.
package app

import km "../math"
import resources "../resources"
import cgltf "../deps/cgltf"
import "core:mem"
import "core:strings"
import "base:runtime"
import "core:slice"

/// Import failures reject the entire candidate before scene publication.
Gltf_Error :: enum { None, IO, Invalid_Path, Invalid_Data, Invalid_Accessor, Invalid_Geometry, Invalid_Animation, Invalid_Skin, Unsupported, Limit, Allocation, Parse, Validation }
/// Imported images retain encoded source bytes; native image decoding/upload has a separate owner.
Gltf_Image :: struct { name,mime:string, encoded:[]byte, source_path:string }
Gltf_Sampler :: struct { min_filter,mag_filter,wrap_s,wrap_t:i32 }
Gltf_Texture :: struct { image,sampler:i32 }
/// Texture indices reference model.textures; -1 means no texture.
Gltf_Texture_View :: struct { texture,texcoord:i32, scale:f32, offset,uv_scale:km.Vec2, rotation:f32 }
Gltf_Alpha_Mode :: enum { Opaque, Mask, Blend }
Gltf_Material_Workflow :: enum { Metallic_Roughness, Specular_Glossiness }
/// glTF PBR factors are linear; color texture bytes retain their authored color space.
Gltf_Material :: struct {
    name:string,
    base_color:km.Vec4,
    metallic,roughness:f32,
    emissive:km.Vec3,
    base_color_texture,normal_texture,metallic_roughness_texture,occlusion_texture,emissive_texture:Gltf_Texture_View,
    alpha_mode:Gltf_Alpha_Mode,
    alpha_cutoff:f32,
    double_sided,unlit:bool,
    workflow:Gltf_Material_Workflow,
    specular:km.Vec3,
    glossiness:f32,
    diffuse_texture,specular_glossiness_texture:Gltf_Texture_View,
}
/// Morph deltas are separate from immutable bind geometry.
Gltf_Morph :: struct { position,normal,tangent:[]km.Vec3 }
/// Skin attributes index the selected node's skin.joints, never global node IDs.
Gltf_Primitive :: struct { mesh:u32, material:i32, geometry:Mesh_Geometry, joints:[][4]u16, weights:[][4]f32, colors:[]km.Vec4, uv_sets:[][]km.Vec2, morphs:[]Gltf_Morph }
/// Original global node indices are preserved across meshes, skins and animation channels.
Gltf_Node :: struct { name:string, parent,mesh,skin:i32, local:km.Transform, local_matrix,world_matrix:km.Mat4, matrix_authored:bool, weights:[]f32 }
Gltf_Skin :: struct { name:string, joints:[]u32, inverse_bind:[]km.Mat4, skeleton:i32 }
Gltf_Scene :: struct { name:string, roots:[]u32 }
/// Owns every extracted resource without retaining C parser pointers or GPU objects.
Gltf_Model :: struct {
    primitives:[]Gltf_Primitive,
    nodes:[]Gltf_Node,
    skins:[]Gltf_Skin,
    materials:[]Gltf_Material,
    images:[]Gltf_Image,
    textures:[]Gltf_Texture,
    samplers:[]Gltf_Sampler,
    scenes:[]Gltf_Scene,
    default_scene:i32,
    animation:Animation_Model,
    allocator:mem.Allocator,
}
MAX_GLTF_NODES :: 65_536
MAX_GLTF_BYTES :: 256*1024*1024
MAX_GLTF_PRIMITIVES :: 65_536

/// Releases model geometry, images, skins, scenes and animation with their captured owner.
gltf_model_destroy :: proc(model:^Gltf_Model) {
    context.allocator=model.allocator
    for &primitive in model.primitives {
        mesh_geometry_destroy(&primitive.geometry); delete(primitive.joints); delete(primitive.weights); delete(primitive.colors)
        for uv in primitive.uv_sets { delete(uv) }; delete(primitive.uv_sets)
        for morph in primitive.morphs { delete(morph.position); delete(morph.normal); delete(morph.tangent) }; delete(primitive.morphs)
    }
    for node in model.nodes { delete(node.name); delete(node.weights) }
    for skin in model.skins { delete(skin.name); delete(skin.joints); delete(skin.inverse_bind) }
    for material in model.materials { delete(material.name) }
    for image in model.images { delete(image.name); delete(image.mime); delete(image.encoded); delete(image.source_path) }
    for scene in model.scenes { delete(scene.name); delete(scene.roots) }
    animation_model_destroy(&model.animation)
    delete(model.primitives); delete(model.nodes); delete(model.skins); delete(model.materials); delete(model.images); delete(model.textures); delete(model.samplers); delete(model.scenes)
    model^={}
}
/// Deep copies an immutable revision for ECS history and staged scene ownership.
gltf_model_clone :: proc(source:^Gltf_Model,allocator:=context.allocator)->Gltf_Model {
    context.allocator=allocator
    result:=source^; result.allocator=allocator
    result.primitives=slice.clone(source.primitives); result.nodes=slice.clone(source.nodes); result.skins=slice.clone(source.skins)
    result.materials=slice.clone(source.materials); result.images=slice.clone(source.images); result.textures=slice.clone(source.textures); result.samplers=slice.clone(source.samplers); result.scenes=slice.clone(source.scenes)
    for &primitive,i in result.primitives {
        original:=&source.primitives[i]; primitive.geometry=mesh_geometry_clone(&original.geometry,allocator); primitive.joints=slice.clone(original.joints); primitive.weights=slice.clone(original.weights); primitive.colors=slice.clone(original.colors)
        primitive.uv_sets=make([][]km.Vec2,len(original.uv_sets)); for uv,j in original.uv_sets { primitive.uv_sets[j]=slice.clone(uv) }
        primitive.morphs=slice.clone(original.morphs); for &morph,j in primitive.morphs { value:=original.morphs[j]; morph.position=slice.clone(value.position); morph.normal=slice.clone(value.normal); morph.tangent=slice.clone(value.tangent) }
    }
    for &node in result.nodes { node.name=strings.clone(node.name); node.weights=slice.clone(node.weights) }
    for &skin in result.skins { skin.name=strings.clone(skin.name); skin.joints=slice.clone(skin.joints); skin.inverse_bind=slice.clone(skin.inverse_bind) }
    for &material in result.materials { material.name=strings.clone(material.name) }
    for &image in result.images { image.name=strings.clone(image.name); image.mime=strings.clone(image.mime); image.encoded=slice.clone(image.encoded); image.source_path=strings.clone(image.source_path) }
    for &scene in result.scenes { scene.name=strings.clone(scene.name); scene.roots=slice.clone(scene.roots) }
    animation_model_clone(&result.animation,&source.animation)
    return result
}
@(private="package")
gltf_name :: proc(name:cstring)->string { if name==nil { return "" }; return strings.clone(string(name)) }
@(private="package")
Gltf_Parser_Memory :: struct { allocator:mem.Allocator, used:uint, failed:bool }
@(private="package")
gltf_parser_alloc :: proc "c" (user:rawptr,size:uint)->rawptr {
    context=runtime.default_context()
    state:=cast(^Gltf_Parser_Memory)user
    if size>MAX_GLTF_BYTES-16 || size+16>MAX_GLTF_BYTES-state.used { state.failed=true; return nil }
    allocation,error:=mem.alloc(int(size)+16,16,state.allocator)
    if error!=.None { state.failed=true; return nil }
    (cast(^uint)allocation)^=size+16; state.used+=size+16
    return rawptr(uintptr(allocation)+16)
}
@(private="package")
gltf_parser_free :: proc "c" (user,pointer:rawptr) {
    if pointer==nil { return }
    context=runtime.default_context()
    state:=cast(^Gltf_Parser_Memory)user; allocation:=rawptr(uintptr(pointer)-16)
    state.used-=(cast(^uint)allocation)^; mem.free(allocation,state.allocator)
}
/// Loads actual GLTF/GLB and every external dependency through the retained confined root.
gltf_load :: proc(root:^resources.Root,path:string,allocator:=context.allocator)->(Gltf_Model,Gltf_Error) {
    if !resources.valid_relative_path(path) { return {},.Invalid_Path }
    context.allocator=allocator
    bytes,read_error:=resources.read_bytes(root,path); defer delete(bytes,root.allocator)
    if read_error!=.None { return {},.IO }
    memory:=Gltf_Parser_Memory{allocator=allocator}
    options:=cgltf.options{memory={alloc_func=gltf_parser_alloc,free_func=gltf_parser_free,user_data=&memory}}
    data,parse_error:=cgltf.parse(options,raw_data(bytes),uint(len(bytes)))
    if parse_error!=.success { if data!=nil { cgltf.free(data) }; return {},.Allocation if memory.failed else .Parse }
    defer { cgltf.free(data); assert(memory.used==0) }
    buffers:=make([][]byte,len(data.buffers),allocator); defer { for buffer in buffers { delete(buffer,allocator) }; delete(buffers) }
    budget:=MAX_GLTF_BYTES-len(bytes)
    if len(data.nodes)>MAX_GLTF_NODES || len(data.meshes)>MAX_GLTF_PRIMITIVES || len(data.materials)>MAX_GLTF_PRIMITIVES || len(data.images)>MAX_GLTF_PRIMITIVES || len(data.buffers)>MAX_GLTF_PRIMITIVES || len(data.accessors)>MAX_GLTF_PRIMITIVES*8 { return {},.Limit }
    if err:=gltf_load_buffers(root,path,data,buffers,&budget); err!=.None { return {},err }
    if err:=gltf_validate_storage(data); err!=.None { return {},err }
    if cgltf.validate(data)!=.success { return {},.Validation }
    for extension in data.extensions_required {
        if string(extension)!="KHR_texture_transform" && string(extension)!="KHR_materials_unlit" && string(extension)!="KHR_materials_pbrSpecularGlossiness" && string(extension)!="KHR_materials_emissive_strength" { return {},.Unsupported }
    }
    model:=Gltf_Model{allocator=allocator,default_scene= -1}
    success:=false; defer { if !success { gltf_model_destroy(&model) } }
    if err:=gltf_extract_materials(data,&model); err!=.None { return {},err }
    if err:=gltf_extract_images(root,path,data,&model,&budget); err!=.None { return {},err }
    if err:=gltf_extract_geometry(data,&model,&budget); err!=.None { return {},err }
    if err:=gltf_extract_nodes(data,&model); err!=.None { return {},err }
    if err:=gltf_extract_skins(data,&model); err!=.None { return {},err }
    if err:=gltf_extract_animation(data,&model,&budget); err!=.None { return {},err }
    success=true; return model,.None
}
