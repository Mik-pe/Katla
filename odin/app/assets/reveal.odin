//! Reveal admits an actual confined selection before dispatching exact native argv.
package asset_browser
import resources "../../resources"
import "core:mem"
import "core:os"
import "core:strings"
import "core:path/filepath"

Reveal_Error :: enum { None,No_Selection,Invalid_Path,Filesystem,Unsupported_Platform,Launch_Failed }
Reveal_Result :: struct { error:Reveal_Error,filesystem_error:resources.Error,native_error:int }
Reveal_Command :: struct { arguments:[3]string,length:int,owned:string,allocator:mem.Allocator }
/// Releases the admitted command's single owned path argument.
reveal_command_destroy :: proc(command:^Reveal_Command) { delete(command.owned,command.allocator); command^={} }
/// Rechecks a displayed selection through the retained root; stale entries and links fail admission.
reveal_command :: proc(state:^State)->(Reveal_Command,Reveal_Result) {
    index,present:=entry_index(state,state.selected); if !present { return {},{error=.No_Selection} }
    root:=root_for(state); if root==nil || !resources.valid_relative_path(state.selected) { return {},{error=.Invalid_Path} }
    if error:=reveal_root_current(root); error!=.None { return {},{error=.Filesystem,filesystem_error=error} }
    if state.entries[index].kind==.Folder {
        inventory,error:=resources.list_directory(root,state.selected); if error!=.None { return {},{error=.Filesystem,filesystem_error=error} }; resources.directory_result_destroy(&inventory)
    } else { _,error:=resources.file_size(root,state.selected); if error!=.None { return {},{error=.Filesystem,filesystem_error=error} } }
    absolute:=strings.concatenate({root.path,"/",state.selected},state.allocator)
    command:=Reveal_Command{owned=absolute,allocator=state.allocator}
    when ODIN_OS==.Darwin { command.arguments={"/usr/bin/open","-R",absolute}; command.length=3 }
    else when ODIN_OS==.Linux { command.arguments={"/usr/bin/xdg-open",reveal_parent(absolute),""}; command.length=2 }
    else when ODIN_OS==.Windows {
        native,allocated:=strings.replace_all(absolute,"/","\\",state.allocator)
        command.owned=strings.concatenate({"/select,",native},state.allocator); delete(absolute,state.allocator); if allocated { delete(native,state.allocator) }
        command.arguments={"explorer.exe",command.owned,""}; command.length=2
    } else { reveal_command_destroy(&command); return {},{error=.Unsupported_Platform} }
    return command,{}
}
/// Reports native launch admission; the external file manager owns its lifetime after dispatch.
reveal :: proc(state:^State)->Reveal_Result {
    command,result:=reveal_command(state); if result.error!=.None { return result }; defer reveal_command_destroy(&command)
    native_error:=reveal_launch(command.arguments[:command.length],state.allocator)
    if native_error!=0 { return {error=.Launch_Failed,native_error=native_error} }; return {}
}

@(private="package")
reveal_parent :: proc(path:string)->string { return filepath.dir(path) }

@(private="package")
reveal_root_current :: proc(root:^resources.Root)->resources.Error {
    current,error:=resources.root_open(root.path,root.allocator); if error!=.None { return error }; defer resources.root_destroy(&current)
    retained,retained_error:=os.fstat(root.file,root.allocator); if retained_error!=nil { return .IO }; defer os.file_info_delete(retained,root.allocator)
    published,published_error:=os.fstat(current.file,root.allocator); if published_error!=nil { return .IO }; defer os.file_info_delete(published,root.allocator)
    if retained.inode==0 || published.inode==0 { return .IO }; if retained.device!=published.device || retained.inode!=published.inode { return .Invalid_Path }; return .None
}
