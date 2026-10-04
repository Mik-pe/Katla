#+test
package gfx
import "core:testing"

@(test)
test_sampler_base_mip_and_portable_anisotropy_policy :: proc(t:^testing.T) {
    desc:=Sampler_Desc{min_filter=.Linear,mag_filter=.Linear,mip_filter=.None,min_lod=3,max_lod=7,max_anisotropy=16}
    normalized,valid:=sampler_desc_normalize(desc);testing.expect(t,valid)
    testing.expect_value(t,normalized.min_lod,f32(0));testing.expect_value(t,normalized.max_lod,f32(0))
    desc.min_filter=.Nearest;_,valid=sampler_desc_normalize(desc);testing.expect(t,!valid)
    desc.max_anisotropy=1;_,valid=sampler_desc_normalize(desc);testing.expect(t,valid)
    desc.max_anisotropy=17;_,valid=sampler_desc_normalize(desc);testing.expect(t,!valid)
    desc.max_anisotropy=1;desc.mip_filter=cast(Mip_Filter)9;_,valid=sampler_desc_normalize(desc);testing.expect(t,!valid)
    desc.mip_filter=.Linear;desc.max_lod=2;_,valid=sampler_desc_normalize(desc);testing.expect(t,!valid)
    testing.expect_value(t,texture_pixel_size(.RGBA16_Unorm),u32(8))
    testing.expect_value(t,vertex_format_size(.Uint8x4),u32(4));testing.expect_value(t,vertex_format_size(.Uint16x4),u32(8));testing.expect_value(t,vertex_format_size(.Unorm16x4),u32(8))
}

Sampler_Phase_Fixture :: struct { graphics:Graphics_Fixture, samplers:[2]Sampler_Handle }
sampler_phase_texture :: proc(state:rawptr,h:Texture_Handle)->(Texture_Info,bool) { f:=cast(^Sampler_Phase_Fixture)state;return fixture_texture(&f.graphics,h) }
sampler_phase_graphics :: proc(state:rawptr,h:Graphics_Pipeline_Handle)->(Graphics_Info,bool) { f:=cast(^Sampler_Phase_Fixture)state;return fixture_graphics(&f.graphics,h) }
sampler_phase_query :: proc(state:rawptr,h:Sampler_Handle)->(Sampler_Info,bool) {
    f:=cast(^Sampler_Phase_Fixture)state
    for handle in f.samplers { if handle==h { return {desc={max_anisotropy=1},identity=f},true } }
    return {},false
}
@(test)
test_phase_samplers_restore_defaults_and_freeze_each_override :: proc(t:^testing.T) {
    graph:Graph;graph_init(&graph);defer graph_destroy(&graph)
    f:Sampler_Phase_Fixture;f.graphics.desc={8,8,1,1,.RGBA8_Unorm,{.Color_Attachment},1};f.graphics.texture={&f,0,0};f.graphics.pipeline={&f,0,0};f.samplers={{&f,0,1},{&f,1,1}}
    formats:=[1]Texture_Format{.RGBA8_Unorm};requirements:=[1]Sampler_Requirement{{2,7,{.Fragment},false}}
    f.graphics.info={colors=formats[:],samplers=requirements[:],supported_draws={.Generated}}
    image,_:=graph_image(&graph,f.graphics.desc,{},false,true);access:=Image_Access{image,image_full_range(f.graphics.desc),.Write,.Color_Attachment}
    pass,_:=graph_pass(&graph,"independent phase sampling",.Graphics,nil,images={access})
    overrides:=[1]Sampler_Binding{{2,7,{.Fragment},f.samplers[1]}}
    packet:=Render{colors={{access,.Clear,.Store,{}}},samplers={{2,7,{.Fragment},f.samplers[0]}},phases={{pipeline=f.graphics.pipeline,draws={Draw{3,1,0,0}},samplers=overrides[:]},{pipeline=f.graphics.pipeline,draws={Draw{3,1,0,0}}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None);overrides[0].handle={}
    plan,error:=graph_compile(&graph);testing.expect_value(t,error,Graph_Error.None);defer compiled_graph_destroy(&plan)
    query:=Graphics_Query{state=&f,texture=sampler_phase_texture,pipeline=sampler_phase_graphics,sampler=sampler_phase_query}
    prepared,failure:=graph_prepare(&graph,&plan,nil,{},textures={{image,f.graphics.texture}},graphics=query);testing.expect_value(t,failure,Packet_Error.None);defer prepared_graph_destroy(&prepared)
    frozen:=prepared.passes[0].packet.(Render);testing.expect_value(t,len(frozen.samplers),0)
    testing.expect_value(t,frozen.phases[0].samplers[0].handle,f.samplers[1]);testing.expect_value(t,frozen.phases[1].samplers[0].handle,f.samplers[0])
    packet.phases[0].samplers={{2,7,{.Vertex},f.samplers[1]}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    rejected,rejection:=graph_prepare(&graph,&plan,nil,{},textures={{image,f.graphics.texture}},graphics=query);defer prepared_graph_destroy(&rejected)
    testing.expect_value(t,rejection,Packet_Error.Invalid_Binding)
    testing.expect_value(t,frozen.phases[0].samplers[0].handle,f.samplers[1])
    packet.phases[0].samplers={{2,7,{.Fragment},f.samplers[1]},{2,7,{.Fragment},f.samplers[0]}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.Invalid_Binding)
}
