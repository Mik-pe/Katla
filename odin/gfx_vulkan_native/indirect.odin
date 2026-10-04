//! GPU fill commands produce the dispatch dimensions consumed by the following shader.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

run_indirect :: proc(r:^gpu.Renderer,desc:gfx.Compute_Desc) {
    pipeline,pipeline_error:=gpu.create_pipeline(r,desc); assert(pipeline_error==.None)
    defer assert(gpu.destroy_pipeline(r,pipeline)==.None)
    command_desc:=gfx.Buffer_Desc{size=12,usage={.Transfer_Destination,.Indirect},memory=.GPU_Private}
    data_desc:=gfx.Buffer_Desc{size=256,usage={.Storage,.Transfer_Source},memory=.GPU_Private}
    output_desc:=gfx.Buffer_Desc{size=256,usage={.Readback,.Transfer_Destination}}
    command,command_error:=gpu.create_buffer(r,command_desc); assert(command_error==.None)
    defer assert(gpu.destroy_buffer(r,command)==.None)
    data,data_error:=gpu.create_buffer(r,data_desc); assert(data_error==.None)
    defer assert(gpu.destroy_buffer(r,data)==.None)
    output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    cmd,_:=gfx.graph_buffer(&graph,command_desc,false,false)
    values,_:=gfx.graph_buffer(&graph,data_desc,false,false)
    result,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    fill,_:=gfx.graph_pass(&graph,"gpu-dispatch-command",.Transfer,{{cmd,{0,12},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,fill,gfx.Fill_Buffer{cmd,0,12,1})==.None)
    command_read:=gfx.Buffer_Access{cmd,{0,12},.Read,.Indirect}
    data_write:=gfx.Buffer_Access{values,{0,256},.Write,.Storage}
    dispatch,_:=gfx.graph_pass(&graph,"gpu-produced-indirect",.Compute,{command_read,data_write})
    assert(gfx.graph_set_packet(&graph,dispatch,gfx.Dispatch{pipeline=pipeline,bindings={{0,0,data_write}},indirect={true,command_read}})==.None)
    copy,_:=gfx.graph_pass(&graph,"indirect-values",.Transfer,{{values,{0,256},.Read,.Transfer_Source},{result,{0,256},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Buffer{values,result,0,0,256})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    for _ in 0..<4 {
        token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
        submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{cmd,command},{values,data},{result,output}},nil)
        assert(native_error==.None && packet_error==.None)
        assert(gpu.wait(r,submission)==.None)
        actual:[64]u32; assert(gpu.read_buffer(r,output,0,mem.slice_to_bytes(actual[:]))==.None)
        for value,i in actual { assert(value==u32(i)*3+7) }
    }
    fmt.println("Indirect: GPU fill produced command dimensions consumed by native indirect dispatch;256 exact compute values verified")
}
