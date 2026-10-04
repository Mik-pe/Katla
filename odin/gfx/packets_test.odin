#+test
package gfx

import "core:testing"

Fixture :: struct { buffers:Resource_Storage(i32,Buffer_Kind), pipeline:Pipeline_Handle, layout:Pipeline_Info }
fixture_buffer :: proc(state:rawptr,handle:Buffer_Handle)->(Buffer_Info,bool) {
    f:=cast(^Fixture)state
    value,ok:=storage_get(&f.buffers,handle); if !ok { return {},false }
    return {{4096,{.Storage,.Uniform,.Transfer_Source,.Transfer_Destination}},value},true
}
fixture_pipeline :: proc(state:rawptr,handle:Pipeline_Handle)->(Pipeline_Info,bool) {
    f:=cast(^Fixture)state
    return f.layout,handle==f.pipeline
}
@(test)
test_packet_preflight_cloning_and_failure_immutability :: proc(t:^testing.T) {
    f:Fixture; storage_init(&f.buffers); defer storage_destroy(&f.buffers)
    handle:=storage_insert(&f.buffers,1); defer storage_remove(&f.buffers,handle)
    f.pipeline={&f,0,0}
    requirements:=[1]Binding_Requirement{{0,.Storage,16,16,4096}}
    f.layout={requirements[:],{64,1,1},1024}
    query:=Resource_Query{&f,fixture_buffer,fixture_pipeline,{65535,65535,65535}}
    graph:Buffer_Graph; graph_init(&graph); defer graph_destroy(&graph)
    buffer,_:=graph_buffer(&graph,{64,{.Storage}},false,true)
    access:=Buffer_Access{buffer,{0,64},.Write,.Storage}
    pass,_:=graph_pass(&graph,"compute",.Compute,{access})
    bindings:=[1]Buffer_Binding{{0,access}}
    testing.expect_value(t,graph_set_packet(&graph,pass,Dispatch{f.pipeline,{1,1,1},bindings[:]}),Packet_Error.None)
    bindings[0].slot=17
    plan,compiled:=graph_compile(&graph); testing.expect_value(t,compiled,Graph_Error.None); defer compiled_graph_destroy(&plan)
    prepared,err:=graph_prepare(&graph,&plan,{{buffer,handle}},query); testing.expect_value(t,err,Packet_Error.None); defer prepared_graph_destroy(&prepared)
    dispatch:=prepared.passes[0].packet.(Dispatch)
    testing.expect_value(t,dispatch.bindings[0].slot,u32(0))
    bad:=access; bad.range={0,16}
    testing.expect_value(t,graph_set_packet(&graph,pass,Dispatch{f.pipeline,{1,1,1},{{0,bad}}}),Packet_Error.Undeclared_Access)
    testing.expect_value(t,graph_set_packet(&graph,pass,Dispatch{f.pipeline,{0,1,1},{{0,access}}}),Packet_Error.Invalid_Dispatch)
    wrong_slot:=Dispatch{f.pipeline,{1,1,1},{{1,access}}}
    testing.expect_value(t,graph_set_packet(&graph,pass,wrong_slot),Packet_Error.None)
    rejected,rejection:=graph_prepare(&graph,&plan,{{buffer,handle}},query); defer prepared_graph_destroy(&rejected)
    testing.expect_value(t,rejection,Packet_Error.Missing_Binding)
    testing.expect_value(t,len(rejected.passes),0)
    testing.expect_value(t,dispatch.bindings[0].slot,u32(0))
    graph_buffer(&graph,{8,{.Storage}},true,false)
    stale,stale_error:=graph_prepare(&graph,&plan,{{buffer,handle}},query); defer prepared_graph_destroy(&stale)
    testing.expect_value(t,stale_error,Packet_Error.Invalid_Plan)
}
@(test)
test_packet_native_layout_and_physical_alias_rejections :: proc(t:^testing.T) {
    f:Fixture; storage_init(&f.buffers); defer storage_destroy(&f.buffers)
    handle:=storage_insert(&f.buffers,1); defer storage_remove(&f.buffers,handle)
    f.pipeline={&f,0,0}
    requirements:=[1]Binding_Requirement{{0,.Uniform,16,16,64}}
    f.layout={requirements[:],{64,1,1},1024}
    query:=Resource_Query{&f,fixture_buffer,fixture_pipeline,{65535,65535,65535}}
    for range in ([]Buffer_Range{{0,8},{1,16},{0,80}}) {
        graph:Buffer_Graph; graph_init(&graph)
        buffer,_:=graph_buffer(&graph,{128,{.Uniform}},true,false)
        access:=Buffer_Access{buffer,range,.Read,.Uniform}
        pass,_:=graph_pass(&graph,"read",.Compute,{access},side_effect=true)
        graph_set_packet(&graph,pass,Dispatch{f.pipeline,{1,1,1},{{0,access}}})
        plan,_:=graph_compile(&graph)
        prepared,err:=graph_prepare(&graph,&plan,{{buffer,handle}},query)
        testing.expect_value(t,err,Packet_Error.Invalid_Binding)
        prepared_graph_destroy(&prepared); compiled_graph_destroy(&plan); graph_destroy(&graph)
    }
    graph:Buffer_Graph; graph_init(&graph); defer graph_destroy(&graph)
    a,_:=graph_buffer(&graph,{64,{.Storage}},true,false)
    b,_:=graph_buffer(&graph,{64,{.Storage}},true,false)
    plan,_:=graph_compile(&graph); defer compiled_graph_destroy(&plan)
    prepared,err:=graph_prepare(&graph,&plan,{{a,handle},{b,handle}},query); defer prepared_graph_destroy(&prepared)
    testing.expect_value(t,err,Packet_Error.Aliased_Resource)
}
