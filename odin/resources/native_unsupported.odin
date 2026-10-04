#+build windows
//! Unsupported platforms fail rather than traversing unconfined paths.
package resources
import "core:os"
@(private="package")
root_open_native :: proc(path:string)->(^os.File,Error) { return nil,.Unsupported_Platform }
@(private="package")
open_child_native :: proc(parent:^os.File,name:string,directory:bool)->(^os.File,Error) { return nil,.Unsupported_Platform }
@(private="package")
open_relative_native :: proc(root:^Root,path:string,directory:=false)->(^os.File,Error) { return nil,.Unsupported_Platform }

@(private="package")
write_atomic_native :: proc(root:^Root,path:string,data:[]byte)->(bool,Error) { return false,.Unsupported_Platform }
