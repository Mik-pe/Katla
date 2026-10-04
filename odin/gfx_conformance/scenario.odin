//! Shared native acceptance scenarios; backend functions are mandatory test inputs.
package gfx_conformance

import gfx "../gfx"
import "core:mem"
import "core:fmt"

/// Test-only function inputs keep Metal and Vulkan on the same authored workload.
API :: struct($R:typeid) {
    create_buffer:proc(^R,gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    create_pipeline:proc(^R,gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^R,gfx.Pipeline_Handle)->gfx.Gpu_Error,
    read_buffer,write_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    submit:proc(^R,^gfx.Buffer_Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,
    poll:proc(^R,gfx.Submission)->(bool,gfx.Gpu_Error),
}

/// Executes multiple live frames, immutable uniform bindings and exact retirement.
run :: proc(r:^$R,api:API(R),fill_desc,param_desc:gfx.Compute_Desc) {
    fill,fill_error:=api.create_pipeline(r,fill_desc); assert(fill_error==.None)
    params,param_error:=api.create_pipeline(r,param_desc); assert(param_error==.None)
    invalid_shader:=param_desc; invalid_shader.buffers={{0,.Storage},{2,.Uniform}}
    invalid_pipeline,invalid_error:=api.create_pipeline(r,invalid_shader)
    assert(invalid_pipeline==gfx.Pipeline_Handle{} && invalid_error==.Invalid_Shader)
    scratch,scratch_error:=api.create_buffer(r,{4096,{.Storage,.Transfer_Source}}); assert(scratch_error==.None)
    outputs:[3]gfx.Buffer_Handle
    uniforms:[3]gfx.Buffer_Handle
    graphs:[3]gfx.Buffer_Graph
    plans:[3]gfx.Compiled_Graph
    inputs:[3][3]gfx.Buffer_Input
    passes:[3]gfx.Pass_Id
    accesses:[3][2]gfx.Buffer_Access
    defer {
        for &plan in plans { gfx.compiled_graph_destroy(&plan) }
        for &graph in graphs { gfx.graph_destroy(&graph) }
    }
    for frame in 0..<3 {
        err:gfx.Gpu_Error
        outputs[frame],err=api.create_buffer(r,{4096,{.Transfer_Destination,.Readback}}); assert(err==.None)
        uniforms[frame],err=api.create_buffer(r,{512,{.Uniform}}); assert(err==.None)
        words:=[4]u32{u32(frame+2),u32(frame*13+1),0,0}
        assert(api.write_buffer(r,uniforms[frame],0,mem.slice_to_bytes(words[:]))==.None)
        graph:=&graphs[frame]; gfx.graph_init(graph)
        data,_:=gfx.graph_buffer(graph,{4096,{.Storage,.Transfer_Source}},false,false)
        output,_:=gfx.graph_buffer(graph,{4096,{.Transfer_Destination,.Readback}},false,true)
        uniform,_:=gfx.graph_buffer(graph,{512,{.Uniform}},true,false)
        inputs[frame]={{data,scratch},{output,outputs[frame]},{uniform,uniforms[frame]}}
        write:=gfx.Buffer_Access{data,{0,4096},.Write,.Storage}
        fill_pass,_:=gfx.graph_pass(graph,"fill",.Compute,{write})
        accesses[frame]={{data,{0,4096},.Read_Write,.Storage},{uniform,{0,16},.Read,.Uniform}}
        passes[frame],_=gfx.graph_pass(graph,"params",.Compute,accesses[frame][:])
        copy_pass,_:=gfx.graph_pass(graph,"copy",.Transfer,{{data,{0,4096},.Read,.Transfer_Source},{output,{0,4096},.Write,.Transfer_Destination}})
        assert(gfx.graph_set_packet(graph,fill_pass,gfx.Dispatch{fill,{16,1,1},{{0,write}}})==.None)
        assert(gfx.graph_set_packet(graph,passes[frame],gfx.Dispatch{params,{16,1,1},{{0,accesses[frame][0]},{1,accesses[frame][1]}}})==.None)
        assert(gfx.graph_set_packet(graph,copy_pass,gfx.Copy_Buffer{data,output,0,0,4096})==.None)
        compile_error:gfx.Graph_Error
        plans[frame],compile_error=gfx.graph_compile(graph); assert(compile_error==.None)
    }
    for range in ([]gfx.Buffer_Range{{0,8},{1,16}}) {
        graph:gfx.Buffer_Graph; gfx.graph_init(&graph)
        data,_:=gfx.graph_buffer(&graph,{4096,{.Storage}},true,false)
        uniform,_:=gfx.graph_buffer(&graph,{512,{.Uniform}},true,false)
        uses:=[]gfx.Buffer_Access{{data,{0,4096},.Read_Write,.Storage},{uniform,range,.Read,.Uniform}}
        pass,_:=gfx.graph_pass(&graph,"invalid-layout",.Compute,uses,side_effect=true)
        assert(gfx.graph_set_packet(&graph,pass,gfx.Dispatch{params,{16,1,1},{{0,uses[0]},{1,uses[1]}}})==.None)
        plan,err:=gfx.graph_compile(&graph); assert(err==.None)
        rejected,native_error,packet_error:=api.submit(r,&graph,&plan,{{data,scratch},{uniform,uniforms[0]}})
        assert(rejected.id==0 && native_error==.Invalid_Graph && packet_error==.Invalid_Binding)
        gfx.compiled_graph_destroy(&plan); gfx.graph_destroy(&graph)
    }
    // A shader-layout mismatch must not acquire a frame or consume a submission number.
    wrong:=gfx.Dispatch{params,{16,1,1},{{0,accesses[0][0]},{2,accesses[0][1]}}}
    assert(gfx.graph_set_packet(&graphs[0],passes[0],wrong)==.None)
    rejected,error,packet_error:=api.submit(r,&graphs[0],&plans[0],inputs[0][:])
    assert(error==.Invalid_Graph && packet_error==.Missing_Binding && rejected.id==0)
    assert(gfx.graph_set_packet(&graphs[0],passes[0],gfx.Dispatch{params,{16,1,1},{{0,accesses[0][0]},{1,accesses[0][1]}}})==.None)
    submissions:[3]gfx.Submission
    for round in 0..<4 {
        for frame in 0..<3 {
            err:gfx.Gpu_Error; preflight:gfx.Packet_Error
            submissions[frame],err,preflight=api.submit(r,&graphs[frame],&plans[frame],inputs[frame][:])
            assert(err==.None && preflight==.None)
            if round==0 && frame==0 { assert(submissions[frame].id==1) }
        }
        overflow,overflow_error,_:=api.submit(r,&graphs[0],&plans[0],inputs[0][:])
        assert(overflow_error==.Busy && overflow.id==0)
        words:[1024]u32; bytes:=mem.slice_to_bytes(words[:])
        assert(api.read_buffer(r,scratch,0,bytes)==.Busy)
        assert(api.write_buffer(r,uniforms[0],0,bytes[:16])==.Busy)
        if round==3 {
            assert(api.destroy_buffer(r,scratch)==.None)
            assert(api.destroy_pipeline(r,fill)==.None)
            assert(api.destroy_pipeline(r,params)==.None)
        }
        for frame in ([3]int{2,0,1}) {
            done,poll_error:=api.poll(r,submissions[frame]); assert(poll_error==.None)
            if !done { assert(api.wait(r,submissions[frame])==.None) }
            assert(api.wait(r,submissions[frame])==.Invalid_Resource)
            done,poll_error=api.poll(r,submissions[frame]); assert(!done && poll_error==.Invalid_Resource)
            assert(api.read_buffer(r,outputs[frame],0,bytes)==.None)
            for value,i in words[:] { assert(value==(u32(i)*3+7)*u32(frame+2)+u32(frame*13+1)) }
            if frame==2 && round!=3 { assert(api.read_buffer(r,scratch,0,bytes)==.Busy) }
        }
        fmt.printf("Round %d: three live submissions, distinct uniforms, 3072 values verified\n",round)
    }
    for frame in 0..<3 {
        assert(api.destroy_buffer(r,outputs[frame])==.None)
        assert(api.destroy_buffer(r,uniforms[frame])==.None)
    }
    fmt.println("Removed resource/pipeline handles survived three pending submissions")
}
