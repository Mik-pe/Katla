#+test
package shader_tests
import shader "../shader"

import "core:testing"
import spirv "../spirv"
import "core:mem"
import "core:time"
import "core:thread"

NAGA_LIBRARY :: #config(NAGA_LIBRARY, "")
SOURCE :: `
struct Frame { transform:mat4x4f, tint:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(2) @binding(3) var<storage,read_write> values:array<u32>;
@group(3) @binding(2) var image:texture_2d<f32>;
@group(3) @binding(5) var image_sampler:sampler;
@group(7) @binding(0) var<uniform> unused:vec4f;
@vertex fn vs(@builtin(vertex_index) index:u32)->@builtin(position) vec4f { return frame.transform*vec4f(f32(index),0,0,1); }
@fragment fn fs(@builtin(position) pos:vec4f)->@location(2) vec4f { return textureSample(image,image_sampler,pos.xy)*frame.tint; }
override BLOCK:u32=4;
@compute @workgroup_size(BLOCK) fn cs(@builtin(global_invocation_id) id:vec3u){ if(id.x<arrayLength(&values)){values[id.x]=id.x*7u;} }
`
@(test)
test_canonical_wgsl_selected_entries_and_owned_reflection :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    testing.expect(t,len(NAGA_LIBRARY)>0,"Run scripts/validate_odin_shader.py to select the compiler dependency")
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None)
    artifact,err:=shader.compile(&compiler,SOURCE,{{"vs",.Vertex},{"fs",.Fragment},{"cs",.Compute}},{{"BLOCK",8}})
    testing.expect(t,err==.None,artifact.message)
    if err==.None {
        testing.expect_value(t,len(artifact.entries),3)
        vs,fs,cs:=artifact.entries[0],artifact.entries[1],artifact.entries[2]
        testing.expect_value(t,len(vs.bindings),1); testing.expect_value(t,vs.bindings[0].minimum_size,u64(80))
        testing.expect_value(t,len(fs.bindings),3); testing.expect_value(t,fs.outputs[0].location,i32(2))
        testing.expect_value(t,len(cs.bindings),1); testing.expect_value(t,cs.workgroup_size,([3]u32{8,1,1}))
        testing.expect(t,cs.bindings[0].runtime_array && cs.bindings[0].runtime_array_stride==4 && cs.bindings[0].access==.Write && cs.bindings[0].declared_access==.Read_Write && cs.sizes_buffer==8)
    }
    shader.compiled_destroy(&artifact)
    testing.expect_value(t,shader.compiler_destroy(&compiler),shader.Error.None)
    testing.expect_value(t,len(tracker.allocation_map),0)
}
@(test)
test_compiler_failures_are_owned_and_never_return_partial_entries :: proc(t:^testing.T) {
    testing.expect(t,len(NAGA_LIBRARY)>0,"Run scripts/validate_odin_shader.py to select the compiler dependency")
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    for example in ([]struct { source:string, entry:shader.Selection, expected:shader.Error }{
        {"invalid wgsl",{"main",.Compute},.Parse},
        {SOURCE,{"absent",.Vertex},.Missing_Entry},
        {"@compute @workgroup_size(1) fn main(){ let x:f32=vec3f(1); }",{"main",.Compute},.Parse},
    }) {
        artifact,err:=shader.compile(&compiler,example.source,{example.entry}); defer shader.compiled_destroy(&artifact)
        testing.expect_value(t,err,example.expected); testing.expect(t,len(artifact.entries)==0 && len(artifact.message)>0)
    }
    artifact,err:=shader.compile(&compiler,SOURCE,{{"cs",.Compute}},{{"MISSING",4}}); defer shader.compiled_destroy(&artifact)
    testing.expect_value(t,err,shader.Error.Constants)
}
@(test)
test_async_replacement_capacity_latest_failure_and_shutdown :: proc(t:^testing.T) {
    testing.expect(t,len(NAGA_LIBRARY)>0,"Run scripts/validate_odin_shader.py to select the compiler dependency")
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    credit:shader.Service; testing.expect_value(t,shader.service_init(&credit,&compiler,1),shader.Service_Error.None)
    shader.service_submit(&credit,1,SOURCE,{{"cs",.Compute}})
    shader.service_close(&credit); thread.join(credit.worker)
    testing.expect_value(t,credit.outstanding,1)
    shader.service_destroy(&credit)
    full_queue:shader.Service; testing.expect_value(t,shader.service_init(&full_queue,&compiler,1),shader.Service_Error.None)
    shader.service_submit(&full_queue,1,SOURCE,{{"cs",.Compute}})
    _,full:=shader.service_submit(&full_queue,2,SOURCE,{{"cs",.Compute}}); testing.expect_value(t,full,shader.Service_Error.Full)
    shader.service_destroy(&full_queue)
    service:shader.Service; testing.expect_value(t,shader.service_init(&service,&compiler,2),shader.Service_Error.None); defer shader.service_destroy(&service)
    first,err:=shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",2}}); testing.expect(t,first>0 && err==.None)
    last,second:=shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",16}}); testing.expect(t,last>first && second==.None)
    started:=time.tick_now()
    got:=false
    for time.tick_since(started)<10*time.Second {
        result,ok:=shader.service_take_result(&service)
        if ok { testing.expect(t,result.key==1 && result.revision==last && result.error==.None && result.compiled.entries[0].workgroup_size[0]==16); shader.replacement_destroy(&result); got=true; break }
        thread.yield()
    }
    testing.expect(t,got)
    failed,accepted:=shader.service_submit(&service,1,"invalid",{{"cs",.Compute}}); testing.expect_value(t,accepted,shader.Service_Error.None)
    shader.service_close(&service)
    _,closed:=shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}}); testing.expect_value(t,closed,shader.Service_Error.Closed)
    thread.join(service.worker)
    result,ok:=shader.service_take_result(&service)
    testing.expect(t,ok && result.revision==failed && result.error==.Parse && len(result.compiled.entries)==0)
    shader.replacement_destroy(&result)
}

