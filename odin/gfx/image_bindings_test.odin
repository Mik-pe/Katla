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
    testing.expect_value(t,prepared.passes[0].packet.(Render).phases[0].images[0].accesses[0],read)
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
    testing.expect_value(t,prepared.passes[0].packet.(Render).phases[0].images[0].accesses[2].mode,Access_Mode.Read)
}

@(test)
test_phase_image_override_preserves_array_shape_and_restores_shared_image :: proc(t:^testing.T) {
    graph:Graph;graph_init(&graph);defer graph_destroy(&graph)
    fixture:=Array_Fixture{colors={.RGBA8_Unorm},descriptions={{4,4,1,1,.RGBA8_Unorm,{.Color_Attachment},1},{4,4,1,1,.RGBA8_Unorm,{.Sampled},1},{4,4,1,1,.RGBA8_Unorm,{.Sampled},1}},requirement={group=1,slot=0,stages={.Fragment},usage=.Sampled,mode=.Read,array_count=1}}
    output,_:=graph_image(&graph,fixture.descriptions[0],{},false,true)
    first,_:=graph_image(&graph,fixture.descriptions[1],{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    second,_:=graph_image(&graph,fixture.descriptions[2],{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    color:=Image_Access{output,image_full_range(fixture.descriptions[0]),.Write,.Color_Attachment}
    shared:=[1]Image_Access{{first,image_full_range(fixture.descriptions[1]),.Read,.Sampled}}
    overrides:=[1]Image_Access{{second,image_full_range(fixture.descriptions[2]),.Read,.Sampled}}
    pass,_:=graph_pass(&graph,"phase-owned images",.Graphics,nil,images={color,shared[0],overrides[0]})
    packet:=Render{colors={{color,.Clear,.Store,{}}},images={{1,0,{.Fragment},shared[:]}},phases={{pipeline={&fixture,0,0},images={{1,0,{.Fragment},overrides[:]}},draws={Draw{3,1,0,0}}},{pipeline={&fixture,0,0},draws={Draw{3,1,0,0}}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    overrides[0]=shared[0]
    plan,error:=graph_compile(&graph);testing.expect_value(t,error,Graph_Error.None);defer compiled_graph_destroy(&plan)
    inputs:=[3]Texture_Input{{output,{&fixture,0,0}},{first,{&fixture,1,0}},{second,{&fixture,2,0}}}
    query:=Graphics_Query{state=&fixture,texture=array_fixture_texture,pipeline=array_fixture_graphics}
    prepared,rejection:=graph_prepare(&graph,&plan,nil,{},textures=inputs[:],graphics=query);defer prepared_graph_destroy(&prepared);testing.expect_value(t,rejection,Packet_Error.None)
    frozen:=prepared.passes[0].packet.(Render);testing.expect_value(t,len(frozen.images),0)
    testing.expect_value(t,frozen.phases[0].images[0].accesses[0].resource,second)
    testing.expect_value(t,frozen.phases[1].images[0].accesses[0].resource,first)
    packet.phases[0].images[0].accesses={shared[0],{second,image_full_range(fixture.descriptions[2]),.Read,.Sampled}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    invalid,failure:=graph_prepare(&graph,&plan,nil,{},textures=inputs[:],graphics=query);defer prepared_graph_destroy(&invalid)
    testing.expect_value(t,failure,Packet_Error.Invalid_Binding)
    testing.expect_value(t,frozen.phases[0].images[0].accesses[0].resource,second)
}

@(test)
test_overridden_storage_image_default_does_not_invent_a_graph_write :: proc(t:^testing.T) {
    graph:Graph
    default_access:=Image_Access{{&graph,0},{0,1,0,1,{.Color}},.Write,.Storage}
    actual_access:=Image_Access{{&graph,1},{0,1,0,1,{.Color}},.Write,.Storage}
    packet:=Render{images={{group=1,slot=0,stages={.Fragment},accesses={default_access}}},phases={{images={{group=1,slot=0,stages={.Fragment},accesses={actual_access}}}}}}
    accesses:=packet_image_accesses(&graph,packet,context.allocator);defer delete(accesses)
    testing.expect_value(t,len(accesses),1);testing.expect_value(t,accesses[0],actual_access)
}
