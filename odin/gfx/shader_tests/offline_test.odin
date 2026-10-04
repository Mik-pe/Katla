#+test
package shader_tests

import shader "../shader"
import "core:testing"
import "core:os"
import "core:path/filepath"
import "core:mem"

@(test)
test_offline_cache_reuses_normalized_options_and_recovers_corrupt_artifact :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-shader-cache-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,SHADER_COMPILER,directory),shader.Error.None); defer shader.compiler_destroy(&compiler)
    source:=`override COUNT:u32=2; override EXTRA:u32=3; @compute @workgroup_size(COUNT) fn main(){let y=EXTRA;}`
    first,first_error:=shader.compile(&compiler,source,{{"main",.Compute}},{{"COUNT",4},{"EXTRA",7}}); defer shader.compiled_destroy(&first)
    testing.expect_value(t,first_error,shader.Error.None)
    second,second_error:=shader.compile(&compiler,source,{{"main",.Compute}},{{"EXTRA",7},{"COUNT",4}}); defer shader.compiled_destroy(&second)
    testing.expect_value(t,second_error,shader.Error.None); testing.expect(t,compiler.process_runs==1 && compiler.cache_hits==1 && second.entries[0].workgroup_size[0]==4)
    dir,dir_error:=os.open(directory); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer os.close(dir)
    iterator:=os.read_directory_iterator_create(dir); defer os.read_directory_iterator_destroy(&iterator)
    changed:=false
    for info,_ in os.read_directory_iterator(&iterator) {
        if info.type!=.Regular || len(info.name)!=64 { continue }
        path,join_error:=filepath.join({directory,info.name}); testing.expect(t,join_error==nil); defer delete(path)
        data,read_error:=os.read_entire_file(path,context.allocator); testing.expect(t,read_error==nil); defer delete(data)
        if len(data)>130 { data[65]='x'; testing.expect(t,os.write_entire_file(path,data)==nil); changed=true; break }
    }
    testing.expect(t,changed)
    repaired,repaired_error:=shader.compile(&compiler,source,{{"main",.Compute}},{{"COUNT",4},{"EXTRA",7}}); defer shader.compiled_destroy(&repaired)
    testing.expect_value(t,repaired_error,shader.Error.None); testing.expect(t,compiler.process_runs==2 && repaired.entries[0].workgroup_size[0]==4)
    updated,updated_error:=shader.compile(&compiler,source,{{"main",.Compute}},{{"COUNT",8},{"EXTRA",7}}); defer shader.compiled_destroy(&updated)
    testing.expect_value(t,updated_error,shader.Error.None); testing.expect(t,compiler.process_runs==3 && updated.entries[0].workgroup_size[0]==8 && first.entries[0].workgroup_size[0]==4)
}
@(private="file")
Offline_Source :: struct { files:map[string]string }
@(private="file")
offline_source_read :: proc(state:rawptr,path:string,allocator:mem.Allocator)->([]byte,bool) {
    sources:=cast(^Offline_Source)state; value,present:=sources.files[path]; if !present { return nil,false }
    data:=make([]byte,len(value),allocator); copy(data,transmute([]byte)value); return data,true
}
@(test)
test_offline_transitive_include_change_invalidates_and_escape_rejects :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-shader-includes-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    source:=Offline_Source{make(map[string]string)}; defer delete(source.files)
    source.files["shaders/pass/main.wgsl"]="#include <constants.wgsl>\n#include \"helper.wgsl\"\n@compute @workgroup_size(COUNT) fn main(){helper();}"
    source.files["shaders/common/constants.wgsl"]="const COUNT:u32=2;"
    source.files["shaders/pass/helper.wgsl"]="#include <constants.wgsl>\nfn helper(){}"
    loader:=shader.Source_Loader{&source,offline_source_read}
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,SHADER_COMPILER,directory),shader.Error.None); defer shader.compiler_destroy(&compiler)
    first,first_error:=shader.compile_source_file(&compiler,loader,"shaders/pass/main.wgsl",{{"main",.Compute}}); defer shader.compiled_destroy(&first)
    testing.expect_value(t,first_error,shader.Error.None)
    second,second_error:=shader.compile_source_file(&compiler,loader,"shaders/pass/main.wgsl",{{"main",.Compute}}); defer shader.compiled_destroy(&second)
    testing.expect(t,second_error==.None && compiler.process_runs==1 && compiler.cache_hits==1)
    source.files["shaders/common/constants.wgsl"]="const COUNT:u32=5;"
    third,third_error:=shader.compile_source_file(&compiler,loader,"shaders/pass/main.wgsl",{{"main",.Compute}}); defer shader.compiled_destroy(&third)
    testing.expect(t,third_error==.None && third.entries[0].workgroup_size[0]==5 && compiler.process_runs==2 && first.entries[0].workgroup_size[0]==2)
    source.files["shaders/pass/helper.wgsl"]="#include \"../../../escape.wgsl\""
    failed,failed_error:=shader.compile_source_file(&compiler,loader,"shaders/pass/main.wgsl",{{"main",.Compute}}); defer shader.compiled_destroy(&failed)
    testing.expect(t,failed_error==.Invalid_Request && compiler.process_runs==2)
}
