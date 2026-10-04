#+test
package shader
import "core:testing"
SOURCE :: "@compute @workgroup_size(1) fn main(){}"
@(test)
test_unloaded_dependency_and_invalid_requests_fail_explicitly :: proc(t:^testing.T) {
    compiler:Compiler
    testing.expect_value(t,compiler_init(&compiler,"/missing/katla-shader-compiler"),Error.Load_Failed)
    artifact,err:=compile(&compiler,SOURCE,{{"cs",.Compute}}); compiled_destroy(&artifact)
    testing.expect_value(t,err,Error.Closed)
    for constants in ([][]Constant{{{"same",1},{"same",2}},{{"",1}}}) {
        invalid,failed:=compile(&compiler,SOURCE,{{"cs",.Compute}},constants); compiled_destroy(&invalid)
        testing.expect_value(t,failed,Error.Invalid_Request)
    }
    empty,empty_error:=compile(&compiler,"",{{"cs",.Compute}}); compiled_destroy(&empty)
    testing.expect_value(t,empty_error,Error.Invalid_Request)
    duplicate,duplicate_error:=compile(&compiler,SOURCE,{{"cs",.Compute},{"cs",.Compute}}); compiled_destroy(&duplicate)
    testing.expect_value(t,duplicate_error,Error.Invalid_Request)
    testing.expect_value(t,compiler_destroy(&compiler),Error.None)
}
