#+test
package gfx

import "core:testing"

@(test)
test_volume_pitches_compressed_edges_and_overflow :: proc(t:^testing.T) {
    desc:=Texture_Desc{width=8,height=4,depth=4,mip_levels=4,layers=1,format=.RGBA8_Unorm,usage={.Transfer_Source,.Transfer_Destination}}
    testing.expect(t,texture_desc_valid(desc))
    region:=Image_Region{width=3,height=2,depth=2,aspect=.Color,bytes_per_row=16,bytes_per_image=48}
    layout,ok:=image_region_layout(region,desc)
    testing.expect(t,ok); testing.expect_value(t,layout.required_bytes,u64(76))
    testing.expect_value(t,layout.row_bytes,u64(12)); testing.expect_value(t,layout.block_rows,u64(2))
    region.bytes_per_image=max(u64)-3; region.depth=4
    _,overflow:=image_region_layout(region,desc); testing.expect(t,!overflow)
    desc.layers=2; testing.expect(t,!texture_desc_valid(desc)); desc.layers=1
    region={mip=2,width=2,height=1,depth=2,aspect=.Color}
    testing.expect(t,!image_region_valid(region,desc))
    compressed:=Texture_Desc{width=7,height=5,depth=1,mip_levels=1,layers=1,format=.BC1_RGBA_Unorm,usage={.Transfer_Source,.Transfer_Destination,.Sampled}}
    edge:=Image_Region{x=4,y=4,width=3,height=1,depth=1,aspect=.Color}
    edge_layout,edge_ok:=image_region_layout(edge,compressed)
    testing.expect(t,edge_ok); testing.expect_value(t,edge_layout.required_bytes,u64(8))
    edge.x=3; testing.expect(t,!image_region_valid(edge,compressed))
    compressed.usage|={.Storage}; testing.expect(t,!texture_desc_valid(compressed))
    testing.expect(t,!texture_filterable_mips(.BC1_RGBA_Unorm) && !texture_filterable_mips(.R32_Uint))
}

@(test)
test_fill_indirect_command_initialization_and_span :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    command,_:=graph_buffer(&graph,{size=12,usage={.Indirect,.Transfer_Destination}},false,false)
    fill,_:=graph_pass(&graph,"commands",.Transfer,{{command,{0,12},.Write,.Transfer_Destination}})
    testing.expect_value(t,graph_set_packet(&graph,fill,Fill_Buffer{command,0,12,1}),Packet_Error.None)
    access:=Buffer_Access{command,{0,12},.Read,.Indirect}
    dispatch,dispatch_error:=graph_pass(&graph,"indirect",.Compute,{access},side_effect=true)
    testing.expect_value(t,dispatch_error,Graph_Error.None)
    packet:=Dispatch{indirect={true,access}}
    testing.expect_value(t,graph_set_packet(&graph,dispatch,packet),Packet_Error.None)
    packet.indirect.command.range.size=8
    testing.expect_value(t,graph_set_packet(&graph,dispatch,packet),Packet_Error.Invalid_Dispatch)
    packet.indirect.command=access; packet.groups={1,1,1}
    testing.expect_value(t,graph_set_packet(&graph,dispatch,packet),Packet_Error.Invalid_Dispatch)
    plan,error:=graph_compile(&graph); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,error,Graph_Error.None); testing.expect_value(t,len(plan.order),2)
    testing.expect_value(t,len(plan.hazards),1)
    testing.expect_value(t,graph_set_packet(&graph,fill,Fill_Buffer{command,0,10,1}),Packet_Error.Invalid_Packet)
}

