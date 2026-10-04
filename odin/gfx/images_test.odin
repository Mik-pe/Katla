#+test
package gfx

import "core:testing"

@(test)
test_image_subresource_initialization_and_hazards :: proc(t:^testing.T) {
    g:Graph; graph_init(&g); defer graph_destroy(&g)
    desc:=Texture_Desc{8,8,2,2,.RGBA8_Unorm,{.Color_Attachment,.Sampled,.Transfer_Source},1}
    image,err:=graph_image(&g,desc,{},false,true); testing.expect_value(t,err,Graph_Error.None)
    ranges:=[4]Image_Range{{0,1,0,1,{.Color}},{0,1,1,1,{.Color}},{1,1,0,1,{.Color}},{1,1,1,1,{.Color}}}
    for r,i in ranges[:3] {
        names:=[3]string{"mip0-layer0","mip0-layer1","mip1-layer0"}
        graph_pass(&g,names[i],.Graphics,nil,images={{image,r,.Write,.Color_Attachment}})
    }
    incomplete,rejected:=graph_compile(&g); defer compiled_graph_destroy(&incomplete)
    testing.expect_value(t,rejected,Graph_Error.Uninitialized_Read)
    graph_pass(&g,"mip1-layer1",.Graphics,nil,images={{image,ranges[3],.Write,.Color_Attachment}})
    read,_:=graph_pass(&g,"sample",.Graphics,nil,side_effect=true,images={{image,image_full_range(desc),.Read,.Sampled}})
    plan,compiled:=graph_compile(&g); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,compiled,Graph_Error.None); testing.expect_value(t,len(plan.order),5); testing.expect_value(t,len(plan.image_hazards),4)
    for hazard in plan.image_hazards { testing.expect_value(t,hazard.after,read) }
    testing.expect(t,!image_ranges_overlap(ranges[0],ranges[1]) && !image_ranges_overlap(ranges[0],ranges[2]))
}
@(test)
test_image_invalid_contracts_ranges_and_depth_aspects :: proc(t:^testing.T) {
    g,other_graph:Graph; graph_init(&g); graph_init(&other_graph); defer graph_destroy(&g); defer graph_destroy(&other_graph)
    desc:=Texture_Desc{16,8,2,1,.D32_Float,{.Depth_Attachment,.Sampled},1}
    depth,_:=graph_image(&g,desc,{},false,false)
    other,_:=graph_image(&other_graph,desc,{},false,false)
    _,invalid_import:=graph_image(&g,desc,{.Undefined,.Shader_Read,true},true,false); testing.expect_value(t,invalid_import,Graph_Error.Invalid_Resource)
    for access in ([]Image_Access{{depth,{0,1,0,1,{.Color}},.Write,.Depth_Attachment},{depth,{1,max(u32),0,1,{.Depth}},.Write,.Depth_Attachment},{depth,{0,1,0,1,{.Depth}},.Read,.Depth_Attachment},{other,{0,1,0,1,{.Depth}},.Write,.Depth_Attachment}}) {
        _,err:=graph_pass(&g,"invalid",.Graphics,nil,images={access}); testing.expect(t,err!=.None)
    }
    testing.expect_value(t,len(g.passes),0)
    testing.expect(t,!texture_desc_valid(Texture_Desc{4,4,4,1,.RGBA8_Unorm,{.Color_Attachment},1}))
    testing.expect(t,!texture_desc_valid(Texture_Desc{4,4,1,1,.D32_Float,{.Color_Attachment},1}))
    testing.expect(t,!image_region_valid(Image_Region{0,0,0,max(u32),4,4,.Depth,0,1,0,0},desc))
}