@(test)
test_native_spirv_reflection_matches_compiled_entry_contract :: proc(t:^testing.T) {
    testing.expect(t,len(NAGA_LIBRARY)>0,"Run scripts/validate_odin_shader.py to select the compiler dependency")
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    artifact,err:=shader.compile(&compiler,SOURCE,{{"vs",.Vertex},{"fs",.Fragment},{"cs",.Compute}}); defer shader.compiled_destroy(&artifact)
    testing.expect(t,err==.None,artifact.message)
    for entry in artifact.entries {
        reflected,reflection_error:=spirv.reflect_entry(entry.spirv,entry.name,spirv.Stage(entry.stage)); defer spirv.stage_destroy(&reflected)
        testing.expect_value(t,reflection_error,spirv.Error.None)
        testing.expect_value(t,len(reflected.resources),len(entry.bindings))
        for expected,i in entry.bindings {
            if i>=len(reflected.resources) { continue }
            actual:=reflected.resources[i]
            testing.expect(t,actual.group==expected.group && actual.binding==expected.binding)
            testing.expect(t,int(actual.kind)==int(expected.kind) && actual.array_count==expected.array_count)
            if actual.kind==.Buffer { testing.expect(t,actual.storage==!expected.uniform && actual.minimum_size==expected.minimum_size) }
            if expected.kind!=.Sampler { testing.expect_value(t,string_access(actual.access),expected.access) }
            if actual.kind==.Image { testing.expect(t,actual.depth==expected.depth && actual.arrayed==expected.arrayed && actual.multisampled==expected.multisampled) }
        }
        if entry.stage==.Fragment { testing.expect_value(t,len(reflected.color_outputs),1); testing.expect_value(t,reflected.color_outputs[0],u32(2)) }
        if entry.stage==.Compute { testing.expect_value(t,reflected.local_size,entry.workgroup_size) }
    }
}


@(test)
test_native_depth_comparison_and_storage_image_reflection :: proc(t:^testing.T) {
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    source:=`
@group(2) @binding(0) var depth_image:texture_depth_2d;
@group(2) @binding(1) var depth_sampler:sampler_comparison;
@group(3) @binding(0) var destination:texture_storage_2d<rgba8unorm,write>;
fn lookup(image:texture_depth_2d,comparison:sampler_comparison)->f32 {return textureSampleCompareLevel(image,comparison,vec2f(0.5),0.3);}
@compute @workgroup_size(1) fn main(){let depth=lookup(depth_image,depth_sampler);textureStore(destination,vec2i(0),vec4f(depth));}
`
    artifact,err:=shader.compile(&compiler,source,{{"main",.Compute}}); defer shader.compiled_destroy(&artifact)
    testing.expect(t,err==.None,artifact.message)
    if err!=.None { return }
    reflected,reflection_error:=spirv.reflect_entry(artifact.entries[0].spirv,"main",.Compute); defer spirv.stage_destroy(&reflected)
    testing.expect_value(t,reflection_error,spirv.Error.None)
    testing.expect_value(t,len(reflected.resources),3)
    if len(reflected.resources)==3 {
        testing.expect(t,reflected.resources[0].kind==.Image && reflected.resources[0].depth && !reflected.resources[0].storage)
        testing.expect(t,reflected.resources[1].kind==.Sampler && reflected.resources[1].comparison)
        testing.expect(t,reflected.resources[2].kind==.Image && reflected.resources[2].storage && reflected.resources[2].format==4 && reflected.resources[2].access==.Write)
    }
}

@(private="package")
string_access :: proc(access:spirv.Access)->shader.Access {
    switch access {
    case .Read: return .Read
    case .Write: return .Write
    case .Read_Write: return .Read_Write
    case .None: return .None
    }
    return .None
}
