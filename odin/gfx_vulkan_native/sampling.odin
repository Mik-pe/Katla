//! Real sampled pixels verify immutable pipeline reuse, draw-local policies and accepted upload ownership.
package main
import gfx "../gfx"
import gpu "../gfx/vulkan"
import vk "vendor:vulkan"
import "core:mem"
import "core:fmt"

sampling_upload_not_ready :: proc "system"(_:vk.Device,_:vk.Fence)->vk.Result { return .NOT_READY }
sampling_upload_query_failure :: proc "system"(_:vk.Device,_:vk.Fence)->vk.Result { return .ERROR_OUT_OF_HOST_MEMORY }
sampling_upload_rejection :: proc "system"(_:vk.Queue,_:u32,_:[^]vk.SubmitInfo2,_:vk.Fence)->vk.Result { return .ERROR_OUT_OF_HOST_MEMORY }
Sampling_Case :: struct { uv_lod:[4]f32,policy:int,expected:[4]byte }
run_sampling :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    real_query,real_submit:=r.table.GetFenceStatus,r.table.QueueSubmit2
    defer { r.table.GetFenceStatus=real_query;r.table.QueueSubmit2=real_submit }
    r.table.GetFenceStatus=sampling_upload_not_ready
    upload_baseline:=len(r.pending_uploads)
    sampled_desc:=gfx.Texture_Desc{width=4,height=4,mip_levels=3,layers=1,format=.RGBA8_Unorm,usage={.Sampled,.Transfer_Destination},depth=1}
    sampled,error:=gpu.create_texture(r,sampled_desc);assert(error==.None)
    for mip in 0..<3 {
        width,height:=gfx.texture_mip_extent(sampled_desc,u32(mip));pixels:=make([]byte,int(width*height*4))
        for y in 0..<height { for x in 0..<width {
            offset:=int((y*width+x)*4);color:=[4]byte{255,255,255,255}
            if mip==0 { color={255,0,0,255} if x<2 else [4]byte{0,0,255,255} };if mip==1 { color={0,255,0,255} };copy(pixels[offset:offset+4],color[:])
        } }
        upload_error:=gpu.upload_texture(r,sampled,{u32(mip),0,0,0,width,height,.Color,0,1,0,0},pixels);assert(upload_error==.None,fmt.tprintf("mip%d upload %v",mip,upload_error))
        for &byte in pixels { byte=0 };delete(pixels)
    }
    assert(len(r.pending_uploads)==upload_baseline+3)
    texture_baseline:=r.textures.count
    r.table.QueueSubmit2=sampling_upload_rejection
    rejected_pixels:=[4]byte{255,0,0,255}
    rejected_texture,rejected_error:=gpu.create_texture_with_data(r,{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1},rejected_pixels[:])
    assert(rejected_error==.Native_Failure && rejected_texture.owner==nil && r.textures.count==texture_baseline && len(r.pending_uploads)==upload_baseline+3)
    r.table.QueueSubmit2=real_submit
    r.table.GetFenceStatus=sampling_upload_query_failure
    rejected_token,query_error:=gpu.acquire(r);assert(query_error==.Native_Failure && rejected_token.owner==nil && len(r.pending_uploads)==upload_baseline+3)
    r.table.GetFenceStatus=sampling_upload_not_ready
    blue_pixels:=[4]byte{0,0,255,255};blue_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    blue,blue_error:=gpu.create_texture_with_data(r,blue_desc,blue_pixels[:]);assert(blue_error==.None)
    policy_desc:=[6]gfx.Sampler_Desc{
        {address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=2,max_anisotropy=1},
        {address_u=.Repeat,address_v=.Repeat,address_w=.Repeat,max_lod=2,max_anisotropy=1},
        {address_u=.Mirror_Repeat,address_v=.Mirror_Repeat,address_w=.Mirror_Repeat,max_lod=2,max_anisotropy=1},
        {min_filter=.Linear,mag_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=2,max_anisotropy=1},
        {mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=2,max_anisotropy=1},
        {mip_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=2,max_anisotropy=1},
    }
    policies:[6]gfx.Sampler_Handle
    sampler_baseline:=len(r.sampler_cache)
    for desc,index in policy_desc { policies[index],error=gpu.create_sampler(r,desc);assert(error==.None) }
    duplicate,duplicate_error:=gpu.create_sampler(r,policy_desc[0]);assert(duplicate_error==.None && duplicate!=policies[0] && len(r.sampler_cache)==sampler_baseline+6)
    duplicate_native,_:=gfx.storage_get(&r.samplers,duplicate);original_native,_:=gfx.storage_get(&r.samplers,policies[0]);assert(duplicate_native^==original_native^)
    assert(gpu.destroy_sampler(r,duplicate)==.None)
    normalized,normal_ok:=gpu.graphics_query(r).sampler(r,policies[4]);assert(normal_ok && normalized.desc.min_lod==0 && normalized.desc.max_lod==0)
    invalid,invalid_error:=gpu.create_sampler(r,{max_anisotropy=2});assert(invalid_error==.Invalid_Range && invalid.owner==nil)
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,buffers={{group=0,slot=0,stages={.Fragment},usage=.Uniform,mode=.Read,minimum_size=16}},images={{group=2,slot=1,stages={.Fragment},usage=.Sampled,arrayed=true,sample_type=.Float,mode=.Read,array_count=1}},samplers={{group=2,slot=2,stages={.Fragment}}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    cache_baseline:=len(r.graphics_cache)
    first,first_error:=gpu.create_graphics_pipeline(r,desc);assert(first_error==.None)
    clone:=gfx.graphics_desc_clone(desc);defer gfx.graphics_desc_destroy(&clone)
    second,second_error:=gpu.create_graphics_pipeline(r,clone);assert(second_error==.None && first!=second && len(r.graphics_cache)==cache_baseline+1)
    first_native,_:=gfx.storage_get(&r.graphics,first);second_native,_:=gfx.storage_get(&r.graphics,second);assert(first_native^==second_native^)
    clone.colors[0].write_mask={.Red};variant,variant_error:=gpu.create_graphics_pipeline(r,clone);assert(variant_error==.None && len(r.graphics_cache)==cache_baseline+2)
    variant_native,_:=gfx.storage_get(&r.graphics,variant);assert(variant_native^!=first_native^);assert(gpu.destroy_graphics_pipeline(r,variant)==.None)
    broken:=desc;broken.fragment_entry="missing_entry";missing,missing_error:=gpu.create_graphics_pipeline(r,broken);assert(missing_error==.Invalid_Shader && missing.owner==nil && len(r.graphics_cache)==cache_baseline+1)
    cases:=[14]Sampling_Case{
        {{1.125,.5,0,0},-1,{0,0,255,255}},{{1.125,.5,0,0},1,{255,0,0,255}},{{1.125,.5,0,0},-1,{0,0,255,255}},
        {{1.125,.5,0,0},2,{0,0,255,255}},{{2.125,.5,0,0},2,{255,0,0,255}},{{.5,.5,0,0},3,{128,0,128,255}},
        {{.125,.5,1,0},4,{255,0,0,255}},{{.125,.5,1,0},-1,{0,255,0,255}},{{.125,.5,.5,0},5,{128,128,0,255}},
        {{.125,.5,2,0},-1,{255,255,255,255}},{{-.125,.5,0,0},1,{0,0,255,255}},{{-.125,.5,0,0},2,{255,0,0,255}},
        {{.125,.5,0,0},-1,{0,0,255,255}},{{.125,.5,0,0},-1,{255,0,0,255}},
    }
    target_desc:=gfx.Texture_Desc{u32(len(cases)),4,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    target,target_error:=gpu.create_texture(r,target_desc);assert(target_error==.None)
    output_desc:=gfx.Buffer_Desc{size=u64(len(cases)*4*4),usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc);assert(output_error==.None);defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph;gfx.graph_init(&graph);defer { assert(gpu.release_graph_exports(r,&graph)==.None);gfx.graph_destroy(&graph) }
    source,_:=gfx.graph_image(&graph,sampled_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    alternate,_:=gfx.graph_image(&graph,blue_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    color,_:=gfx.graph_image(&graph,target_desc,{},false,true);destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    sampled_access:=gfx.Image_Access{source,gfx.image_full_range(sampled_desc),.Read,.Sampled};alternate_access:=gfx.Image_Access{alternate,gfx.image_full_range(blue_desc),.Read,.Sampled};color_access:=gfx.Image_Access{color,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    parameters:[14][4]f32;phases:[14]gfx.Render_Phase;constants:[14][1]gfx.Constant_Binding;phase_samplers:[14][1]gfx.Sampler_Binding;draws:[14][1]gfx.Draw_Op
    override_images:=[1]gfx.Image_Binding{{2,1,{.Fragment},{alternate_access}}}
    for fixture,index in cases {
        parameters[index]=fixture.uv_lod
        constants[index][0]={0,0,{.Fragment},.Uniform,mem.slice_to_bytes(parameters[index][:])};draws[index][0]=gfx.Draw{3,1,0,0}
        phases[index]={pipeline=first if index%2==0 else second,scissor={true,u32(index),0,1,4},constants=constants[index][:],draws=draws[index][:]}
        if fixture.policy>=0 { phase_samplers[index][0]={2,2,{.Fragment},policies[fixture.policy]};phases[index].samplers=phase_samplers[index][:] }
        if index==12 { phases[index].images=override_images[:] }
    }
    draw,_:=gfx.graph_pass(&graph,"sampler-policy-phases",.Graphics,nil,images={sampled_access,alternate_access,color_access})
    assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,0}}},images={{2,1,{.Fragment},{sampled_access}}},samplers={{2,2,{.Fragment},policies[0]}},phases=phases[:]})==.None)
    copy_pass,_:=gfx.graph_pass(&graph,"sampler-policy-pixels",.Transfer,{{destination,{0,output_desc.size},.Write,.Transfer_Destination}},images={{color,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{color,{0,0,0,0,target_desc.width,target_desc.height,.Color,0,1,0,0},destination,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r);assert(acquire_error==.None && len(r.pending_uploads)==upload_baseline+4)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{destination,output}},{{source,sampled},{alternate,blue},{color,target}});assert(native_error==.None && packet_error==.None)
    assert(gpu.destroy_texture(r,sampled)==.None);assert(gpu.destroy_texture(r,blue)==.None);assert(gpu.destroy_texture(r,target)==.None)
    for policy in policies { assert(gpu.destroy_sampler(r,policy)==.None) }
    assert(gpu.destroy_graphics_pipeline(r,first)==.None);assert(gpu.destroy_graphics_pipeline(r,second)==.None)
    assert(len(r.graphics_cache)==cache_baseline+1 && len(r.sampler_cache)==sampler_baseline+6)
    assert(gpu.wait(r,submission)==.None)
    actual:=make([]byte,int(output_desc.size));defer delete(actual);assert(gpu.read_buffer(r,output,0,actual)==.None)
    for y in 0..<4 { for fixture,index in cases { for expected,channel in fixture.expected { found:=actual[(y*len(cases)+index)*4+channel];assert(found==expected,fmt.tprintf("sampler case %d channel %d expected %d got %d",index,channel,expected,found)) } } }
    assert(len(r.graphics_cache)==cache_baseline && len(r.sampler_cache)==sampler_baseline && len(r.pending_uploads)==upload_baseline+4)
    r.table.GetFenceStatus=real_query
    flushed,flush_error:=gpu.acquire(r);assert(flush_error==.None && len(r.pending_uploads)==upload_baseline);assert(gpu.abort(r,flushed)==.None)
    fmt.println("Sampling: 56 actual pixels prove clamp/repeat/mirror, spatial/mip filters, disabled explicit LOD, draw image/sampler override restoration; identical native PSOs/policies share immutable owners through pending handle removal; queued upload source bytes retired, delayed fences retained, submit/query failure rollback verified")
}