Graphics_Fixture :: struct { texture:Texture_Handle, pipeline:Graphics_Pipeline_Handle, desc:Texture_Desc, info:Graphics_Info }
fixture_texture :: proc(state:rawptr,h:Texture_Handle)->(Texture_Info,bool) { f:=cast(^Graphics_Fixture)state; return {f.desc,f},h==f.texture }
fixture_graphics :: proc(state:rawptr,h:Graphics_Pipeline_Handle)->(Graphics_Info,bool) { f:=cast(^Graphics_Fixture)state; return f.info,h==f.pipeline }
@(test)
test_render_packet_clear_copy_and_native_preflight :: proc(t:^testing.T) {
    g:Graph; graph_init(&g); defer graph_destroy(&g)
    f:=Graphics_Fixture{desc={8,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}}; f.texture={&f,0,0}; f.pipeline={&f,0,0}
    formats:=[1]Texture_Format{.RGBA8_Unorm}; f.info.colors=formats[:]; f.info.supported_draws={.Generated}
    image,_:=graph_image(&g,f.desc,{},false,true)
    access:=Image_Access{image,image_full_range(f.desc),.Write,.Color_Attachment}
    pass,_:=graph_pass(&g,"triangle",.Graphics,nil,images={access})
    draws:=[1]Draw_Op{Draw{3,1,0,0}}
    packet:=Render{colors={{access,.Clear,.Store,{0,0,0,1}}},phases={{pipeline=f.pipeline,draws=draws[:]}}}
    testing.expect_value(t,graph_set_packet(&g,pass,packet),Packet_Error.None); draws[0]=Draw{}
    plan,err:=graph_compile(&g); testing.expect_value(t,err,Graph_Error.None); defer compiled_graph_destroy(&plan)
    query:=Graphics_Query{state=&f,texture=fixture_texture,pipeline=fixture_graphics}
    prepared,preflight:=graph_prepare(&g,&plan,nil,{},textures={{image,f.texture}},graphics=query)
    testing.expect_value(t,preflight,Packet_Error.None); defer prepared_graph_destroy(&prepared)
    testing.expect_value(t,prepared.passes[0].packet.(Render).phases[0].draws[0].(Draw).vertex_count,u32(3))
    packet.colors[0].load=.Load
    testing.expect_value(t,graph_set_packet(&g,pass,packet),Packet_Error.Invalid_Packet)
    testing.expect_value(t,prepared.passes[0].packet.(Render).colors[0].load,Load_Op.Clear)
    formats[0]=.BGRA8_Unorm
    rejected,rejection:=graph_prepare(&g,&plan,nil,{},textures={{image,f.texture}},graphics=query); defer prepared_graph_destroy(&rejected)
    testing.expect_value(t,rejection,Packet_Error.Invalid_Pipeline)
    testing.expect_value(t,len(rejected.passes),0)
}
@(test)
test_physical_aliases_require_disjoint_live_intervals :: proc(t:^testing.T) {
    g:Graph; graph_init(&g); defer graph_destroy(&g)
    a,_:=graph_buffer(&g,{size=16,usage={.Storage}},false,false)
    b,_:=graph_buffer(&g,{size=16,usage={.Storage}},false,false)
    first,_:=graph_pass(&g,"a-write",.Compute,{{a,{0,16},.Write,.Storage}},side_effect=true)
    graph_pass(&g,"a-read",.Compute,{{a,{0,16},.Read,.Storage}},side_effect=true)
    second,_:=graph_pass(&g,"b-write",.Compute,{{b,{0,16},.Write,.Storage}},side_effect=true)
    plan,err:=graph_compile(&g); testing.expect_value(t,err,Graph_Error.None); defer compiled_graph_destroy(&plan)
    prepared:=Prepared_Graph{allocator=g.allocator}; defer prepared_graph_destroy(&prepared)
    testing.expect_value(t,prepare_buffer_alias(&prepared,&g,&plan,a,b),Packet_Error.None)
    testing.expect_value(t,len(prepared.aliases),1); testing.expect_value(t,prepared.aliases[0].before,plan.order[1]); testing.expect_value(t,prepared.aliases[0].after,second)
    graph_pass(&g,"a-again",.Compute,{{a,{0,16},.Read,.Storage}},side_effect=true)
    overlap,_:=graph_compile(&g); defer compiled_graph_destroy(&overlap)
    testing.expect_value(t,prepare_buffer_alias(&prepared,&g,&overlap,a,b),Packet_Error.Aliased_Resource)
    testing.expect_value(t,first,plan.order[0])
}

@(test)
test_latest_surviving_image_and_buffer_producers_are_live :: proc(t:^testing.T) {
    g:Graph; graph_init(&g); defer graph_destroy(&g)
    buffer,_:=graph_buffer(&g,{size=64,usage={.Storage}},false,true)
    graph_pass(&g,"overwritten-buffer",.Compute,{{buffer,{0,64},.Write,.Storage}})
    buffer_head,_:=graph_pass(&g,"buffer-head",.Compute,{{buffer,{0,32},.Write,.Storage}})
    buffer_tail,_:=graph_pass(&g,"buffer-tail",.Compute,{{buffer,{32,32},.Write,.Storage}})
    desc:=Texture_Desc{4,4,2,2,.RGBA8_Unorm,{.Color_Attachment},1}
    image,_:=graph_image(&g,desc,{},false,true)
    graph_pass(&g,"overwritten-image",.Graphics,nil,images={{image,{0,1,0,1,{.Color}},.Write,.Color_Attachment}})
    mip0,_:=graph_pass(&g,"mip-zero",.Graphics,nil,images={{image,{0,1,0,1,{.Color}},.Write,.Color_Attachment},{image,{0,1,1,1,{.Color}},.Write,.Color_Attachment}})
    mip1,_:=graph_pass(&g,"mip-one",.Graphics,nil,images={{image,{1,1,0,1,{.Color}},.Write,.Color_Attachment},{image,{1,1,1,1,{.Color}},.Write,.Color_Attachment}})
    plan,err:=graph_compile(&g); testing.expect_value(t,err,Graph_Error.None); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,len(plan.order),4)
    testing.expect(t,plan.order[0]==buffer_head && plan.order[1]==buffer_tail && plan.order[2]==mip0 && plan.order[3]==mip1)
}
