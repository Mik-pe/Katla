#+test
package gfx

import "core:testing"
import "core:mem"

@(test)
test_remove_last_pass_releases_owned_packet_and_invalidates_plan :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator)
    defer mem.tracking_allocator_destroy(&tracker)
    {
        graph:Graph; graph_init(&graph,mem.tracking_allocator(&tracker)); defer graph_destroy(&graph)
        image,error:=graph_image(&graph,{width=4,height=4,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}, {},false,true)
        testing.expect_value(t,error,Graph_Error.None)
        access:=Image_Access{image,{0,1,0,1,{.Color}},.Write,.Color_Attachment}
        first,first_error:=graph_pass(&graph,"First",.Graphics,nil,images={access}); testing.expect_value(t,first_error,Graph_Error.None)
        testing.expect_value(t,graph_set_packet(&graph,first,Render{colors={{{access.resource,access.range,.Write,.Color_Attachment},.Clear,.Store,{}}}}),Packet_Error.None)
        last,last_error:=graph_pass(&graph,"Final",.Graphics,nil,images={access}); testing.expect_value(t,last_error,Graph_Error.None)
        bytes:=[16]byte{}
        phases:=[1]Render_Phase{{pipeline={&graph,0,0},constants={{group=0,slot=0,stages={.Fragment},usage=.Uniform,bytes=bytes[:]}},draws={Draw{3,1,0,0}}}}
        packet:=Render{colors={{access,.Clear,.Store,{}}},phases=phases[:]}
        testing.expect_value(t,graph_set_packet(&graph,last,packet),Packet_Error.None)
        plan,plan_error:=graph_compile(&graph); defer compiled_graph_destroy(&plan); testing.expect_value(t,plan_error,Graph_Error.None)
        revision:=graph.revision
        testing.expect_value(t,graph_remove_last_pass(&graph,first),Graph_Error.Invalid_Resource)
        other:Graph
        testing.expect_value(t,graph_remove_last_pass(&graph,{&other,last.index}),Graph_Error.Invalid_Resource)
        testing.expect_value(t,graph.revision,revision)
        testing.expect_value(t,graph_remove_last_pass(&graph,last),Graph_Error.None)
        testing.expect_value(t,len(graph.passes),1)
        testing.expect_value(t,graph.revision,revision+1)
        testing.expect(t,plan.revision!=graph.revision)
        testing.expect_value(t,graph_remove_last_pass(&graph,last),Graph_Error.Invalid_Resource)
        replacement,replacement_error:=graph_compile(&graph); defer compiled_graph_destroy(&replacement)
        testing.expect_value(t,replacement_error,Graph_Error.None)
        testing.expect_value(t,len(replacement.order),1)
    }
    testing.expect_value(t,len(tracker.allocation_map),0)
}
