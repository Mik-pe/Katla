#+test
#+build windows
package app

import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_windows_installed_asset_identity_preserves_root_and_reads_exact_relative_script :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-win-identity-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); scripts:=strings.concatenate({resource,"/scripts"}); defer delete(scripts)
    testing.expect(t,os.make_directory(resource)==nil && os.make_directory(scripts)==nil)
    file:=strings.concatenate({scripts,"/Öak.luau"}); defer delete(file); testing.expect(t,os.write_entire_file(file,"speed=7")==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,asset_resources_init(&owner,directory,resource)==.None)
    bytes:=make([]byte,len(file)); defer delete(bytes); copy(bytes,file)
    for &character in bytes { if character=='/' { character='\\' } }
    if len(bytes)>1 && bytes[1]==':' { if bytes[0]>='A' && bytes[0]<='Z' { bytes[0]+=32 } else if bytes[0]>='a' && bytes[0]<='z' { bytes[0]-=32 } }
    identity,kind,valid:=asset_identify_path(&owner,string(bytes)); defer delete(identity); testing.expect(t,valid && kind==.Resource && identity=="scripts/Öak.luau")
    script,error_source:=script_source_resolve(&owner,string(bytes)); defer delete(script.path); testing.expect(t,error_source==.None && script.root==.Resource && script.path=="scripts/Öak.luau")
    source,source_error:=script_source_read(&owner,script); defer delete(source); testing.expect(t,source_error==.None && string(source)=="speed=7")
    outside:=strings.concatenate({directory,"Backup/logic.luau"}); defer delete(outside)
    outside_identity,outside_kind,outside_valid:=asset_identify_path(&owner,outside); defer delete(outside_identity); testing.expect(t,outside_valid && outside_kind==.File && outside_identity==outside)
}
