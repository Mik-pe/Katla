#+test
//! Opt-in native file-manager acceptance retains the selected fixture for desktop observation.
package asset_browser
import app ".."
import resources "../../resources"
import "core:testing"
import "core:os"
import "core:strings"

ASSET_REVEAL_PROOF_DIRECTORY :: #config(ASSET_REVEAL_PROOF_DIRECTORY, "")
@(private="file")
reveal_native_fixture :: proc(t:^testing.T) {
    testing.expect(t,ASSET_REVEAL_PROOF_DIRECTORY!=""); if ASSET_REVEAL_PROOF_DIRECTORY=="" { return }
    resource:=strings.concatenate({ASSET_REVEAL_PROOF_DIRECTORY,"/resources"}); defer delete(resource); if !os.exists(resource) { testing.expect(t,os.make_directory(resource)==nil) }
    name:=string(`Quoted asset "selected" ; $(literal).txt`); when ODIN_OS==.Windows { name=`Quoted asset selected ; $(literal).txt` }
    filename:=strings.concatenate({resource,"/",name}); defer delete(filename)
    testing.expect(t,os.write_entire_file(filename,"Native Reveal keeps this exact confined filename selected.")==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    testing.expect_value(t,app.asset_resources_init(&owner,ASSET_REVEAL_PROOF_DIRECTORY,resource),resources.Error.None)
    state:State; init(&state,&owner); defer destroy(&state); testing.expect_value(t,refresh(&state),resources.Error.None)
    testing.expect(t,select(&state,name)); testing.expect_value(t,reveal(&state).error,Reveal_Error.None)
}
when ASSET_REVEAL_PROOF_DIRECTORY!="" {
@(test)
test_browser_native_os_reveal_selected_confined_literal_filename :: proc(t:^testing.T) { reveal_native_fixture(t) }
}
