#+test
//! Graph extension rollback preserves the prefix and rejects live tail references atomically.
package gfx

import "core:testing"
import "core:mem"

@(test)
test_graph_truncate_atomic_prefix_and_bounded_packet_owners :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    {
        graph:Graph; graph_init(&graph,mem.tracking_allocator(&tracker)); defer graph_destroy(&graph)
        desc:=Texture_Desc{width=4,height=4,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment}}
        persistent,_:=graph_image(&graph,desc,{},false,true)
        access:=Image_Access{persistent,image_full_range(desc),.Write,.Color_Attachment}
        first,_:=graph_pass(&graph,"Persistent",.Graphics,nil,images={access}); testing.expect_value(t,graph_set_packet(&graph,first,Render{colors={{access,.Clear,.Store,{1,0,0,1}}}}),Packet_Error.None)
        graph_buffer(&graph,{size=16,usage={.Storage}},true,false)
        testing.expect_value(t,graph_truncate(&graph,1,0,1),Graph_Error.None)
        prefix_allocations:=len(tracker.allocation_map)
        for _ in 0..<128 {
            temporary,_:=graph_image(&graph,desc,{},false,true)
            buffer,_:=graph_buffer(&graph,{size=16,usage={.Storage}},true,false)
            temporary_access:=Image_Access{temporary,image_full_range(desc),.Write,.Color_Attachment}
            buffer_access:=Buffer_Access{buffer,{0,16},.Read,.Storage}
            tail,_:=graph_pass(&graph,"Temporary UI",.Graphics,{buffer_access},images={temporary_access})
            bytes:=[16]byte{}
            packet:=Render{colors={{temporary_access,.Clear,.Store,{}}},buffers={{0,0,{.Vertex},buffer_access}},phases={{pipeline={&graph,0,0},constants={{group=0,slot=1,stages={.Fragment},usage=.Uniform,bytes=bytes[:]}},draws={Draw{3,1,0,0}}}}}
            testing.expect_value(t,graph_set_packet(&graph,tail,packet),Packet_Error.None)
            plan,error:=graph_compile(&graph); testing.expect_value(t,error,Graph_Error.None)
            revision:=graph.revision
            testing.expect_value(t,graph_truncate(&graph,2,0,2),Graph_Error.Invalid_Resource)
            testing.expect_value(t,graph_truncate(&graph,2,1,1),Graph_Error.Invalid_Resource)
            testing.expect_value(t,graph_truncate(&graph,1,0,0),Graph_Error.Invalid_Resource)
            testing.expect_value(t,graph_truncate(&graph,-1,0,0),Graph_Error.Invalid_Range)
            testing.expect_value(t,graph.revision,revision); testing.expect_value(t,len(graph.passes),2)
            testing.expect_value(t,graph_truncate(&graph,1,0,1),Graph_Error.None)
            testing.expect_value(t,graph.revision,revision+1); testing.expect(t,plan.revision!=graph.revision)
            testing.expect_value(t,len(graph.buffers),0); testing.expect_value(t,len(graph.images),1); testing.expect_value(t,len(graph.passes),1)
            compiled_graph_destroy(&plan)
            testing.expect_value(t,len(tracker.allocation_map),prefix_allocations)
            testing.expect_value(t,graph_truncate(&graph,1,0,1),Graph_Error.None); testing.expect_value(t,graph.revision,revision+1)
        }
    }
    testing.expect_value(t,len(tracker.allocation_map),0)
}
