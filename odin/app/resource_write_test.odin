#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_resource_mutations_real_templates_exclusive_creation_and_confined_parent_links :: proc(t:^testing.T) {
    directory,err:=os.make_directory_temp("","katla-resource-writes-*",context.allocator); testing.expect(t,err==nil); if err!=nil { return }
    defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect(t,os.make_directory(resource_path)==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_path),resources.Error.None)
    authoring_services_init(&owner)
    perform :: proc(owner:^Authoring,tool,data:string)->editor.Tool_Result {
        result,undo:=authoring_application(owner,{tool_name=tool,value=transmute([]byte)data}); editor.undo_group_destroy(&undo); return result
    }
    created:=perform(&owner,"create_resource",`{"path":"resources/new/deep/material.json","template":"material","content":"ignored"}`); defer editor.tool_result_destroy(&created)
    testing.expect_value(t,created.error,editor.Scene_Error.None)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots)
    bytes,read_error:=resources.read_text(&roots.project,"resources/new/deep/material.json"); testing.expect_value(t,read_error,resources.Error.None)
    testing.expect(t,strings.contains(string(bytes),`"shader":"pbr"`)); delete(bytes)
    duplicate:=perform(&owner,"create_resource",`{"path":"resources/new/deep/material.json","content":"must not overwrite"}`); defer editor.tool_result_destroy(&duplicate)
    testing.expect_value(t,duplicate.error,editor.Scene_Error.Invalid_Operation)
    changed:=perform(&owner,"write_resource",`{"path":"resources/new/deep/material.json","content":"new UTF-8 åäö"}`); defer editor.tool_result_destroy(&changed)
    testing.expect_value(t,changed.error,editor.Scene_Error.None)
    bytes,read_error=resources.read_text(&roots.project,"resources/new/deep/material.json"); testing.expect_value(t,read_error,resources.Error.None); testing.expect_value(t,string(bytes),"new UTF-8 åäö"); delete(bytes)
    missing:=perform(&owner,"write_resource",`{"path":"resources/missing.txt","content":"not created"}`); defer editor.tool_result_destroy(&missing); testing.expect_value(t,missing.error,editor.Scene_Error.Invalid_Operation)
    outside,outside_error:=os.make_directory_temp("","katla-resource-outside-*",context.allocator); testing.expect(t,outside_error==nil); if outside_error!=nil { return }; defer { os.remove_all(outside); delete(outside) }
    parent_link:=strings.concatenate({directory,"/resources/link"}); defer delete(parent_link); testing.expect(t,os.symlink(outside,parent_link)==nil)
    linked:=perform(&owner,"create_resource",`{"path":"resources/link/escaped.txt","content":"rejected"}`); defer editor.tool_result_destroy(&linked); testing.expect_value(t,linked.error,editor.Scene_Error.Invalid_Operation)
    target:=strings.concatenate({outside,"/target.txt"}); defer delete(target); testing.expect(t,os.write_entire_file(target,"outside preserved")==nil)
    file_link:=strings.concatenate({directory,"/resources/file.txt"}); defer delete(file_link); testing.expect(t,os.symlink(target,file_link)==nil)
    for tool in ([2]string{"create_resource","write_resource"}) {
        rejected:=perform(&owner,tool,`{"path":"resources/file.txt","content":"rejected"}`); defer editor.tool_result_destroy(&rejected); testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation)
    }
    untouched,file_error:=os.read_entire_file(target,context.allocator); testing.expect(t,file_error==nil); testing.expect_value(t,string(untouched),"outside preserved"); delete(untouched)
    invalid:=perform(&owner,"create_resource",`{"path":"../escaped.txt","content":"rejected"}`); defer editor.tool_result_destroy(&invalid); testing.expect_value(t,invalid.error,editor.Scene_Error.Invalid_Operation)
    owner.mode=.Playing
    playing:=perform(&owner,"create_resource",`{"path":"resources/playing.txt","content":"rejected"}`); defer editor.tool_result_destroy(&playing); testing.expect_value(t,playing.error,editor.Scene_Error.Editing_Required)
}

@(test)
test_resource_write_envelopes_optional_null_and_required_content :: proc(t:^testing.T) {
    wire:string=`{"path":"notes.txt","template":null,"content":null}`
    accepted,error:=asset.resource_write_decode("create_resource",transmute([]byte)wire); defer asset.resource_write_destroy(&accepted)
    testing.expect_value(t,error,asset.Error.None); testing.expect(t,!accepted.request.has_template && accepted.request.content=="")
    for invalid_wire in ([4]string{`{"path":"notes.txt"}`,`{"path":"notes.txt","content":null}`,`{"path":"notes.txt","content":3}`,`{"path":"notes.txt","content":"x","template":"scene"}`}) {
        rejected,decode_error:=asset.resource_write_decode("write_resource",transmute([]byte)invalid_wire); defer asset.resource_write_destroy(&rejected); testing.expect_value(t,decode_error,asset.Error.Invalid_Arguments)
    }
}
