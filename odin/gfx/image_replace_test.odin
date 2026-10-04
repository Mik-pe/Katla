#+test
package gfx

import "core:testing"

@(test)
test_imported_image_replacement_is_bounded_and_prepared_recording_is_immutable :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    old:=Texture_Desc{width=8,height=8,depth=1,layers=1,mip_levels=2,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    fixture:=Graphics_Fixture{desc=old}; fixture.texture={&fixture,0,0}
    image,error:=graph_image(&graph,old,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,true); testing.expect_value(t,error,Graph_Error.None)
    pass,_:=graph_pass(&graph,"clear imported image",.Graphics,nil,images={{image,{0,1,0,1,{.Color}},.Write,.Color_Attachment}})
    packet:=Render{colors={{{image,{0,1,0,1,{.Color}},.Write,.Color_Attachment},.Clear,.Store,{1,0,0,1}}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,packet),Packet_Error.None)
    plan,compile_error:=graph_compile(&graph); testing.expect_value(t,compile_error,Graph_Error.None); defer compiled_graph_destroy(&plan)
    recording,record_error:=graph_prepare(&graph,&plan,nil,{},{{image,fixture.texture}},Graphics_Query{state=&fixture,texture=fixture_texture})
    testing.expect_value(t,record_error,Packet_Error.None); defer prepared_graph_destroy(&recording)
    saved_revision:=graph.revision
    smaller:=old; smaller.width=1; smaller.height=1; smaller.mip_levels=1
    testing.expect_value(t,graph_replace_image(&graph,image,smaller),Graph_Error.None)
    testing.expect_value(t,len(graph.images),1); testing.expect_value(t,recording.images[0].desc,old)
    stale,stale_error:=graph_prepare(&graph,&plan,nil,{},{{image,fixture.texture}},Graphics_Query{state=&fixture,texture=fixture_texture}); defer prepared_graph_destroy(&stale)
    testing.expect_value(t,stale_error,Packet_Error.Invalid_Plan)
    bad:=smaller; bad.usage={.Sampled}
    testing.expect_value(t,graph_replace_image(&graph,image,bad),Graph_Error.None)
    rejected,rejection:=graph_compile(&graph); defer compiled_graph_destroy(&rejected); testing.expect_value(t,rejection,Graph_Error.Invalid_Usage)
    testing.expect_value(t,graph_replace_image(&graph,image,old),Graph_Error.None)
    // A transaction that restores every declaration and command can restore its old revision.
    graph.revision=saved_revision
    restored,restored_error:=graph_prepare(&graph,&plan,nil,{},{{image,fixture.texture}},Graphics_Query{state=&fixture,texture=fixture_texture}); defer prepared_graph_destroy(&restored)
    testing.expect_value(t,restored_error,Packet_Error.None)
    for extent in 1..<33 { desc:=smaller; desc.width=u32(extent); testing.expect_value(t,graph_replace_image(&graph,image,desc),Graph_Error.None) }
    testing.expect_value(t,len(graph.images),1); testing.expect_value(t,recording.images[0].desc,old)
}
