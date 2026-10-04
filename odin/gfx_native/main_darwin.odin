#+build darwin, arm64
//! Native Metal 4 acceptance consumer for the Odin buffer graph and frame owners.
package main

import gfx "../gfx"
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "base:intrinsics"
import "core:fmt"
import "core:sync"

send :: intrinsics.objc_send
new_object :: proc(name:cstring)->^NS.Object {
    cls:=NS.objc_lookUpClass(name); assert(cls!=nil)
    object:=send(^NS.Object,cast(^NS.Object)cls,"alloc")
    return send(^NS.Object,object,"init")
}
checked :: proc(object:^NS.Object,error:^NS.Error,label:string)->^NS.Object {
    if object==nil {
        if error!=nil { fmt.eprintln(label, error->localizedDescription()->odinString()) }
        panic(label)
    }
    return object
}
Completion :: struct { wait_group:sync.Wait_Group, failed:bool, code:NS.Integer }
feedback :: proc "c" (data:rawptr,native:^NS.Object) {
    c:=cast(^Completion)data
    error:=send(^NS.Error,native,"error")
    c.failed=error!=nil
    if error!=nil { c.code=error->code() }
    sync.wait_group_done(&c.wait_group)
}
shader_source :: `
#include <metal_stdlib>
using namespace metal;
kernel void fill(device uint *data [[buffer(0)]], uint i [[thread_position_in_grid]]) {
    data[i] = i * 3u + 7u;
}
kernel void transform(device uint *data [[buffer(0)]], uint i [[thread_position_in_grid]]) {
    data[i] = data[i] * 5u + 11u;
}
`
main :: proc() {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    device:=MTL.CreateSystemDefaultDevice()
    if device==nil || !bool(device->supportsFamily(MTL.GPUFamily(5002))) {
        panic("BLOCKED: Metal 4 device required for native Odin acceptance")
    }
    defer send(nil,device,"release")
    compiler_desc:=new_object("MTL4CompilerDescriptor"); defer compiler_desc->release()
    native_error:^NS.Error
    compiler:=checked(send(^NS.Object,device,"newCompilerWithDescriptor:error:",compiler_desc,&native_error),native_error,"create compiler")
    defer compiler->release()
    library_desc:=new_object("MTL4LibraryDescriptor"); defer library_desc->release()
    source:=NS.String.alloc()->initWithOdinString(shader_source); defer source->release()
    send(nil,library_desc,"setSource:",source)
    library:=checked(send(^NS.Object,compiler,"newLibraryWithDescriptor:error:",library_desc,&native_error),native_error,"compile library")
    defer library->release()
    pipelines:[2]^NS.Object
    for name,i in ([2]string{"fill","transform"}) {
        function:=new_object("MTL4LibraryFunctionDescriptor"); defer function->release()
        function_name:=NS.String.alloc()->initWithOdinString(name); defer function_name->release()
        send(nil,function,"setLibrary:",library); send(nil,function,"setName:",function_name)
        descriptor:=new_object("MTL4ComputePipelineDescriptor"); defer descriptor->release()
        send(nil,descriptor,"setComputeFunctionDescriptor:",function)
        pipelines[i]=checked(send(^NS.Object,compiler,"newComputePipelineStateWithDescriptor:compilerTaskOptions:error:",descriptor,cast(^NS.Object)nil,&native_error),native_error,"compile pipeline")
    }
    defer { for pipeline in pipelines { pipeline->release() } }
    queue:=checked(send(^NS.Object,device,"newMTL4CommandQueue"),nil,"create queue"); defer queue->release()
    allocator:=checked(send(^NS.Object,device,"newCommandAllocator"),nil,"create allocator"); defer allocator->release()
    command:=checked(send(^NS.Object,device,"newCommandBuffer"),nil,"create command buffer"); defer command->release()
    data:=device->newBufferWithLength(4096,MTL.ResourceStorageModeShared); assert(data!=nil); defer send(nil,data,"release")
    output:=device->newBufferWithLength(4096,MTL.ResourceStorageModeShared); assert(output!=nil); defer send(nil,output,"release")
    residency_desc:=new_object("MTLResidencySetDescriptor"); defer residency_desc->release()
    residency:=checked(send(^NS.Object,device,"newResidencySetWithDescriptor:error:",residency_desc,&native_error),native_error,"create residency")
    defer residency->release()
    send(nil,residency,"addAllocation:",data); send(nil,residency,"addAllocation:",output)
    send(nil,residency,"commit"); send(nil,residency,"requestResidency")
    defer send(nil,residency,"endResidency")
    table_desc:=new_object("MTL4ArgumentTableDescriptor"); defer table_desc->release()
    send(nil,table_desc,"setMaxBufferBindCount:",NS.UInteger(1))
    table:=checked(send(^NS.Object,device,"newArgumentTableWithDescriptor:error:",table_desc,&native_error),native_error,"create arguments")
    defer table->release()
    send(nil,table,"setAddress:atIndex:",data->gpuAddress(),NS.UInteger(0))

    graph:gfx.Buffer_Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    scratch,_:=gfx.graph_buffer(&graph,{4096,{.Storage,.Transfer_Source}},false,false)
    result,_:=gfx.graph_buffer(&graph,{4096,{.Transfer_Destination,.Readback}},false,true)
    fill,_:=gfx.graph_pass(&graph,"fill",.Compute,{{scratch,{0,4096},.Write,.Storage}})
    transform,_:=gfx.graph_pass(&graph,"transform",.Compute,{{scratch,{0,4096},.Read_Write,.Storage}})
    copy_pass,_:=gfx.graph_pass(&graph,"copy",.Transfer,{{scratch,{0,4096},.Read,.Transfer_Source},{result,{0,4096},.Write,.Transfer_Destination}})
    plan,err:=gfx.graph_compile(&graph); assert(err==.None); defer gfx.compiled_graph_destroy(&plan)
    assert(len(plan.order)==3 && len(plan.hazards)==3)
    frames:gfx.Frames; gfx.frames_init(&frames,1); defer gfx.frames_destroy(&frames)
    for cycle in 0..<8 {
        token,acquired:=gfx.frame_acquire(&frames,0); assert(acquired==.None)
        send(nil,allocator,"reset")
        send(nil,command,"beginCommandBufferWithAllocator:",allocator)
        send(nil,command,"useResidencySet:",residency)
        for pass in plan.order {
            encoder:=send(^NS.Object,command,"computeCommandEncoder"); assert(encoder!=nil)
            stage:=u64(1<<28) if pass==copy_pass else u64(1<<27)
            after:u64
            for hazard in plan.hazards { if hazard.after==pass { after|=u64(1<<28) if hazard.before==copy_pass else u64(1<<27) } }
            if after!=0 { send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",after,stage,NS.UInteger(1)) }
            if pass==fill || pass==transform {
                pipeline:=pipelines[0] if pass==fill else pipelines[1]
                send(nil,encoder,"setComputePipelineState:",pipeline)
                send(nil,encoder,"setArgumentTable:",table)
                send(nil,encoder,"dispatchThreads:threadsPerThreadgroup:",MTL.Size{1024,1,1},MTL.Size{64,1,1})
            } else {
                send(nil,encoder,"copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",data,NS.UInteger(0),output,NS.UInteger(0),NS.UInteger(4096))
            }
            send(nil,encoder,"endEncoding")
        }
        send(nil,command,"endCommandBuffer")
        assert(gfx.frame_recorded(&frames,token)==.None)
        options:=new_object("MTL4CommitOptions")
        completion:Completion
        sync.wait_group_add(&completion.wait_group,1)
        block:=NS.Block.createLocalWithParam(&completion,feedback)
        send(nil,options,"addFeedbackHandler:",block)
        commands:=[1]^NS.Object{command}
        send(nil,queue,"commit:count:options:",raw_data(commands[:]),NS.UInteger(1),options)
        submission,accepted:=gfx.frame_submitted(&frames,token); assert(accepted==.None)
        sync.wait_group_wait(&completion.wait_group)
        if completion.failed { fmt.eprintln("GPU failure:",completion.code); panic("Metal 4 commit failed") }
        options->release(); block->release()
        assert(gfx.frame_completed(&frames,token,submission)==.None)
        pixels:=cast([^]u32)raw_data(output->contents())
        for i in 0..<1024 { assert(pixels[i]==(u32(i)*3+7)*5+11) }
        fmt.printf("Metal 4 cycle %d: 1024 values verified, submission %d\n",cycle,submission)
    }
}
