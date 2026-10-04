#+test
package render

import shader "../../gfx/shader"
import "core:mem"
import "core:os"
import "core:path/filepath"
import "core:testing"
import "core:time"
import "core:sync"

@(private="file")
RELOAD_TEST_COMPILER :: #config(SHADER_COMPILER,"")
@(private="file")
Reload_Test_Pack :: struct { sizes:[2]u32,allocator:mem.Allocator }
@(private="file")
Reload_Test_Owner :: struct { current:^Reload_Test_Pack,prepared,destroyed,published:int,fail:bool,allocator:mem.Allocator }
@(private="file")
reload_test_prepare :: proc(state:rawptr,artifacts:[]shader.Compiled)->(rawptr,Shader_Reload_Error) {
    owner:=cast(^Reload_Test_Owner)state
    pack:=new(Reload_Test_Pack,owner.allocator);pack.allocator=owner.allocator;owner.prepared+=1
    if len(artifacts)!=2 { return pack,.Prepare }
    for artifact,index in artifacts { if len(artifact.entries)!=1 { return pack,.Prepare };pack.sizes[index]=artifact.entries[0].workgroup_size[0] }
    return pack,.Prepare if owner.fail else .None
}
@(private="file")
reload_test_publish :: proc(state,candidate:rawptr)->rawptr {
    owner:=cast(^Reload_Test_Owner)state;previous:=owner.current;owner.current=cast(^Reload_Test_Pack)candidate;owner.published+=1;return previous
}
@(private="file")
reload_test_destroy :: proc(state,candidate:rawptr) { owner:=cast(^Reload_Test_Owner)state;owner.destroyed+=1;pack:=cast(^Reload_Test_Pack)candidate;free(pack,pack.allocator) }
@(private="file")
reload_test_write :: proc(root,name,source:string) { path,error:=filepath.join({root,name});assert(error==nil);defer delete(path);assert(os.write_entire_file(path,transmute([]byte)source)==nil) }
@(private="file")
reload_test_wait :: proc(service:^Shader_Reload_Service,frame:^u64)->Shader_Reload_Status {
    started:=time.tick_now()
    for time.tick_since(started)<10*time.Second {
        frame^+=1;status:=shader_reload_poll(service,frame^)
        if status.published!=0 || status.failed!=0 { return status }
        time.sleep(time.Millisecond)
    }
    panic("asynchronous shader family did not finish")
}
@(test)
test_shader_reload_transitive_snapshot_atomic_failure_and_recovery :: proc(t:^testing.T) {
    root,error:=os.make_directory_temp("","katla-app-shader-reload-*",context.allocator);assert(error==nil);defer { os.remove_all(root);delete(root) }
    cache,cache_error:=filepath.join({root,"cache"});assert(cache_error==nil);defer delete(cache)
    compiler:shader.Compiler;testing.expect_value(t,shader.compiler_init(&compiler,RELOAD_TEST_COMPILER,cache),shader.Error.None);assert(compiler.executable!="","Pass -define:SHADER_COMPILER for actual asynchronous source reload acceptance");defer shader.compiler_destroy(&compiler)
    reload_test_write(root,"common.wgsl","const COUNT:u32=2;")
    source:="// #include common\n@compute @workgroup_size(COUNT) fn main(){}"
    reload_test_write(root,"first.wgsl",source);reload_test_write(root,"second.wgsl",source)
    service:Shader_Reload_Service;testing.expect_value(t,shader_reload_init(&service,&compiler,root),Shader_Reload_Error.None);defer shader_reload_destroy(&service)
    owner:=Reload_Test_Owner{allocator=context.allocator};defer { if owner.current!=nil { reload_test_destroy(&owner,owner.current) } }
    family,registered:=shader_reload_register(&service,{name="atomic-pair",modules={{path="first.wgsl",selections={{"main",.Compute}}},{path="second.wgsl",selections={{"main",.Compute}}}},publisher={&owner,reload_test_prepare,reload_test_publish,reload_test_destroy}})
    testing.expect(t,family==0 && registered==.None)
    frame:u64;status:=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{2,2})
    runs:=compiler.process_runs
    for _ in 0..<4 { testing.expect_value(t,shader_reload_poll(&service,frame).changed,0);frame+=1;testing.expect_value(t,shader_reload_poll(&service,frame).changed,0) }
    testing.expect(t,compiler.process_runs==runs && owner.published==1)
    reload_test_write(root,"common.wgsl","const COUNT:u32=5;")
    status=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{5,5})
    stable:=owner.current;published:=owner.published
    reload_test_write(root,"second.wgsl","this is not WGSL")
    status=reload_test_wait(&service,&frame);testing.expect(t,status.error==.Compile && owner.current==stable && owner.published==published)
    failed_runs:=compiler.process_runs
    for _ in 0..<4 { frame+=1;shader_reload_poll(&service,frame) };testing.expect_value(t,compiler.process_runs,failed_runs)
    reload_test_write(root,"second.wgsl",source);owner.fail=true
    status=reload_test_wait(&service,&frame);testing.expect(t,status.error==.Prepare && owner.current==stable && owner.published==published && owner.destroyed==2)
    owner.fail=false;testing.expect_value(t,shader_reload_set_options(&service,family,{1}),Shader_Reload_Error.None)
    status=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{5,5})
}
@(test)
test_shader_reload_missing_decode_and_override_keep_current_and_resume :: proc(t:^testing.T) {
    root,error:=os.make_directory_temp("","katla-app-shader-override-*",context.allocator);assert(error==nil);defer { os.remove_all(root);delete(root) }
    cache,cache_error:=filepath.join({root,"cache"});assert(cache_error==nil);defer delete(cache)
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,RELOAD_TEST_COMPILER,cache)==.None);defer shader.compiler_destroy(&compiler)
    source:="override COUNT:u32=2; @compute @workgroup_size(COUNT) fn main(){}"
    reload_test_write(root,"first.wgsl",source);reload_test_write(root,"second.wgsl",source)
    service:Shader_Reload_Service;assert(shader_reload_init(&service,&compiler,root)==.None);defer shader_reload_destroy(&service)
    owner:=Reload_Test_Owner{allocator=context.allocator};defer { if owner.current!=nil { reload_test_destroy(&owner,owner.current) } }
    family,error_family:=shader_reload_register(&service,{name="options",modules={{path="first.wgsl",selections={{"main",.Compute}}},{path="second.wgsl",selections={{"main",.Compute}}}},publisher={&owner,reload_test_prepare,reload_test_publish,reload_test_destroy}});assert(error_family==.None)
    frame:u64;status:=reload_test_wait(&service,&frame);testing.expect_value(t,status.published,1)
    stable:=owner.current;runs:=compiler.process_runs
    reload_test_write(root,"first.wgsl","\xff\xfe")
    frame+=1;status=shader_reload_poll(&service,frame);testing.expect(t,status.error==.Source && owner.current==stable && compiler.process_runs==runs)
    reload_test_write(root,"first.wgsl",source)
    status=reload_test_wait(&service,&frame);testing.expect_value(t,status.published,1);testing.expect_value(t,compiler.process_runs,runs)
    testing.expect_value(t,shader_reload_set_constants(&service,family,1,{{"COUNT",7}}),Shader_Reload_Error.None)
    status=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{2,7} && compiler.process_runs==runs+1)
    unchanged_runs:=compiler.process_runs;unchanged_published:=owner.published
    testing.expect_value(t,shader_reload_set_constants(&service,family,1,{{"COUNT",7}}),Shader_Reload_Error.None)
    frame+=1;status=shader_reload_poll(&service,frame);testing.expect(t,status.changed==0 && owner.published==unchanged_published && compiler.process_runs==unchanged_runs)
    testing.expect_value(t,shader_reload_set_constants(&service,family,1,{{"COUNT",1},{"COUNT",2}}),Shader_Reload_Error.Invalid_Config)
    frame+=1;status=shader_reload_poll(&service,frame);testing.expect(t,status.changed==0 && owner.current.sizes==[2]u32{2,7})
    missing,error_missing:=filepath.join({root,"first.wgsl"});assert(error_missing==nil);defer delete(missing);assert(os.remove(missing)==nil)
    frame+=1;status=shader_reload_poll(&service,frame);testing.expect_value(t,status.error,Shader_Reload_Error.Source)
    reload_test_write(root,"first.wgsl",source);status=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{2,7})
}
@(test)
test_shader_reload_registration_rejects_invalid_requests_without_admission :: proc(t:^testing.T) {
    root,error:=os.make_directory_temp("","katla-app-shader-config-*",context.allocator);assert(error==nil);defer { os.remove_all(root);delete(root) }
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,RELOAD_TEST_COMPILER)==.None);defer shader.compiler_destroy(&compiler)
    service:Shader_Reload_Service;assert(shader_reload_init(&service,&compiler,root)==.None);defer shader_reload_destroy(&service)
    owner:=Reload_Test_Owner{allocator=context.allocator};publisher:=Shader_Reload_Publisher{&owner,reload_test_prepare,reload_test_publish,reload_test_destroy}
    configurations:=[4]Shader_Reload_Family{
        {name="empty-entry",modules={{path="first.wgsl",selections={{"",.Compute}}}},publisher=publisher},
        {name="duplicate-entry",modules={{path="first.wgsl",selections={{"main",.Compute},{"main",.Compute}}}},publisher=publisher},
        {name="invalid-stage",modules={{path="first.wgsl",selections={{"main",cast(shader.Stage)99}}}},publisher=publisher},
        {name="duplicate-override",modules={{path="first.wgsl",selections={{"main",.Compute}},constants={{"COUNT",1},{"COUNT",2}}}},publisher=publisher},
    }
    for config in configurations {
        index,registration_error:=shader_reload_register(&service,config)
        testing.expect(t,index== -1 && registration_error==.Invalid_Config && len(service.families)==0)
    }
    testing.expect(t,compiler.process_runs==0 && service.next_key==0 && owner.prepared==0)
}
@(test)
test_shader_reload_changed_and_missing_compiler_preserve_accepted_family :: proc(t:^testing.T) {
    root,error:=os.make_directory_temp("","katla-app-compiler-reload-*",context.allocator);assert(error==nil);defer { os.remove_all(root);delete(root) }
    executable,path_error:=filepath.join({root,"compiler.exe"});assert(path_error==nil);defer delete(executable)
    cache,cache_error:=filepath.join({root,"cache"});assert(cache_error==nil);defer delete(cache)
    assert(os.copy_file(executable,RELOAD_TEST_COMPILER)==nil)
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,executable,cache)==.None);defer shader.compiler_destroy(&compiler)
    source:="@compute @workgroup_size(3) fn main(){}"
    reload_test_write(root,"first.wgsl",source);reload_test_write(root,"second.wgsl",source)
    service:Shader_Reload_Service;assert(shader_reload_init(&service,&compiler,root)==.None);defer shader_reload_destroy(&service)
    owner:=Reload_Test_Owner{allocator=context.allocator};defer { if owner.current!=nil { reload_test_destroy(&owner,owner.current) } }
    _,registration_error:=shader_reload_register(&service,{name="compiler",modules={{path="first.wgsl",selections={{"main",.Compute}}},{path="second.wgsl",selections={{"main",.Compute}}}},publisher={&owner,reload_test_prepare,reload_test_publish,reload_test_destroy}});assert(registration_error==.None)
    frame:u64;status:=reload_test_wait(&service,&frame);testing.expect(t,status.published==1 && owner.current.sizes==[2]u32{3,3})
    stable:=owner.current;runs:=compiler.process_runs;identity:=service.compiler_digest
    assert(os.write_entire_file(executable,"invalid executable replacement")==nil)
    status=reload_test_wait(&service,&frame);testing.expect(t,status.error==.Compile && owner.current==stable && service.compiler_digest!=identity)
    assert(os.remove(executable)==nil);frame+=1;status=shader_reload_poll(&service,frame)
    testing.expect(t,status.error==.Compiler && owner.current==stable && compiler.process_runs==runs)
    started:=time.tick_now()
    for {
        sync.mutex_lock(&service.worker.mutex);outstanding:=service.worker.outstanding;sync.mutex_unlock(&service.worker.mutex)
        if outstanding==0 { break }
        assert(time.tick_since(started)<10*time.Second)
        time.sleep(time.Millisecond);frame+=1;shader_reload_poll(&service,frame)
    }
    assert(os.copy_file(executable,RELOAD_TEST_COMPILER)==nil)
    status=reload_test_wait(&service,&frame)
    testing.expect_value(t,status.published,1)
    testing.expect_value(t,owner.current.sizes,([2]u32{3,3}))
    testing.expect_value(t,service.compiler_digest,identity)
    testing.expect_value(t,compiler.process_runs,runs)
}
