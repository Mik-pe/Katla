#+test
package gfx

import "core:testing"

@(test)
test_draw_layout_counts_alignment_and_indirect_bounds :: proc(t:^testing.T) {
    layouts:=[1]Vertex_Layout_Binding{{0,16,.Vertex}}
    attributes:=[1]Vertex_Attribute{{0,0,0,.Float4}}
    info:=Graphics_Info{vertex={attributes[:],layouts[:]},supported_draws={.Vertices,.Indexed,.Indirect,.Indexed_Indirect}}
    vertices:=[1]Vertex_Binding{{0,{{},{0,48},.Read,.Vertex}}}
    draw:=Draw_Vertices{vertices[:],3,1,0,0}
    testing.expect_value(t,preflight_draw(draw,info),Packet_Error.None)
    draw.first_vertex=1
    testing.expect_value(t,preflight_draw(draw,info),Packet_Error.Invalid_Binding)
    draw.first_vertex=max(u32); draw.vertex_count=max(u32)
    testing.expect_value(t,preflight_draw(draw,info),Packet_Error.Invalid_Binding)
    indirect:=Draw_Indirect{vertices[:],{{},{0,32},.Read,.Indirect},2,16}
    testing.expect_value(t,validate_draw_packet(indirect),Packet_Error.None)
    indirect.count=3
    testing.expect_value(t,validate_draw_packet(indirect),Packet_Error.Invalid_Binding)
    index:=Draw_Indexed{vertices[:],{{},{0,6},.Read,.Index},.Uint16,3,1,0,0,0}
    testing.expect_value(t,validate_draw_packet(index),Packet_Error.None)
    index.first_index=1
    testing.expect_value(t,validate_draw_packet(index),Packet_Error.Invalid_Binding)
    index.first_index=0; index.index.range.offset=1
    testing.expect_value(t,validate_draw_packet(index),Packet_Error.Invalid_Binding)
    attributes[0].offset=12
    testing.expect(t,!vertex_layout_valid(info.vertex))
}
@(test)
test_constant_packet_ownership_and_reflection_spans :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    f:=Graphics_Fixture{desc={8,8,1,1,.RGBA8_Unorm,{.Color_Attachment},1}}; f.texture={&f,0,0}; f.pipeline={&f,0,0}
    formats:=[1]Texture_Format{.RGBA8_Unorm}; f.info.colors=formats[:]; f.info.supported_draws={.Generated}
    requirements:=[1]Stage_Buffer_Requirement{{group=2,slot=9,stages={.Fragment},usage=.Uniform,minimum_size=16,alignment=16,maximum_size=4096,mode=.Read}}; f.info.buffers=requirements[:]
    image,_:=graph_image(&graph,f.desc,{},false,true)
    access:=Image_Access{image,image_full_range(f.desc),.Write,.Color_Attachment}
    pass,_:=graph_pass(&graph,"constants",.Graphics,nil,images={access})
    bytes:[16]byte; bytes[0]=42
    constant:=Constant_Binding{2,9,{.Fragment},.Uniform,bytes[:]}
    packet:=Render{colors={{access,.Clear,.Store,{0,0,0,1}}},constants={constant},phases={{pipeline=f.pipeline,draws={Draw{3,1,0,0}}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    bytes[0]=99
    plan,err:=graph_compile(&graph); testing.expect_value(t,err,Graph_Error.None); defer compiled_graph_destroy(&plan)
    query:=Graphics_Query{state=&f,texture=fixture_texture,pipeline=fixture_graphics}
    prepared,preflight:=graph_prepare(&graph,&plan,nil,{},textures={{image,f.texture}},graphics=query); defer prepared_graph_destroy(&prepared)
    testing.expect_value(t,preflight,Packet_Error.None)
    testing.expect_value(t,prepared.passes[0].packet.(Render).phases[0].constants[0].bytes[0],byte(42))
    packet.constants[0].bytes=bytes[:8]
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    rejected,rejection:=graph_prepare(&graph,&plan,nil,{},textures={{image,f.texture}},graphics=query); defer prepared_graph_destroy(&rejected)
    testing.expect_value(t,rejection,Packet_Error.Invalid_Binding)
    testing.expect_value(t,len(rejected.passes),0)
    testing.expect_value(t,prepared.passes[0].packet.(Render).phases[0].constants[0].bytes[0],byte(42))
}
