#+build darwin, arm64
//! Real production model vertex/fragment stages render the numerical material acceptance matrix.
package main

import render "../../app/render"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import shader "../../gfx/shader"
import km "../../math"
import "core:mem"
import "core:fmt"
import "core:math"
import "core:os"
import NS "core:sys/darwin/Foundation"

Read_Ops :: struct($R:typeid) {
    create_texture:proc(^R,gfx.Texture_Desc,[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error),
    read_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    capture:proc(^R,u64,mem.Allocator)->(gfx.Capture_Snapshot,bool),
}
input_buffer :: proc(renderer:^$R,ops:render.GPU_Ops(R),graph:^gfx.Graph,owners:^[dynamic]gfx.Buffer_Handle,inputs:^[dynamic]gfx.Buffer_Input,data:[]byte)->gfx.Buffer_Access {
    desc:=gfx.Buffer_Desc{u64(len(data)),{.Storage},.GPU_Private}
    handle,error:=ops.create_buffer(renderer,desc,data);assert(error==.None);append(owners,handle)
    resource,graph_error:=gfx.graph_buffer(graph,desc,true,false);assert(graph_error==.None);append(inputs,gfx.Buffer_Input{resource,handle})
    return {resource,{0,desc.size},.Read,.Storage}
}
exercise :: proc(renderer:^$R,ops:render.GPU_Ops(R),reads:Read_Ops(R),model:^render.Model_Shader,backend:string)->[960]u16 {
    graph:gfx.Graph;gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    buffer_owners:=make([dynamic]gfx.Buffer_Handle);defer delete(buffer_owners)
    inputs:=make([dynamic]gfx.Buffer_Input);defer delete(inputs)
    texture_owners:=make([dynamic]gfx.Texture_Handle);defer delete(texture_owners)
    textures:=make([dynamic]gfx.Texture_Input);defer delete(textures)
    descriptor:=model.descriptors[1];descriptor.depth={}
    pipeline,pipeline_error:=ops.create_pipeline(renderer,descriptor);assert(pipeline_error==.None)
    samples:=parameter_cases();objects:[240]render.Model_Object;frames:[240]render.Frame_Data
    phases:[240]gfx.Render_Phase;phase_constants:[240][1]gfx.Constant_Binding;draws:[240][1]gfx.Draw_Op
    identity:=km.identity(km.Mat4)
    for sample,index in samples { for mapped in 0..<2 {
        i:=index*2+mapped
        objects[i]={model=identity,normal_model=identity,base_color={.8,.3,.1,1},factors={sample.metallic,sample.roughness,1,1},flags={0,0,0,u32(mapped)}}
        frames[i]={view_projection=identity,camera_position={sample.view[0],sample.view[1],sample.view[2],1},light_direction={-sample.light[0],-sample.light[1],-sample.light[2],0},light_color={1,.8,.6,1},ambient={0,0,0,-1 if backend=="vulkan" else 1}}
        frames[i].view_projection[3][2]=.5
        phase_constants[i][0]={group=0,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(frames[i:i+1])}
        draws[i][0]=gfx.Draw{3,1,0,u32(i)}
        phases[i]={pipeline=pipeline,constants=phase_constants[i][:],viewport={true,f64(i),0,1,1,0,1},scissor={true,u32(i),0,1,1},draws=draws[i][:]}
    } }
    vertices:[3]render.Model_Vertex
    for &vertex,i in vertices {
        positions:=[3]km.Vec4{{-1,-1,0,1},{3,-1,0,1},{-1,3,0,1}}
        vertex={position=positions[i],normal={0,0,1,0},tangent={1,0,0,1},color={1,1,1,1}}
        for &uv in vertex.uvs { uv={.5,.5,1,0} }
    }
    object_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(objects[:]))
    vertex_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(vertices[:]))
    points:[256]render.Point_Light_GPU;indices:[2048]u32;counts:[16]u32;shadow:[1]render.Shadow_Frame
    point_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(points[:]))
    index_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(indices[:]))
    count_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(counts[:]))
    shadow_access:=input_buffer(renderer,ops,&graph,&buffer_owners,&inputs,mem.slice_to_bytes(shadow[:]))
    bindings:=[6]gfx.Stage_Buffer_Binding{{0,1,{.Vertex,.Fragment},object_access},{0,2,{.Vertex},vertex_access},{3,1,{.Fragment},point_access},{3,2,{.Fragment},index_access},{3,3,{.Fragment},count_access},{4,0,{.Fragment},shadow_access}}
    accesses:=[6]gfx.Buffer_Access{object_access,vertex_access,point_access,index_access,count_access,shadow_access}
    lighting:=[1]render.Lighting_Frame{{view=identity,inverse_view_projection=identity,viewport={240,1,15,1},settings={}}}
    lighting_constant:=[1]gfx.Constant_Binding{{group=3,slot=0,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(lighting[:])}}
    white:=[4]f16{1,1,1,1};neutral:=[4]f16{.5,.5,1,1}
    sampled_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA16_Float,{.Sampled,.Transfer_Destination},1}
    white_handle,white_error:=reads.create_texture(renderer,sampled_desc,mem.slice_to_bytes(white[:]));assert(white_error==.None);append(&texture_owners,white_handle)
    neutral_handle,normal_error:=reads.create_texture(renderer,sampled_desc,mem.slice_to_bytes(neutral[:]));assert(normal_error==.None);append(&texture_owners,neutral_handle)
    depth_desc:=gfx.Texture_Desc{1,1,1,1,.D32_Float,{.Sampled,.Depth_Attachment},1}
    depth_handle,depth_error:=ops.create_texture(renderer,depth_desc);assert(depth_error==.None);append(&texture_owners,depth_handle)
    white_image,_:=gfx.graph_image(&graph,sampled_desc,{.Shader_Read,.Shader_Read,true},true,false);normal_image,_:=gfx.graph_image(&graph,sampled_desc,{.Shader_Read,.Shader_Read,true},true,false)
    depth_image,_:=gfx.graph_image(&graph,depth_desc,{},false,false)
    depth_write:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    clear_depth,_:=gfx.graph_pass(&graph,"initialize-disabled-shadow-atlas",.Graphics,nil,images={depth_write})
    assert(gfx.graph_set_packet(&graph,clear_depth,gfx.Render{depth={enabled=true,access=depth_write,load=.Clear,store=.Store,clear_depth=1}})==.None)
    append(&textures,gfx.Texture_Input{white_image,white_handle},gfx.Texture_Input{normal_image,neutral_handle},gfx.Texture_Input{depth_image,depth_handle})
    white_access:=gfx.Image_Access{white_image,gfx.image_full_range(sampled_desc),.Read,.Sampled};normal_access:=gfx.Image_Access{normal_image,gfx.image_full_range(sampled_desc),.Read,.Sampled};depth_read:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Read,.Sampled}
    image_bindings:=[6]gfx.Image_Binding{{1,0,{.Fragment},{white_access}},{1,1,{.Fragment},{normal_access}},{1,2,{.Fragment},{white_access}},{1,3,{.Fragment},{white_access}},{1,4,{.Fragment},{white_access}},{4,1,{.Fragment},{depth_read}}}
    sampler,sampler_error:=ops.create_sampler(renderer,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1});assert(sampler_error==.None)
    comparison,comparison_error:=ops.create_sampler(renderer,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1,comparison=true,compare=.Less_Equal});assert(comparison_error==.None)
    samplers:=[6]gfx.Sampler_Binding{{1,5,{.Fragment},sampler},{1,6,{.Fragment},sampler},{1,7,{.Fragment},sampler},{1,8,{.Fragment},sampler},{1,9,{.Fragment},sampler},{4,2,{.Fragment},comparison}}
    output_desc:=gfx.Texture_Desc{240,1,1,1,.RGBA16_Float,{.Color_Attachment,.Transfer_Source},1}
    output,output_error:=ops.create_texture(renderer,output_desc);assert(output_error==.None);append(&texture_owners,output)
    output_image,_:=gfx.graph_image(&graph,output_desc,{},false,false);append(&textures,gfx.Texture_Input{output_image,output})
    color:=gfx.Image_Access{output_image,gfx.image_full_range(output_desc),.Write,.Color_Attachment}
    pass,_:=gfx.graph_pass(&graph,"production-model-fullshader-brdf-120",.Graphics,accesses[:],images={white_access,normal_access,depth_read,color})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{color,.Clear,.Store,{}}},buffers=bindings[:],images=image_bindings[:],samplers=samplers[:],constants=lighting_constant[:],phases=phases[:]})==.None)
    capture_desc:=gfx.Buffer_Desc{1920,{.Transfer_Destination,.Readback},.CPU_Visible};zero:[1920]byte
    capture,capture_error:=ops.create_buffer(renderer,capture_desc,zero[:]);assert(capture_error==.None)
    captured,_:=gfx.graph_buffer(&graph,capture_desc,false,true);append(&inputs,gfx.Buffer_Input{captured,capture})
    copy_pass,_:=gfx.graph_pass(&graph,"capture-production-hdr-materials",.Transfer,{{captured,{0,1920},.Write,.Transfer_Destination}},images={{output_image,gfx.image_full_range(output_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{output_image,{0,0,0,0,240,1,.Color,0,1,0,0},captured,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=ops.acquire(renderer);assert(acquire_error==.None)
    submission,gpu_error,packet_error:=ops.submit(renderer,token,&graph,&plan,inputs[:],textures[:]);if gpu_error!=.None || packet_error!=.None { fmt.println("BRDF recording failed",backend,gpu_error,packet_error);assert(false) }
    assert(ops.destroy_pipeline(renderer,pipeline)==.None && ops.destroy_sampler(renderer,sampler)==.None && ops.destroy_sampler(renderer,comparison)==.None)
    for owner in buffer_owners { assert(ops.destroy_buffer(renderer,owner)==.None) };for owner in texture_owners { assert(ops.destroy_texture(renderer,owner)==.None) }
    assert(ops.wait(renderer,submission)==.None)
    verify_production_capture(renderer,reads,submission,240,backend)
    pixels:[960]u16;assert(reads.read_buffer(renderer,capture,0,mem.slice_to_bytes(pixels[:]))==.None);assert(ops.destroy_buffer(renderer,capture)==.None)
    largest_relative:f64;checked:=0
    for sample,index in samples {
        expected:=reference(sample)
        for channel in 0..<3 {
            plain:=pixels[index*8+channel];mapped:=pixels[index*8+4+channel]
            actual:=f64(f32(transmute(f16)plain));quantized:=f64(f32(f16(expected[channel])))
            tolerance:=max(abs(quantized)*.002,2e-7)
            if math.is_nan(actual) || math.is_inf(actual) || abs(actual-quantized)>tolerance { fmt.println("BRDF mismatch",backend,index,sample,"channel",channel,"gpu",actual,"reference",expected[channel],"quantized",quantized);assert(false) }
            assert(plain==mapped,"exact neutral normal map changed the production material result")
            if abs(quantized)>=.0001 { largest_relative=max(largest_relative,abs(actual-quantized)/abs(quantized)) };checked+=1
        }
        assert(pixels[index*8+3]==0x3c00 && pixels[index*8+7]==0x3c00)
    }
    fmt.println("Production model BRDF PASS",backend,"120 cases x exact neutral/no-normal pairs, f64 reference channels",checked,"maximum half-reference relativeerror above1e-4",largest_relative,"all accepted parents removed while pending")
    return pixels
}
main :: proc() {
    assert(len(os.args)==3)
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0);mem.tracking_allocator_destroy(&tracker) }
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer assert(shader.compiler_destroy(&compiler)==.None)
    model,error:=render.model_shader_compile(&compiler);assert(error==.None);defer render.model_shader_destroy(&model)
    metal_pixels:[960]u16;metal_lighting:[480]u16
    {
        renderer:metal.Renderer;assert(metal.renderer_init(&renderer)==.None);defer assert(metal.renderer_destroy(&renderer)==.None)
        assert(metal.capture_enable(&renderer,true)==.None)
        ops:=render.GPU_Ops(metal.Renderer){create_pipeline=metal.create_graphics_pipeline,destroy_pipeline=metal.destroy_graphics_pipeline,create_buffer=metal.create_buffer_with_data,destroy_buffer=metal.destroy_buffer,create_texture=metal.create_texture,destroy_texture=metal.destroy_texture,acquire=metal.acquire,abort=metal.abort,submit=metal.submit,wait=metal.wait,create_sampler=metal.create_sampler,destroy_sampler=metal.destroy_sampler}
        metal_pixels=exercise(&renderer,ops,Read_Ops(metal.Renderer){metal.create_texture_with_data,metal.read_buffer,metal.capture_snapshot},&model,"metal")
        metal_lighting=exercise_lighting(&renderer,ops,Read_Ops(metal.Renderer){metal.create_texture_with_data,metal.read_buffer,metal.capture_snapshot},&model,"metal")
    }
    {
        renderer:vulkan.Renderer;assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None);defer { assert(renderer.validation_errors==0);assert(vulkan.renderer_destroy(&renderer)==.None) }
        assert(vulkan.capture_enable(&renderer,true)==.None)
        ops:=render.GPU_Ops(vulkan.Renderer){create_pipeline=vulkan.create_graphics_pipeline,destroy_pipeline=vulkan.destroy_graphics_pipeline,create_buffer=vulkan.create_buffer_with_data,destroy_buffer=vulkan.destroy_buffer,create_texture=vulkan.create_texture,destroy_texture=vulkan.destroy_texture,acquire=vulkan.acquire,abort=vulkan.abort,submit=vulkan.submit,wait=vulkan.wait,create_sampler=vulkan.create_sampler,destroy_sampler=vulkan.destroy_sampler}
        vulkan_pixels:=exercise(&renderer,ops,Read_Ops(vulkan.Renderer){vulkan.create_texture_with_data,vulkan.read_buffer,vulkan.capture_snapshot},&model,"vulkan")
        vulkan_lighting:=exercise_lighting(&renderer,ops,Read_Ops(vulkan.Renderer){vulkan.create_texture_with_data,vulkan.read_buffer,vulkan.capture_snapshot},&model,"vulkan")
        for bits,i in metal_lighting { a,b:=f64(f32(transmute(f16)bits)),f64(f32(transmute(f16)vulkan_lighting[i]));assert(abs(a-b)<=max(max(a,b)*.002,2e-7),"accepted Metal/Vulkan complete lighting values diverged") }
        for bits,i in metal_pixels { a,b:=f64(f32(transmute(f16)bits)),f64(f32(transmute(f16)vulkan_pixels[i]));assert(abs(a-b)<=max(max(a,b)*.002,2e-7),"accepted Metal/Vulkan production HDR values diverged") }
    }
}

verify_production_capture :: proc(renderer:^$R,reads:Read_Ops(R),submission:gfx.Submission,phases:int,backend:string) {
    snapshot,exists:=reads.capture(renderer,submission.id,context.allocator);assert(exists);defer gfx.capture_snapshot_destroy(&snapshot)
    assert(snapshot.feedback==.Completed && snapshot.submission==submission.id && snapshot.generation==submission.token.generation)
    findings:=gfx.capture_compare(&snapshot);defer delete(findings)
    if len(findings)>0 { fmt.println("Production material capture divergence",backend,findings);assert(false) }
    pipelines,images,samplers,allocations,tables:int
    for event in snapshot.events { #partial switch event.kind {
    case .Bind_Pipeline:pipelines+=1
    case .Bind_Image:images+=1
    case .Bind_Sampler:samplers+=1
    case .Allocation:allocations+=1
    case .Argument_Table:tables+=1
    } }
    assert(pipelines==phases && images>=phases*6 && samplers>=phases*6 && allocations>=8)
    if backend=="metal" { assert(tables==phases*2) }
    fmt.println("Production material native capture PASS",backend,"phases",phases,"observed events",len(snapshot.events),"independent expectations",len(snapshot.expected),"zero divergence; actual completed feedback")
}
