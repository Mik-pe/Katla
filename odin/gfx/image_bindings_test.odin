#+test
package gfx

import "core:testing"

Array_Fixture :: struct { descriptions:[3]Texture_Desc, requirement:Image_Binding_Requirement, colors:[1]Texture_Format }
array_fixture_texture :: proc(state:rawptr,handle:Texture_Handle)->(Texture_Info,bool) {
    fixture:=cast(^Array_Fixture)state
    if handle.owner!=fixture || handle.index>=3 || handle.generation!=0 { return {},false }
    return {fixture.descriptions[handle.index],&fixture.descriptions[handle.index]},true
}
array_fixture_graphics :: proc(state:rawptr,handle:Graphics_Pipeline_Handle)->(Graphics_Info,bool) {
    fixture:=cast(^Array_Fixture)state
    if handle.owner!=fixture || handle.index!=0 { return {},false }
    return {images=(cast([^]Image_Binding_Requirement)&fixture.requirement)[:1],colors=fixture.colors[:],supported_draws={.Generated}},true
}
@(test)
test_fixed_image_arrays_own_nested_packets_and_validate_every_element :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    fixture:=Array_Fixture{colors={.RGBA8_Unorm},descriptions={{4,4,1,1,.RGBA8_Unorm,{.Color_Attachment},1},{4,4,1,1,.RGBA8_Unorm,{.Sampled},1},{4,4,1,1,.RGBA8_Unorm,{.Sampled},1}},requirement={group=1,slot=0,stages={.Fragment},usage=.Sampled,mode=.Read,array_count=3}}
    output,_:=graph_image(&graph,fixture.descriptions[0],{},false,true)
    fallback,_:=graph_image(&graph,fixture.descriptions[1],{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    second,_:=graph_image(&graph,fixture.descriptions[2],{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    color:=Image_Access{output,image_full_range(fixture.descriptions[0]),.Write,.Color_Attachment}
    read:=Image_Access{fallback,image_full_range(fixture.descriptions[1]),.Read,.Sampled}
    another:=Image_Access{second,image_full_range(fixture.descriptions[2]),.Read,.Sampled}
    pass,_:=graph_pass(&graph,"fixed-array",.Graphics,nil,images={color,read,another})
    accesses:=[3]Image_Access{read,another,read}
    packet:=Render{colors={{color,.Clear,.Store,{0,0,0,1}}},images={{1,0,{.Fragment},accesses[:]}},phases={{pipeline={&fixture,0,0},draws={Draw{3,1,0,0}}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    accesses[0]=another
    stored:=graph.passes[pass.index].packet.(Render)
    testing.expect_value(t,stored.images[0].accesses[0],read)
    plan,error:=graph_compile(&graph); defer compiled_graph_destroy(&plan); testing.expect_value(t,error,Graph_Error.None)
    query:=Graphics_Query{state=&fixture,texture=array_fixture_texture,pipeline=array_fixture_graphics}
    inputs:=[3]Texture_Input{{output,{&fixture,0,0}},{fallback,{&fixture,1,0}},{second,{&fixture,2,0}}}
    prepared,result:=graph_prepare(&graph,&plan,nil,{},textures=inputs[:],graphics=query)
    defer prepared_graph_destroy(&prepared); testing.expect_value(t,result,Packet_Error.None)
    stored.images[0].accesses[0]=another
    testing.expect_value(t,prepared.passes[0].packet.(Render).images[0].accesses[0],read)
    stored.images[0].accesses[0]=read
    short:=Image_Binding{1,0,{.Fragment},accesses[:2]}
    testing.expect_value(t,preflight_image_bindings({short},{fixture.requirement},inputs[:],query),Packet_Error.Invalid_Binding)
    fixture.requirement.array_count=0
    testing.expect_value(t,preflight_image_bindings(stored.images,{fixture.requirement},inputs[:],query),Packet_Error.Invalid_Binding)
    fixture.requirement.array_count=3
    fixture.descriptions[2].depth=4
    testing.expect_value(t,preflight_image_bindings(stored.images,{fixture.requirement},inputs[:],query),Packet_Error.Invalid_Binding)
    fixture.descriptions[2].depth=1
    inputs[2].handle.generation=1
    testing.expect_value(t,preflight_image_bindings(stored.images,{fixture.requirement},inputs[:],query),Packet_Error.Missing_Resource)
    inputs[2].handle.generation=0
    stored.images[0].accesses[2].mode=.None
    testing.expect_value(t,preflight_image_bindings(stored.images,{fixture.requirement},inputs[:],query),Packet_Error.Invalid_Binding)
    testing.expect_value(t,prepared.passes[0].packet.(Render).images[0].accesses[2].mode,Access_Mode.Read)
}
