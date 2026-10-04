#+build darwin, arm64
//! Real production model shader pixels cover complete affine/skinned PBR lighting.
package main
import render "../../app/render"
import gfx "../../gfx"
import km "../../math"
import "core:mem"
import "core:math"
import "core:fmt"
exercise_lighting :: proc(renderer:^$R,ops:render.GPU_Ops(R),reads:Read_Ops(R),model:^render.Model_Shader,backend:string)->[480]u16 {
    graph:gfx.Graph;gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    buffer_owners:=make([dynamic]gfx.Buffer_Handle);defer delete(buffer_owners)
    inputs:=make([dynamic]gfx.Buffer_Input);defer delete(inputs)
    texture_owners:=make([dynamic]gfx.Texture_Handle);defer delete(texture_owners)
    textures:=make([dynamic]gfx.Texture_Input);defer delete(textures)
    descriptor:=model.descriptors[1];descriptor.depth={}
    pipeline,pipeline_error:=ops.create_pipeline(renderer,descriptor);assert(pipeline_error==.None)
    samples:=lighting_cases();transforms:=lighting_transforms();objects:[120]render.Model_Object;frames:[120]render.Frame_Data
    phases:[120]gfx.Render_Phase;phase_constants:[120][3]gfx.Constant_Binding;draws:[120][1]gfx.Draw_Op
    phase_lighting:[120]render.Lighting_Frame;phase_counts:[120][8]u32
    identity:=km.identity(km.Mat4);vertices:[36]render.Model_Vertex
    sun:=km.normalize(km.Vec3{.5,.25,1})
    for transform,transform_index in transforms { for path in 0..<4 {
        path_index:=transform_index*4+path;geometry,object:=lighting_geometry(transform,path)
        for vertex,j in geometry { vertices[path_index*3+j]=vertex }
        for sample,case_index in samples {
            i:=path_index*10+case_index
            objects[i]={model=object,normal_model=identity,base_color={.6,.3,.15,1},factors={sample.metallic,sample.roughness,sample.ao,sample.scale},emissive={sample.emission[0],sample.emission[1],sample.emission[2],0},flags={0,0,0,1}}
            frames[i]={view_projection=identity,camera_position={0,0,5,1},light_direction={-sun[0],-sun[1],-sun[2],0},light_color={1,.8,.6,1.7},ambient={.15,.15,.15,-1 if backend=="vulkan" else 1}}
            frames[i].view_projection[3][2]=.5
            phase_lighting[i]={view=identity,inverse_view_projection=identity,viewport={120,1,8,1},settings={2,0,1,0}}
            for &count in phase_counts[i] { count=2 if sample.points else 0 }
            phase_constants[i]={{group=0,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(frames[i:i+1])},{group=3,slot=0,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(phase_lighting[i:i+1])},{group=3,slot=3,stages={.Fragment},usage=.Storage,bytes=mem.slice_to_bytes(phase_counts[i][:])}}
            draws[i][0]=gfx.Draw{3,1,u32(path_index*3),u32(i)}
            phases[i]={pipeline=pipeline,constants=phase_constants[i][:],viewport={true,f64(i),0,1,1,0,1},scissor={true,u32(i),0,1,1},draws=draws[i][:]}
        }
    } }
    object_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(objects[:]))
    vertex_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(vertices[:]))
    points:[256]render.Point_Light_GPU;points[0]={position={2,1,4.5},range=8,color={.8,.3,.1},intensity=2};points[1]={position={-1,2,3.5},range=6,color={.1,.4,.9},intensity=1.5}
    indices:[1024]u32;for tile in 0..<8 { indices[tile*128+1]=1 }
    shadow:=[1]render.Shadow_Frame{{direction={-sun[0],-sun[1],-sun[2],0}}}
    for &cascade in shadow[0].cascades { cascade.view_projection=identity;cascade.view_projection[3][2]=.5;cascade.split_texel={100,0,0,0} }
    point_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(points[:]))
    index_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(indices[:]))
    shadow_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(shadow[:]))
    bindings:=[5]gfx.Stage_Buffer_Binding{{0,1,{.Vertex,.Fragment},object_access},{0,2,{.Vertex},vertex_access},{3,1,{.Fragment},point_access},{3,2,{.Fragment},index_access},{4,0,{.Fragment},shadow_access}}
    accesses:=[5]gfx.Buffer_Access{object_access,vertex_access,point_access,index_access,shadow_access}
    white:=[4]f16{1,1,1,1};neutral:=[4]f16{.5,.5,1,1};oblique:=[4]f16{.75,.5,1,1}
    sampled_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA16_Float,{.Sampled,.Transfer_Destination},1}
    white_handle,white_error:=reads.create_texture(renderer,sampled_desc,mem.slice_to_bytes(white[:]));assert(white_error==.None);append(&texture_owners,white_handle)
    neutral_handle,normal_error:=reads.create_texture(renderer,sampled_desc,mem.slice_to_bytes(neutral[:]));assert(normal_error==.None);append(&texture_owners,neutral_handle)
    oblique_handle,oblique_error:=reads.create_texture(renderer,sampled_desc,mem.slice_to_bytes(oblique[:]));assert(oblique_error==.None);append(&texture_owners,oblique_handle)
    oblique_image,_:=gfx.graph_image(&graph,sampled_desc,{.Shader_Read,.Shader_Read,true},true,false);append(&textures,gfx.Texture_Input{oblique_image,oblique_handle})
    depth_desc:=gfx.Texture_Desc{4,4,1,1,.D32_Float,{.Sampled,.Depth_Attachment},1}
    depth_handle,depth_error:=ops.create_texture(renderer,depth_desc);assert(depth_error==.None);append(&texture_owners,depth_handle)
    white_image,_:=gfx.graph_image(&graph,sampled_desc,{.Shader_Read,.Shader_Read,true},true,false);normal_image,_:=gfx.graph_image(&graph,sampled_desc,{.Shader_Read,.Shader_Read,true},true,false)
    depth_image,_:=gfx.graph_image(&graph,depth_desc,{},false,false)
    depth_write:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    clear_depth,_:=gfx.graph_pass(&graph,"initialize-disabled-shadow-atlas",.Graphics,nil,images={depth_write})
    assert(gfx.graph_set_packet(&graph,clear_depth,gfx.Render{depth={enabled=true,access=depth_write,load=.Clear,store=.Store,clear_depth=1}})==.None)
    append(&textures,gfx.Texture_Input{white_image,white_handle},gfx.Texture_Input{normal_image,neutral_handle},gfx.Texture_Input{depth_image,depth_handle})
    dark_handle,dark_error:=ops.create_texture(renderer,depth_desc);assert(dark_error==.None);append(&texture_owners,dark_handle)
    dark_image,_:=gfx.graph_image(&graph,depth_desc,{},false,false);append(&textures,gfx.Texture_Input{dark_image,dark_handle})
    dark_write:=gfx.Image_Access{dark_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    dark_clear,_:=gfx.graph_pass(&graph,"initialize-shadow-occluder-depth",.Graphics,nil,images={dark_write})
    assert(gfx.graph_set_packet(&graph,dark_clear,gfx.Render{depth={enabled=true,access=dark_write,load=.Clear,store=.Store,clear_depth=0}})==.None)
    white_access:=gfx.Image_Access{white_image,gfx.image_full_range(sampled_desc),.Read,.Sampled};normal_access:=gfx.Image_Access{normal_image,gfx.image_full_range(sampled_desc),.Read,.Sampled};depth_read:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Read,.Sampled}
    image_bindings:=[6]gfx.Image_Binding{{1,0,{.Fragment},{white_access}},{1,1,{.Fragment},{normal_access}},{1,2,{.Fragment},{white_access}},{1,3,{.Fragment},{white_access}},{1,4,{.Fragment},{white_access}},{4,1,{.Fragment},{depth_read}}}
    oblique_read:=gfx.Image_Access{oblique_image,gfx.image_full_range(sampled_desc),.Read,.Sampled};dark_read:=gfx.Image_Access{dark_image,gfx.image_full_range(depth_desc),.Read,.Sampled}
    phase_image_pairs:[120][2]gfx.Image_Binding;phase_image_accesses:[120][2][1]gfx.Image_Access
    for &phase,i in phases { sample:=samples[i%10];phase_image_accesses[i][0][0]=oblique_read if sample.textured else normal_access;phase_image_accesses[i][1][0]=dark_read if sample.shadowed else depth_read;phase_image_pairs[i]={{1,1,{.Fragment},phase_image_accesses[i][0][:]},{4,1,{.Fragment},phase_image_accesses[i][1][:]}};phase.images=phase_image_pairs[i][:] }
    sampler,sampler_error:=ops.create_sampler(renderer,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1});assert(sampler_error==.None)
    comparison,comparison_error:=ops.create_sampler(renderer,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1,comparison=true,compare=.Less_Equal});assert(comparison_error==.None)
    samplers:=[6]gfx.Sampler_Binding{{1,5,{.Fragment},sampler},{1,6,{.Fragment},sampler},{1,7,{.Fragment},sampler},{1,8,{.Fragment},sampler},{1,9,{.Fragment},sampler},{4,2,{.Fragment},comparison}}
    output_desc:=gfx.Texture_Desc{120,1,1,1,.RGBA16_Float,{.Color_Attachment,.Transfer_Source},1}
    output,output_error:=ops.create_texture(renderer,output_desc);assert(output_error==.None);append(&texture_owners,output)
    output_image,_:=gfx.graph_image(&graph,output_desc,{},false,false);append(&textures,gfx.Texture_Input{output_image,output})
    color:=gfx.Image_Access{output_image,gfx.image_full_range(output_desc),.Write,.Color_Attachment}
    pass,_:=gfx.graph_pass(&graph,"production-model-fullshader-brdf-120",.Graphics,accesses[:],images={white_access,normal_access,depth_read,oblique_read,dark_read,color})
    packet_error:=gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{color,.Clear,.Store,{}}},buffers=bindings[:],images=image_bindings[:],samplers=samplers[:],phases=phases[:]});if packet_error!=.None { fmt.println("complete lighting graph packet",packet_error);assert(false) }
    capture_desc:=gfx.Buffer_Desc{960,{.Transfer_Destination,.Readback},.CPU_Visible};zero:[960]byte
    capture,capture_error:=ops.create_buffer(renderer,capture_desc,zero[:]);assert(capture_error==.None)
    captured,_:=gfx.graph_buffer(&graph,capture_desc,false,true);append(&inputs,gfx.Buffer_Input{captured,capture})
    copy_pass,_:=gfx.graph_pass(&graph,"capture-production-hdr-materials",.Transfer,{{captured,{0,960},.Write,.Transfer_Destination}},images={{output_image,gfx.image_full_range(output_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{output_image,{0,0,0,0,120,1,.Color,0,1,0,0},captured,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=ops.acquire(renderer);assert(acquire_error==.None)
    submission,gpu_error,submit_packet_error:=ops.submit(renderer,token,&graph,&plan,inputs[:],textures[:]);if gpu_error!=.None || submit_packet_error!=.None { fmt.println("BRDF recording failed",backend,gpu_error,submit_packet_error);assert(false) }
    assert(ops.destroy_pipeline(renderer,pipeline)==.None && ops.destroy_sampler(renderer,sampler)==.None && ops.destroy_sampler(renderer,comparison)==.None)
    for owner in buffer_owners { assert(ops.destroy_buffer(renderer,owner)==.None) };for owner in texture_owners { assert(ops.destroy_texture(renderer,owner)==.None) }
    assert(ops.wait(renderer,submission)==.None)
    verify_production_capture(renderer,reads,submission,120,backend)
    pixels:[480]u16;assert(reads.read_buffer(renderer,capture,0,mem.slice_to_bytes(pixels[:]))==.None);assert(ops.destroy_buffer(renderer,capture)==.None)
    largest_error:f64;checked:=0
    for affine,transform_index in transforms { for path in 0..<4 { for sample,index in samples {
        pixel_index:=(transform_index*4+path)*10+index;expected:=complete_reference(affine,sample)
        for channel in 0..<3 {
            actual:=f64(f32(transmute(f16)pixels[pixel_index*4+channel]));tolerance:=.00005+abs(expected[channel])*.0006
            if math.is_nan(actual) || math.is_inf(actual) || abs(actual-expected[channel])>tolerance { fmt.println("Complete lighting mismatch",backend,"transform",transform_index,"path",path,"case",index,"channel",channel,"gpu",actual,"reference",expected[channel],"tolerance",tolerance);assert(false) }
            largest_error=max(largest_error,abs(actual-expected[channel]));checked+=1
        }
        assert(pixels[pixel_index*4+3]==0x3c00)
        if index==9 { neutral_index:=(transform_index*4+path)*10+1;for channel in 0..<4 { assert(pixels[pixel_index*4+channel]==pixels[neutral_index*4+channel],"missing normal-mapped_normal scale0/1 changed actual affine material pixels") } }
    } } }
    fmt.println("Complete production lighting PASS",backend,"3 affine matrices x4 real baked/live/skin/composed paths x10 light cases",checked,"f64 RGB channels, maximum absoluteerror",largest_error,"shadow-depth comparisons, two pointlights, ambientAO, signed normalscale, HDR emission")
    return pixels
}
