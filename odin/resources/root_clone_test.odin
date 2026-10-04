#+test
package resources
import "core:testing"
import "core:os"
import "core:path/filepath"

@(test)
test_root_clone_retains_directory_after_original_owner_closes :: proc(t:^testing.T) {
    directory,error:=os.mkdir_temp("","katla-root-clone",context.allocator);testing.expect(t,error==nil);if error!=nil { return };defer { _=os.remove_all(directory);delete(directory) }
    filename,_:=filepath.join({directory,"sample.bin"});defer delete(filename)
    testing.expect(t,os.write_entire_file(filename,([]byte{3,7,11}))==nil)
    original,original_error:=root_open(directory);testing.expect_value(t,original_error,Error.None);if original_error!=.None { return }
    retained,retain_error:=root_clone(&original);root_destroy(&original);testing.expect_value(t,retain_error,Error.None);if retain_error!=.None { return };defer root_destroy(&retained)
    bytes,read_error:=read_bytes(&retained,"sample.bin");defer delete(bytes,retained.allocator)
    testing.expect_value(t,read_error,Error.None);testing.expect(t,len(bytes)==3&&bytes[0]==3&&bytes[1]==7&&bytes[2]==11)
}
