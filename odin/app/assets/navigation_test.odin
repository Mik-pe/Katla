#+test
#+build darwin, linux
//! Real directory moves, stale history, confined reveal admission and exact argv own their lifetimes.
package asset_browser
import app ".."
import resources "../../resources"
import "core:testing"
import "core:os"
import "core:strings"
import "core:mem"
import "core:time"

@(private="file")
navigation_fixture :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-browser-history-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for folder in ([3]string{"a","a/deep","b"}) { path:=strings.concatenate({resource,"/",folder}); testing.expect(t,os.make_directory(path)==nil); delete(path) }
    selected_name:string=`a/deep/quoted "file" ; $(touch NEVER).txt`; selected_file:=strings.concatenate({resource,"/",selected_name}); defer delete(selected_file); testing.expect(t,os.write_entire_file(selected_file,"real confined asset")==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.asset_resources_init(&owner,directory,resource),resources.Error.None)
    state:State; init(&state,&owner); defer destroy(&state); testing.expect_value(t,refresh(&state),resources.Error.None)
    testing.expect_value(t,navigate(&state,"a"),resources.Error.None); testing.expect_value(t,navigate(&state,"a/deep"),resources.Error.None)
    testing.expect(t,can_back(&state) && !can_forward(&state) && len(state.back_history)==2 && state.entries[0].path==selected_name)
    testing.expect(t,select(&state,selected_name))
    file_command,file_admission:=reveal_command(&state); testing.expect_value(t,file_admission.error,Reveal_Error.None)
    when ODIN_OS==.Darwin { testing.expect(t,strings.has_suffix(file_command.arguments[2],selected_name) && file_command.length==3) }
    else { testing.expect(t,strings.has_suffix(file_command.arguments[1],"/a/deep") && file_command.length==2) }
    reveal_command_destroy(&file_command); before_revision:=state.revision
    testing.expect_value(t,navigate(&state,"missing"),resources.Error.IO); testing.expect(t,state.directory=="a/deep" && len(state.back_history)==2 && !can_forward(&state) && state.selected==selected_name && state.revision==before_revision)
    testing.expect_value(t,breadcrumb(&state,1),resources.Error.None); testing.expect(t,state.directory=="a" && len(state.back_history)==3)
    testing.expect_value(t,back(&state),resources.Error.None); testing.expect(t,state.directory=="a/deep" && can_forward(&state))
    testing.expect_value(t,forward(&state),resources.Error.None); testing.expect_value(t,parent(&state),resources.Error.None); testing.expect(t,state.directory=="" && !can_forward(&state))
    testing.expect_value(t,navigate_root(&state,.Project,"resources/a/deep"),resources.Error.None); testing.expect(t,state.root==.Project)
    testing.expect_value(t,back(&state),resources.Error.None); testing.expect(t,state.root==.Resource && state.directory=="")
    testing.expect_value(t,forward(&state),resources.Error.None); testing.expect(t,state.root==.Project && state.directory=="resources/a/deep")
    testing.expect_value(t,navigate_root(&state,.Resource,"b"),resources.Error.None); testing.expect(t,!can_forward(&state))
    deep:=strings.concatenate({resource,"/a/deep"}); defer delete(deep); testing.expect(t,os.remove_all(deep)==nil)
    saved_back:=len(state.back_history); saved_forward:=len(state.forward_history); saved_revision:=state.revision
    testing.expect_value(t,back(&state),resources.Error.IO); testing.expect(t,state.root==.Resource && state.directory=="b" && len(state.back_history)==saved_back && len(state.forward_history)==saved_forward && state.revision==saved_revision)
    testing.expect_value(t,navigate(&state,"../escape"),resources.Error.Invalid_Path); testing.expect_value(t,breadcrumb(&state,100),resources.Error.Invalid_Path)
    for i in 0..<MAX_NAVIGATION_HISTORY+10 { testing.expect_value(t,navigate(&state,"a" if i&1==0 else "b"),resources.Error.None) }
    testing.expect_value(t,len(state.back_history),MAX_NAVIGATION_HISTORY)
    testing.expect_value(t,navigate(&state,""),resources.Error.None); testing.expect(t,select(&state,"a"))
    command,admission:=reveal_command(&state); testing.expect_value(t,admission.error,Reveal_Error.None)
    when ODIN_OS==.Darwin { testing.expect(t,command.length==3 && command.arguments[0]=="/usr/bin/open" && command.arguments[1]=="-R" && strings.has_suffix(command.arguments[2],"/resources/a")) }
    else { testing.expect(t,command.length==2 && command.arguments[0]=="/usr/bin/xdg-open" && command.arguments[1]==resource) }
    reveal_command_destroy(&command)
    a:=strings.concatenate({resource,"/a"}); defer delete(a); testing.expect(t,os.remove_all(a)==nil); testing.expect(t,os.symlink(directory,a)==nil)
    _,rejected:=reveal_command(&state); testing.expect(t,rejected.error==.Filesystem)
    delete(state.selected,state.allocator); state.selected=strings.clone("../escape",state.allocator); _,escaped:=reveal_command(&state); testing.expect(t,escaped.error==.No_Selection); delete(state.selected,state.allocator); state.selected=strings.clone("a",state.allocator)
    testing.expect(t,select(&state,"b"))
    retired_root:=strings.concatenate({directory,"/retired-resources"}); defer delete(retired_root); testing.expect(t,os.rename(resource,retired_root)==nil); testing.expect(t,os.make_directory(resource)==nil)
    replacement:=strings.concatenate({resource,"/b"}); defer delete(replacement); testing.expect(t,os.make_directory(replacement)==nil)
    _,retired:=reveal_command(&state); testing.expect(t,retired.error==.Filesystem && retired.filesystem_error==.Invalid_Path)
}
@(test)
test_browser_history_roots_breadcrumbs_failed_admission_bounded_ownership_and_reveal :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    navigation_fixture(t); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_browser_detached_native_launch_exact_argv_and_exec_error :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-reveal-argv-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    marker:=strings.concatenate({directory,`/quoted "file" ; $(touch NEVER).txt`}); defer delete(marker)
    testing.expect_value(t,reveal_launch({"/usr/bin/touch",marker},context.allocator),0)
    exists:=false
    for _ in 0..<50 { if os.exists(marker) { exists=true; break }; time.sleep(10*time.Millisecond) }
    testing.expect(t,exists)
    testing.expect(t,reveal_launch({"/missing/katla-file-manager",marker},context.allocator)!=0)
}