@(test)
test_mip_generation_requires_initialized_base_and_owns_whole_chain :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    desc:=Texture_Desc{width=8,height=8,depth=1,mip_levels=4,layers=1,format=.RGBA8_Srgb,usage={.Transfer_Source,.Transfer_Destination}}
    image,_:=graph_image(&graph,desc,{},false,true)
    base:=Image_Range{0,1,0,1,{.Color}}; rest:=Image_Range{1,3,0,1,{.Color}}
    generate,error:=graph_pass(&graph,"mips",.Transfer,nil,images={{image,base,.Read,.Transfer_Source},{image,rest,.Write,.Transfer_Destination}})
    testing.expect_value(t,error,Graph_Error.None)
    testing.expect_value(t,graph_set_packet(&graph,generate,Generate_Mips{image,image_full_range(desc)}),Packet_Error.None)
    missing,missing_error:=graph_compile(&graph); defer compiled_graph_destroy(&missing)
    testing.expect_value(t,missing_error,Graph_Error.Uninitialized_Read)
    graph.images[0].imported=true; graph.images[0].contract={.Transfer_Source,.Shader_Read,true}
    plan,compile_error:=graph_compile(&graph); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,compile_error,Graph_Error.None); testing.expect_value(t,len(plan.order),1)
    invalid:=Generate_Mips{image,base}
    testing.expect_value(t,graph_set_packet(&graph,generate,invalid),Packet_Error.Invalid_Packet)
}

@(test)
test_discard_invalidates_latest_content_and_compiled_plan :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    desc:=Texture_Desc{width=4,height=4,depth=1,mip_levels=1,layers=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Sampled}}
    image,_:=graph_image(&graph,desc,{.Shader_Read,.Shader_Read,true},true,true)
    access:=Image_Access{image,image_full_range(desc),.Read_Write,.Color_Attachment}
    pass,_:=graph_pass(&graph,"attachment",.Graphics,nil,side_effect=true,images={access})
    packet:=Render{colors={{access,.Load,.Store,{}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    plan,error:=graph_compile(&graph); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,error,Graph_Error.None)
    packet.colors[0].store=.Discard
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    testing.expect(t,graph.revision!=plan.revision)
    discarded,discard_error:=graph_compile(&graph); defer compiled_graph_destroy(&discarded)
    testing.expect_value(t,discard_error,Graph_Error.Uninitialized_Read)
    graph.images[0].exported=false
    _,read_error:=graph_pass(&graph,"sample",.Graphics,nil,side_effect=true,images={{image,image_full_range(desc),.Read,.Sampled}})
    testing.expect_value(t,read_error,Graph_Error.None)
    read_plan,uninitialized:=graph_compile(&graph); defer compiled_graph_destroy(&read_plan)
    testing.expect_value(t,uninitialized,Graph_Error.Uninitialized_Read)
}

@(test)
test_pitched_image_copy_preserves_unwritten_buffer_padding :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    desc:=Texture_Desc{width=2,height=2,depth=1,mip_levels=1,layers=1,format=.RGBA8_Unorm,usage={.Transfer_Source}}
    image,_:=graph_image(&graph,desc,{.Transfer_Source,.Transfer_Source,true},true,false)
    output,_:=graph_buffer(&graph,{size=24,usage={.Transfer_Destination,.Readback}},false,true)
    access:=Image_Access{image,image_full_range(desc),.Read,.Transfer_Source}
    pass,_:=graph_pass(&graph,"pitched",.Transfer,{{output,{0,24},.Write,.Transfer_Destination}},images={access})
    packet:=Copy_Image_Buffer{source=image,destination=output,region={width=2,height=2,depth=1,aspect=.Color,bytes_per_row=16}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.Undeclared_Access)
    packet_error,graph_error:=graph_set_commands(&graph,pass,packet,{{output,{0,8},.Write,.Transfer_Destination},{output,{16,8},.Write,.Transfer_Destination}},{access})
    testing.expect_value(t,packet_error,Packet_Error.None); testing.expect_value(t,graph_error,Graph_Error.None)
    plan,error:=graph_compile(&graph); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,error,Graph_Error.Uninitialized_Read)
    graph.images[0].exported=false; graph.buffers[0].exported=false; graph.passes[0].side_effect=true
    useful,useful_error:=graph_compile(&graph); defer compiled_graph_destroy(&useful)
    testing.expect_value(t,useful_error,Graph_Error.None); testing.expect_value(t,len(useful.order),1)
}
