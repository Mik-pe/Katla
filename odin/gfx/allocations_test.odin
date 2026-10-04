#+test
package gfx

import "core:testing"

Allocation_Fixture :: struct { buffers:Resource_Storage(i32,Buffer_Kind), images:Resource_Storage(i32,Texture_Kind), calls:int, fail_call:int }
allocation_buffer_requirements :: proc(_:rawptr,desc:Buffer_Desc)->(Memory_Requirements,Gpu_Error) { return {desc.size,16,3,desc.memory},.None }
allocation_image_requirements :: proc(_:rawptr,desc:Texture_Desc)->(Memory_Requirements,Gpu_Error) { return {u64(desc.width)*u64(desc.height)*4,64,2,.GPU_Private},.None }
allocation_fixture_allocate :: proc(state:rawptr,_:Allocation_Group,requests:[]Allocation_Request)->(Allocation_Result,Gpu_Error) {
    fixture:=cast(^Allocation_Fixture)state; fixture.calls+=1
    if fixture.calls==fixture.fail_call { return {},.Allocation_Failed }
    handles:=make([]Allocation_Handle,len(requests))
    for request,i in requests {
        switch resource in request {
        case Buffer_Allocation: handles[i]=storage_insert(&fixture.buffers,i32(resource.desc.size))
        case Image_Allocation: handles[i]=storage_insert(&fixture.images,i32(resource.desc.width))
        }
    }
    return {handles,context.allocator},.None
}
allocation_fixture_destroy_buffer :: proc(state:rawptr,handle:Buffer_Handle)->Gpu_Error { fixture:=cast(^Allocation_Fixture)state; _,ok:=storage_remove(&fixture.buffers,handle); return ok ? .None : .Invalid_Resource }
allocation_fixture_destroy_image :: proc(state:rawptr,handle:Texture_Handle)->Gpu_Error { fixture:=cast(^Allocation_Fixture)state; _,ok:=storage_remove(&fixture.images,handle); return ok ? .None : .Invalid_Resource }

@(test)
test_allocation_groups_native_compatibility_and_exact_lifetimes :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    a,_:=graph_buffer(&graph,{size=64,usage={.Storage},memory=.GPU_Private},false,false)
    b,_:=graph_buffer(&graph,{size=128,usage={.Storage},memory=.GPU_Private},false,false)
    imported,_:=graph_buffer(&graph,{size=16,usage={.Storage}},true,false)
    image,_:=graph_image(&graph,{8,8,1,1,.RGBA8_Unorm,{.Storage},1}, {},false,false)
    graph_pass(&graph,"first",.Compute,{{a,{0,64},.Write,.Storage},{imported,{0,16},.Read,.Storage}},side_effect=true)
    graph_pass(&graph,"second",.Compute,{{b,{0,128},.Write,.Storage}},side_effect=true)
    graph_pass(&graph,"image",.Compute,nil,side_effect=true,images={{image,{0,1,0,1,{.Color}},.Write,.Storage}})
    compiled,compile_error:=graph_compile(&graph); testing.expect_value(t,compile_error,Graph_Error.None); defer compiled_graph_destroy(&compiled)
    plan,error:=graph_allocation_plan(&graph,&compiled,{buffer=allocation_buffer_requirements,texture=allocation_image_requirements}); defer allocation_plan_destroy(&plan)
    testing.expect_value(t,error,Gpu_Error.None); testing.expect_value(t,len(plan.groups),1); testing.expect_value(t,len(plan.resources),3)
    testing.expect_value(t,plan.groups[0].memory_types,u32(2)); testing.expect_value(t,plan.groups[0].size,u64(256)); testing.expect_value(t,plan.groups[0].alignment,u64(64))
    fixture:Allocation_Fixture; storage_init(&fixture.buffers); storage_init(&fixture.images); defer storage_destroy(&fixture.buffers); defer storage_destroy(&fixture.images)
    api:=Allocation_API{&fixture,allocation_fixture_allocate,allocation_fixture_destroy_buffer,allocation_fixture_destroy_image}
    first,first_error:=graph_allocate(&plan,api); testing.expect_value(t,first_error,Gpu_Error.None)
    second,second_error:=graph_allocate(&plan,api); testing.expect_value(t,second_error,Gpu_Error.None)
    testing.expect_value(t,len(first.buffers),2); testing.expect_value(t,len(first.textures),1)
    testing.expect(t,first.buffers[0].handle!=second.buffers[0].handle)
    testing.expect_value(t,graph_allocations_destroy(&first),Gpu_Error.None); testing.expect_value(t,graph_allocations_destroy(&second),Gpu_Error.None)
}
@(test)
test_allocation_failure_rolls_back_prior_native_groups :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph); defer graph_destroy(&graph)
    a,_:=graph_buffer(&graph,{size=64,usage={.Storage}},false,true)
    b,_:=graph_buffer(&graph,{size=64,usage={.Storage}},false,true)
    graph_pass(&graph,"both",.Compute,{{a,{0,64},.Write,.Storage},{b,{0,64},.Write,.Storage}})
    compiled,_:=graph_compile(&graph); defer compiled_graph_destroy(&compiled)
    plan,error:=graph_allocation_plan(&graph,&compiled,{buffer=allocation_buffer_requirements}); testing.expect_value(t,error,Gpu_Error.None); defer allocation_plan_destroy(&plan)
    testing.expect_value(t,len(plan.groups),2)
    fixture:=Allocation_Fixture{fail_call=2}; storage_init(&fixture.buffers); storage_init(&fixture.images); defer storage_destroy(&fixture.buffers); defer storage_destroy(&fixture.images)
    owner,allocation_error:=graph_allocate(&plan,{&fixture,allocation_fixture_allocate,allocation_fixture_destroy_buffer,allocation_fixture_destroy_image})
    testing.expect_value(t,allocation_error,Gpu_Error.Allocation_Failed); testing.expect_value(t,fixture.buffers.count,0); testing.expect_value(t,len(owner.resources),0)
    graph_buffer(&graph,{size=4,usage={.Storage}},false,false)
    stale,stale_error:=graph_allocate(&plan,{&fixture,allocation_fixture_allocate,allocation_fixture_destroy_buffer,allocation_fixture_destroy_image})
    testing.expect_value(t,stale_error,Gpu_Error.Invalid_Graph); testing.expect_value(t,len(stale.resources),0)
}
