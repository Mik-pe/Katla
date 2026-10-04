#+build darwin, arm64
//! Material sampling and coverage retain immutable state independently of public handles.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:mem"
import "core:strings"
import "core:fmt"

@(private="package")
Material_Sampling_Params :: struct { uv:[2]f32,lod,padding:f32 }

@(test)
test_native_material_sampler_phases_unorm16_coverage_and_pipeline_reuse :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer assert(renderer_destroy(&r)==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
vertex float4 fullscreen(uint i [[vertex_id]]) { const float2 p[3]={float2(-1,-1),float2(3,-1),float2(-1,3)}; return float4(p[i],0,1); }
struct Params { float2 uv; float lod; float padding; };
fragment float4 material(texture2d<float> image [[texture(0)]], sampler policy [[sampler(0)]], constant Params& params [[buffer(0)]]) { return image.sample(policy,params.uv,level(params.lod)); }`
    owned_source:=strings.clone(shader)
    descriptor:=gfx.Graphics_Desc{vertex_entry="fullscreen",fragment_entry="material",vertex_metal_entry="fullscreen",fragment_metal_entry="material",vertex_metal_source=owned_source,fragment_metal_source=owned_source,
        vertex_sizes_index=-1,fragment_sizes_index=-1,buffers={{group=2,slot=0,stages={.Fragment},usage=.Uniform,vertex_index=-1,fragment_index=0,vertex_size_index=-1,fragment_size_index=-1,mode=.Read,minimum_size=16}},
        images={{group=2,slot=1,stages={.Fragment},usage=.Sampled,vertex_index=-1,fragment_index=0,array_count=1,mode=.Read}},samplers={{group=2,slot=2,stages={.Fragment},vertex_index=-1,fragment_index=0}},colors={{format=.RGBA16_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    first,error:=create_graphics_pipeline(&r,descriptor); assert(error==.None)
    first_owner,_:=gfx.storage_get(&r.graphics,first); native_owner:=first_owner^
    delete(owned_source); descriptor.vertex_metal_source=shader; descriptor.fragment_metal_source=shader
    reused,reused_error:=create_graphics_pipeline(&r,descriptor); assert(reused_error==.None && reused!=first)
    reused_owner,_:=gfx.storage_get(&r.graphics,reused); testing.expect(t,reused_owner^==native_owner && len(r.graphics_cache)==1)
    descriptor.colors[0].blend_enabled=true; descriptor.colors[0].source_color=.Source_Alpha; descriptor.colors[0].destination_color=.One_Minus_Source_Alpha; descriptor.colors[0].source_alpha=.One; descriptor.colors[0].destination_alpha=.One_Minus_Source_Alpha
    blended,blend_error:=create_graphics_pipeline(&r,descriptor); assert(blend_error==.None)
    blend_owner,_:=gfx.storage_get(&r.graphics,blended); testing.expect(t,blend_owner^!=native_owner && len(r.graphics_cache)==2)
    descriptor.colors[0]={format=.RGBA16_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}
    policies:=[5]gfx.Sampler_Desc{
        {min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=1,max_anisotropy=1},
        {min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.Nearest,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=1,max_anisotropy=1},
        {min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Repeat,address_v=.Repeat,address_w=.Repeat,max_lod=1,max_anisotropy=1},
        {min_filter=.Linear,mag_filter=.Linear,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=1,max_anisotropy=1},
        {min_filter=.Linear,mag_filter=.Linear,mip_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=1,max_anisotropy=1},
    }
    samplers:[5]gfx.Sampler_Handle
    for policy,i in policies { samplers[i],error=create_sampler(&r,policy); assert(error==.None) }
    bad:=policies[0]; bad.max_anisotropy=4; _,bad_error:=create_sampler(&r,bad); testing.expect_value(t,bad_error,gfx.Gpu_Error.Invalid_Range)
    repeated,repeated_error:=create_sampler(&r,policies[0]); assert(repeated_error==.None && repeated!=samplers[0])
    original_sampler,_:=gfx.storage_get(&r.samplers,samplers[0]); sampler_owner:=original_sampler^
    repeated_sampler,_:=gfx.storage_get(&r.samplers,repeated); testing.expect(t,repeated_sampler^==sampler_owner && sampler_owner.desc.max_lod==0 && len(r.sampler_cache)==5)
    source_desc:=gfx.Texture_Desc{2,2,2,1,.RGBA16_Unorm,{.Sampled,.Transfer_Destination},1}
    source,source_error:=create_texture(&r,source_desc); assert(source_error==.None)
    base:=[16]u16{33333,0,0,32768,0,65535,0,49152,0,0,65535,16384,65535,65535,65535,65535}
    mip:=[4]u16{65535,65535,0,32768}
    assert(upload_texture(&r,source,{0,0,0,0,2,2,.Color,0,1,0,0},mem.slice_to_bytes(base[:]))==.None)
    assert(upload_texture(&r,source,{1,0,0,0,1,1,.Color,0,1,0,0},mem.slice_to_bytes(mip[:]))==.None)
    alternate,alternate_error:=create_texture(&r,source_desc); assert(alternate_error==.None)
    alternate_base:=[16]u16{50000,1000,2000,65535,50000,1000,2000,65535,50000,1000,2000,65535,50000,1000,2000,65535}
    assert(upload_texture(&r,alternate,{0,0,0,0,2,2,.Color,0,1,0,0},mem.slice_to_bytes(alternate_base[:]))==.None)
    assert(upload_texture(&r,alternate,{1,0,0,0,1,1,.Color,0,1,0,0},mem.slice_to_bytes(alternate_base[:4]))==.None)
    output_desc:=gfx.Texture_Desc{64,8,1,1,.RGBA16_Unorm,{.Color_Attachment,.Transfer_Source},1}
    output,output_error:=create_texture(&r,output_desc); assert(output_error==.None)
    capture_desc:=gfx.Buffer_Desc{4096,{.Transfer_Destination,.Readback},.CPU_Visible}; capture,capture_error:=create_buffer(&r,capture_desc); assert(capture_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    image,_:=gfx.graph_image(&graph,source_desc,{.Transfer_Destination,.Shader_Read,true},true,false)
    alternate_image,_:=gfx.graph_image(&graph,source_desc,{.Transfer_Destination,.Shader_Read,true},true,false)
    target,_:=gfx.graph_image(&graph,output_desc,{},false,false); buffer,_:=gfx.graph_buffer(&graph,capture_desc,false,true)
    read:=gfx.Image_Access{image,gfx.image_full_range(source_desc),.Read,.Sampled}; write:=gfx.Image_Access{target,gfx.image_full_range(output_desc),.Write,.Color_Attachment}
    alternate_read:=gfx.Image_Access{alternate_image,gfx.image_full_range(source_desc),.Read,.Sampled}
    image_override:=[1]gfx.Image_Binding{{2,1,{.Fragment},{alternate_read}}}
    pass,_:=gfx.graph_pass(&graph,"independent-material-sampler-phases",.Graphics,nil,images={read,alternate_read,write})
    params:=[8]Material_Sampling_Params{{{.25,.25},1,0},{{.25,.25},1,0},{{1.25,.25},0,0},{{1.25,.25},0,0},{{.5,.5},0,0},{{.5,.5},.5,0},{{.25,.25},1,0},{{.25,.25},1,0}}
    selections:=[8]int{0,1,1,2,3,4,1,0}; phases:[8]gfx.Render_Phase; constants:[8][1]gfx.Constant_Binding; overrides:[8][1]gfx.Sampler_Binding
    for _,i in params {
        constants[i][0]={group=2,slot=0,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(params[i:i+1])}
        overrides[i][0]={2,2,{.Fragment},samplers[selections[i]]}
        phases[i]={pipeline=reused,constants=constants[i][:],draws={gfx.Draw{3,1,0,0}},scissor={true,u32(i*8),0,8,8},samplers=overrides[i][:]}
        if i==2 || i==6 { phases[i].samplers=nil }
        if i==6 { phases[i].images=image_override[:] }
        if i==7 { phases[i].pipeline=blended }
    }
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{write,.Clear,.Store,{0,0,1,.25}}},images={{2,1,{.Fragment},{read}}},samplers={{2,2,{.Fragment},samplers[1]}},phases=phases[:]})==.None)
    copied,_:=gfx.graph_pass(&graph,"capture-material-sampling",.Transfer,{{buffer,{0,4096},.Write,.Transfer_Destination}},images={{target,gfx.image_full_range(output_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copied,gfx.Copy_Image_Buffer{target,{0,0,0,0,64,8,.Color,0,1,0,0},buffer,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,_:=acquire(&r); submission,native_error,packet_error:=submit(&r,token,&graph,&plan,{{buffer,capture}},{{image,source},{alternate_image,alternate},{target,output}}); assert(native_error==.None && packet_error==.None)
    assert(destroy_graphics_pipeline(&r,first)==.None && destroy_graphics_pipeline(&r,reused)==.None && destroy_graphics_pipeline(&r,blended)==.None)
    _,first_live:=query_graphics(&r,first); testing.expect(t,!first_live)
    for sampler in samplers { assert(destroy_sampler(&r,sampler)==.None) }; assert(destroy_sampler(&r,repeated)==.None)
    pending_copy,pending_copy_error:=create_graphics_pipeline(&r,descriptor); assert(pending_copy_error==.None)
    pending_owner,_:=gfx.storage_get(&r.graphics,pending_copy); testing.expect(t,pending_owner^==native_owner)
    pending_sampler,pending_sampler_error:=create_sampler(&r,policies[0]); assert(pending_sampler_error==.None)
    pending_sampler_owner,_:=gfx.storage_get(&r.samplers,pending_sampler); testing.expect(t,pending_sampler_owner^==sampler_owner)
    assert(destroy_texture(&r,source)==.None && destroy_texture(&r,alternate)==.None && destroy_texture(&r,output)==.None)
    assert(wait(&r,submission)==.None)
    pixels:[2048]u16; assert(read_buffer(&r,capture,0,mem.slice_to_bytes(pixels[:]))==.None)
    expected:=[8][4]int{{33333,0,0,32768},{65535,65535,0,32768},{0,65535,0,49152},{33333,0,0,32768},{24717,32768,32768,40960},{45126,49151,16384,36864},{50000,1000,2000,65535},{16667,0,32767,40960}}
    for y in 0..<8 { for x in 0..<64 { for channel in 0..<4 {
        actual:=int(pixels[(y*64+x)*4+channel]); wanted:=expected[x/8][channel]; testing.expect(t,actual>=wanted-2 && actual<=wanted+2)
    } } }
    testing.expect(t,len(r.graphics_cache)==1 && len(r.sampler_cache)==1)
    assert(destroy_graphics_pipeline(&r,pending_copy)==.None && destroy_sampler(&r,pending_sampler)==.None && destroy_buffer(&r,capture)==.None)
    testing.expect(t,len(r.graphics_cache)==0 && len(r.sampler_cache)==0)
    fmt.println("Metal material sampling: 512 RGBA16UNORM pixels, disabled/nearest/linear mip policies, spatial filtering/repeat/clamp, phase image/sampler override+shared restoration, source-over coverage, exact-content PSO reuse and pending public-owner removal PASS")
}
