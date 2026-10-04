#+test
#+build darwin, linux
package editor_app

import app ".."
import ecs "../../ecs"
import script "../../script"
import "core:testing"
import "core:os"
import "core:strings"

CODE_LUAU_LIBRARY :: #config(LUAU_LIBRARY,"")
@(test)
test_code_tabs_actual_file_buffers_and_dirty_switch_close_decisions :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-code-tabs-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); scripts:=strings.concatenate({resource,"/scripts"}); defer delete(scripts)
    testing.expect(t,os.make_directory(resource)==nil && os.make_directory(scripts)==nil)
    first:=strings.concatenate({scripts,"/first.luau"}); defer delete(first); second:=strings.concatenate({scripts,"/second.luau"}); defer delete(second)
    testing.expect(t,os.write_entire_file(first,"speed=1")==nil && os.write_entire_file(second,"speed=2")==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect(t,app.authoring_services_init(&owner)==.None && app.asset_resources_init(&owner,directory,resource)==.None)
    documents:Code_Documents; code_documents_init(&documents,&owner); defer code_documents_destroy(&documents)
    testing.expect(t,code_document_open(&documents,.Resource,"first.lua")==.None && documents.has_active && documents.tabs[0].path=="scripts/first.luau")
    testing.expect(t,code_document_edit(&documents,"speed=4")==.None && code_document_dirty(documents.tabs[0]))
    testing.expect(t,code_document_open(&documents,.Resource,"second")==.None && documents.dialog==.Unsaved && documents.active==0 && len(documents.tabs)==2)
    testing.expect(t,code_document_respond(&documents,.Cancel)==.None && documents.tabs[0].text=="speed=4" && documents.active==0)
    testing.expect(t,code_document_select(&documents,1)==.None && code_document_respond(&documents,.Discard)==.None && documents.active==1 && documents.tabs[0].text=="speed=1")
    testing.expect(t,code_document_edit(&documents,"speed=5")==.None && code_document_close(&documents,1)==.None && documents.dialog==.Unsaved)
    testing.expect(t,code_document_respond(&documents,.Save)==.Application_Owned && documents.dialog==.Unsaved && len(documents.tabs)==2 && code_document_dirty(documents.tabs[1]))
    testing.expect(t,code_document_respond(&documents,.Discard)==.None && len(documents.tabs)==1 && documents.active==0)
    invalid:string="\xff"; testing.expect(t,code_document_edit(&documents,invalid)==.Invalid_Field_Value)
    testing.expect(t,code_document_open(&documents,.Resource,"../outside")==.Invalid_Field_Value)
    testing.expect(t,code_document_edit(&documents,"speed=9")==.None)
    ready,leave_error:=code_documents_request_leave(&documents); testing.expect(t,!ready && leave_error==.None && documents.dialog==.Unsaved && documents.leave_requested)
    testing.expect(t,code_document_respond(&documents,.Cancel)==.None && !documents.leave_requested && !documents.leave_ready && documents.tabs[0].text=="speed=9")
    ready,leave_error=code_documents_request_leave(&documents); testing.expect(t,!ready && leave_error==.None)
    testing.expect(t,code_document_respond(&documents,.Discard)==.None && documents.leave_ready && len(documents.tabs)==1 && documents.tabs[0].text=="speed=1")
}

@(private="file")
native_code_document_save :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-code-save-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    external:=strings.concatenate({directory,"/selected.luau"}); defer delete(external); testing.expect(t,os.write_entire_file(external,"speed=2")==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect(t,app.authoring_services_init(&owner)==.None && app.asset_resources_init(&owner,directory,resource)==.None && app.script_native_init(&owner,CODE_LUAU_LIBRARY)==.None)
    documents:Code_Documents; code_documents_init(&documents,&owner); defer code_documents_destroy(&documents)
    testing.expect(t,code_document_open(&documents,.File,external)==.None)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,app.Script_Component{path=strings.clone(external),root=.File}); testing.expect(t,app.script_native_sync(&owner)==.None)
    runtime:=ecs.get_resource_mut(&owner.world,app.Script_Native_Runtime); old,had_old:=script.handle(runtime.runtime,u64(entity)); testing.expect(t,had_old)
    testing.expect(t,code_document_edit(&documents,"function broken(")==.None)
    rejected:=code_document_save(&documents); testing.expect(t,!rejected.published && rejected.error==.Invalid_Operation && documents.message!="" && code_document_dirty(documents.tabs[0]))
    bytes,read_error:=os.read_entire_file(external,context.allocator); testing.expect(t,read_error==nil && string(bytes)=="speed=2"); delete(bytes)
    retained,has_retained:=script.handle(runtime.runtime,u64(entity)); testing.expect(t,has_retained && retained==old)
    testing.expect(t,code_document_edit(&documents,"speed=3\nnew_value=true")==.None)
    saved:=code_document_save(&documents); testing.expect(t,saved.published && saved.reloaded && saved.reload_count==1 && saved.error==.None && !code_document_dirty(documents.tabs[0]) && documents.message=="")
    accepted,accepted_error:=os.read_entire_file(external,context.allocator); testing.expect(t,accepted_error==nil && string(accepted)==documents.tabs[0].text); delete(accepted)
    testing.expect(t,code_document_close(&documents,0)==.None && !documents.has_active)
}
when CODE_LUAU_LIBRARY!="" {
    @(test)
    test_code_document_native_syntax_preflight_atomic_save_and_attached_reload :: proc(t:^testing.T) { native_code_document_save(t) }
}
