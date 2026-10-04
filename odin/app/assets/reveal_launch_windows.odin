#+build windows
//! Windows closes the admitted process handle; Explorer independently owns its external window.
package asset_browser
import "core:os"
import "core:mem"
import win "core:sys/windows"

@(private="package")
reveal_launch :: proc(arguments:[]string,allocator:mem.Allocator)->int {
    context.allocator=allocator
    process,error:=os.process_start({command=arguments})
    if error!=nil { return int(win.GetLastError()) if win.GetLastError()!=0 else 1 }
    if !win.CloseHandle(win.HANDLE(process.handle)) { return int(win.GetLastError()) }
    return 0
}
