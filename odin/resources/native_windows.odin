#+build windows
//! NT handle-relative operations preserve confinement across directory renames and reject reparses.
package resources

import "core:os"
import "core:strings"
import "core:crypto"
import "core:unicode/utf16"
import win "core:sys/windows"

foreign import resource_nt "system:ntdll.lib"
@(default_calling_convention="system")
foreign resource_nt {
    NtSetInformationFile :: proc(handle:win.HANDLE,status:^win.IO_STATUS_BLOCK,information:rawptr,length:win.ULONG,kind:win.FILE_INFORMATION_CLASS)->win.NTSTATUS ---
}
@(private="package")
WINDOWS_OPEN :: u32(1)
@(private="package")
WINDOWS_CREATE :: u32(2)
@(private="package")
WINDOWS_OPEN_IF :: u32(3)
@(private="package")
windows_handle :: proc(file:^os.File)->win.HANDLE { return win.HANDLE(os.fd(file)) }
@(private="package")
windows_attributes :: proc(handle:win.HANDLE)->(win.FILE_ATTRIBUTE_TAG_INFO,Error) {
    info:win.FILE_ATTRIBUTE_TAG_INFO
    if !bool(win.GetFileInformationByHandleEx(handle,.FileAttributeTagInfo,&info,u32(size_of(info)))) { return {},.IO }
    return info,.None
}
@(private="package")
windows_wrap :: proc(handle:win.HANDLE,name:string)->(^os.File,Error) {
    file:=os.new_file(uintptr(handle),name)
    if file==nil { win.CloseHandle(handle); return nil,.IO }; return file,.None
}
@(private="package")
root_open_native :: proc(path:string)->(^os.File,Error) {
    name:=win.utf8_to_wstring(path,context.allocator); if name==nil { return nil,.Invalid_Path }; defer delete(name)
    handle:=win.CreateFileW(name,win.FILE_LIST_DIRECTORY|win.FILE_READ_ATTRIBUTES|win.FILE_TRAVERSE|win.SYNCHRONIZE,win.FILE_SHARE_READ|win.FILE_SHARE_WRITE|win.FILE_SHARE_DELETE,nil,win.OPEN_EXISTING,win.FILE_FLAG_BACKUP_SEMANTICS|win.FILE_FLAG_OPEN_REPARSE_POINT,nil)
    if handle==win.INVALID_HANDLE_VALUE { return nil,.IO }
    info,error:=windows_attributes(handle)
    if error!=.None || info.FileAttributes&win.FILE_ATTRIBUTE_REPARSE_POINT!=0 || info.FileAttributes&win.FILE_ATTRIBUTE_DIRECTORY==0 { win.CloseHandle(handle); return nil,.Not_Regular }
    return windows_wrap(handle,path)
}
@(private="package")
windows_open_child :: proc(parent:^os.File,name:string,directory:bool,disposition: u32=WINDOWS_OPEN,write:bool=false,remove:bool=false,allow_link:bool=false)->(^os.File,Error,win.NTSTATUS) {
    if parent==nil || (!valid_relative_path(name) && name!=".") || strings.contains(name,"/") { return nil,.Invalid_Path,0 }
    encoded:=win.utf8_to_utf16(name,context.allocator); if len(encoded)==0 || len(encoded)>32766 { delete(encoded); return nil,.Invalid_Path,0 }; defer delete(encoded)
    unicode:=win.UNICODE_STRING{Length=u16(len(encoded)*2),MaximumLength=u16(len(encoded)*2),Buffer=raw_data(encoded)}
    attributes:=win.OBJECT_ATTRIBUTES{Length=u32(size_of(win.OBJECT_ATTRIBUTES)),RootDirectory=windows_handle(parent),ObjectName=&unicode,Attributes=0x40}
    access:=win.FILE_READ_ATTRIBUTES|win.SYNCHRONIZE
    if directory { access|=win.FILE_LIST_DIRECTORY|win.FILE_TRAVERSE } else { access|=win.FILE_READ_DATA }
    if write { access|=win.FILE_WRITE_DATA|win.FILE_WRITE_ATTRIBUTES }; if remove { access|=win.DELETE }
    options:u32=0x00200000|0x20 // Open the reparse point itself and use synchronous nonalert I/O.
    if directory { options|=1 } else if !allow_link { options|=0x40 }
    if write { options|=2 }
    handle:win.HANDLE; status:win.IO_STATUS_BLOCK
    outcome:=win.NtCreateFile(&handle,access,&attributes,&status,nil,win.FILE_ATTRIBUTE_NORMAL,win.FILE_SHARE_READ|win.FILE_SHARE_WRITE|win.FILE_SHARE_DELETE,disposition,options,nil,0)
    if outcome<0 { return nil,.IO,outcome }
    info,error:=windows_attributes(handle)
    if error!=.None || (!allow_link && info.FileAttributes&win.FILE_ATTRIBUTE_REPARSE_POINT!=0) || (!allow_link && (info.FileAttributes&win.FILE_ATTRIBUTE_DIRECTORY!=0)!=directory) {
        win.CloseHandle(handle); return nil,.Not_Regular,outcome
    }
    file,wrap_error:=windows_wrap(handle,name); return file,wrap_error,outcome
}
@(private="package")
open_child_native :: proc(parent:^os.File,name:string,directory:bool)->(^os.File,Error) {
    if parent==nil { return nil,.Invalid_Path }
    if name=="." && directory {
        process:=win.GetCurrentProcess(); handle:win.HANDLE
        if !bool(win.DuplicateHandle(process,windows_handle(parent),process,&handle,0,false,win.DUPLICATE_SAME_ACCESS)) { return nil,.IO }
        return windows_wrap(handle,name)
    }
    file,error,_:=windows_open_child(parent,name,directory); return file,error
}
@(private="package")
open_relative_native :: proc(root:^Root,path:string,directory:=false)->(^os.File,Error) {
    if root.file==nil || !valid_relative_path(path) { return nil,.Invalid_Path }
    remaining:=path; current:=root.file; defer { if current!=root.file { os.close(current) } }
    for part in strings.split_iterator(&remaining,"/") {
        next,error:=open_child_native(current,part,len(remaining)>0 || directory); if error!=.None { return nil,error }
        if len(remaining)==0 { return next,.None }; if current!=root.file { os.close(current) }; current=next
    }
    return nil,.Invalid_Path
}
@(private="package")
windows_parent :: proc(root:^Root,path:string)->(^os.File,string,Error) {
    slash:=strings.last_index_byte(path,'/'); if slash<0 { return root.file,path,.None }
    parent,error:=open_relative_native(root,path[:slash],true); return parent,path[slash+1:],error
}
@(private="package")
windows_dispose :: proc(file:^os.File)->Error {
    remove:win.BOOL=true
    if !bool(win.SetFileInformationByHandle(windows_handle(file),.FileDispositionInfo,&remove,u32(size_of(remove)))) { return .IO }; return .None
}
@(private="package")
Windows_Rename :: struct {replace:u8,root:win.HANDLE,length:u32,name:[1]u16}
@(private="package")
write_atomic_native :: proc(root:^Root,path:string,data:[]byte,exclusive:=false)->(bool,Error) {
    parent,basename,parent_error:=windows_parent(root,path); if parent_error!=.None { return false,parent_error }; defer { if parent!=root.file { os.close(parent) } }
    existing,existing_error,existing_status:=windows_open_child(parent,basename,false)
    if existing_error==.None { os.close(existing); if exclusive { return false,.IO } }
    else if u32(existing_status)!=0xc0000034 && u32(existing_status)!=0xc000003a { return false,existing_error }
    random:[16]byte; crypto.rand_bytes(random[:]); name:[42]byte; copy(name[:],".katla-tmp-"); digits:="0123456789abcdef"
    for number,i in random { name[10+i*2]=digits[number>>4]; name[11+i*2]=digits[number&15] }
    temporary,temporary_error,_:=windows_open_child(parent,string(name[:]),false,WINDOWS_CREATE,write=true,remove=true)
    if temporary_error!=.None { return false,temporary_error }; defer os.close(temporary)
    published:=false; defer { if !published { windows_dispose(temporary) } }
    offset:=0
    for offset<len(data) { count,error:=os.write(temporary,data[offset:]); if error!=nil || count<=0 { return false,.IO }; offset+=int(count) }
    if !bool(win.FlushFileBuffers(windows_handle(temporary))) { return false,.IO }
    encoded:=win.utf8_to_utf16(basename,context.allocator); defer delete(encoded)
    rename_length:=size_of(Windows_Rename)+len(encoded)*2
    buffer:=make([]u64,(rename_length+7)/8); defer delete(buffer)
    rename:=cast(^Windows_Rename)raw_data(buffer); rename.replace=u8(!exclusive); rename.root=windows_handle(parent); rename.length=u32(len(encoded)*2)
    target_data:=cast([^]u16)(uintptr(rename)+uintptr(offset_of(Windows_Rename,name))); copy(target_data[:len(encoded)],encoded)
    status:win.IO_STATUS_BLOCK
    if outcome:=NtSetInformationFile(windows_handle(temporary),&status,rename,u32(rename_length),.FileRenameInformation); outcome<0 { return false,.IO }
    published=true; return true,.None
}
@(private="package")
make_parents_native :: proc(root:^Root,path:string)->Error {
    slash:=strings.last_index_byte(path,'/'); if slash<0 { return .None }; current:=root.file; remaining:=path[:slash]
    defer { if current!=root.file { os.close(current) } }
    for part in strings.split_iterator(&remaining,"/") {
        next,error,_:=windows_open_child(current,part,true,WINDOWS_OPEN_IF); if error!=.None { return error }
        if current!=root.file { os.close(current) }; current=next
    }; return .None
}
@(private="package")
create_directory_native :: proc(root:^Root,path:string)->Error {
    parent,basename,parent_error:=windows_parent(root,path); if parent_error!=.None { return parent_error }; defer { if parent!=root.file { os.close(parent) } }
    file,error,_:=windows_open_child(parent,basename,true,WINDOWS_CREATE); if error==.None { os.close(file) }; return error
}
@(private="package")
remove_path_native :: proc(root:^Root,path:string,recursive:bool)->Error {
    parent,basename,parent_error:=windows_parent(root,path); if parent_error!=.None { return parent_error }; defer { if parent!=root.file { os.close(parent) } }
    budget:=MAX_ENTRIES; return windows_remove_child(parent,basename,recursive,false,&budget,0)
}
@(private="package")
windows_remove_child :: proc(parent:^os.File,name:string,recursive,allow_link:bool,budget:^int,depth:int,count:=true)->Error {
    if depth>256 { return .Limit }; if count { if budget^==0 { return .Limit }; budget^-=1 }
    file,error,_:=windows_open_child(parent,name,false,remove=true,allow_link=true); if error!=.None { return error }; defer os.close(file)
    info,info_error:=windows_attributes(windows_handle(file)); if info_error!=.None { return info_error }
    link:=info.FileAttributes&win.FILE_ATTRIBUTE_REPARSE_POINT!=0; directory:=info.FileAttributes&win.FILE_ATTRIBUTE_DIRECTORY!=0
    if link && !allow_link { return .Not_Regular }
    if directory && !link {
        if !recursive { return .Not_Regular }
        inventory,inventory_error:=directory_entries_native(file,budget); if inventory_error!=.None { return inventory_error }; defer native_directory_destroy(&inventory)
        for entry in inventory.entries { if child_error:=windows_remove_child(file,entry.name,true,true,budget,depth+1,false); child_error!=.None { return child_error } }
    }
    return windows_dispose(file)
}
@(private="package")
Windows_Directory_Entry :: struct {next,index:u32,creation,access,write,change,size,allocation:i64,attributes,name_length:u32,name:[1]u16}
@(private="package")
directory_entries_native :: proc(file:^os.File,budget:^int=nil)->(Native_Directory,Error) {
    local_budget:=MAX_ENTRIES; remaining_budget:=budget; if remaining_budget==nil { remaining_budget=&local_budget }
    result:=Native_Directory{entries=make([dynamic]Native_Entry,context.allocator),allocator=context.allocator}; accepted:=false; defer { if !accepted { native_directory_destroy(&result) } }
    buffer:[4096]u64; restart:=true
    for {
        flags:u32=win.SL_RETURN_SINGLE_ENTRY; if restart { flags|=win.SL_RESTART_SCAN; restart=false }
        status:win.IO_STATUS_BLOCK
        outcome:=win.NtQueryDirectoryFileEx(windows_handle(file),nil,nil,nil,&status,&buffer,u32(size_of(buffer)),.FileDirectoryInformation,flags,nil)
        if u32(outcome)==0x80000006 || u32(outcome)==0xc000000f { break }; if outcome<0 { return {},.IO }
        header:=cast(^Windows_Directory_Entry)&buffer
        if status.Information>uint(size_of(buffer)) || status.Information<uint(offset_of(Windows_Directory_Entry,name)) || header.name_length&1!=0 || uint(header.name_length)>status.Information-uint(offset_of(Windows_Directory_Entry,name)) { return {},.IO }
        unit_data:=cast([^]u16)(uintptr(header)+uintptr(offset_of(Windows_Directory_Entry,name))); units:=unit_data[:int(header.name_length/2)]
        if remaining_budget^==0 { return {},.Limit }; remaining_budget^-=1
        if !windows_utf16_valid(units) { continue }
        name_bytes:=make([]byte,len(units)*3,context.allocator); length:=utf16.decode_to_utf8(name_bytes,units); name:=string(name_bytes[:length]); defer delete(name_bytes)
        if name=="." || name==".." { continue }; if !valid_relative_path(name) || strings.contains(name,"/") { continue }
        link:=header.attributes&win.FILE_ATTRIBUTE_REPARSE_POINT!=0; directory:=header.attributes&win.FILE_ATTRIBUTE_DIRECTORY!=0
        append(&result.entries,Native_Entry{strings.clone(name),directory,!directory && !link,link,header.size})
    }
    accepted=true; return result,.None
}

@(private="package")
windows_utf16_valid :: proc(units:[]u16)->bool {
    for i:=0;i<len(units);i+=1 {
        unit:=units[i]; if unit==0 { return false }
        if unit>=0xd800 && unit<0xdc00 { if i+1>=len(units) || units[i+1]<0xdc00 || units[i+1]>=0xe000 { return false }; i+=1 }
        else if unit>=0xdc00 && unit<0xe000 { return false }
    }; return true
}
#assert(offset_of(Windows_Rename,name)==size_of(uintptr)*2+4)
#assert(offset_of(Windows_Directory_Entry,name)==64)
