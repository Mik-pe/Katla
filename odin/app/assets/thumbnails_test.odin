#+test
#+build darwin, linux
//! Freshness and root identity preserve real navigation admission while isolating borrowed image views.
package asset_browser
import app ".."
import ecs "../../ecs"
import resources "../../resources"
import "core:testing"
import "core:os"
import "core:strings"
import "core:mem"
import "core:math"

@(private="file")
thumbnail_source_fixture :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-thumbnail-source-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for folder in ([2]string{directory,resource}) { file:=strings.concatenate({folder,"/image.png"}); testing.expect(t,os.write_entire_file(file,"first source revision")==nil); delete(file) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect(t,app.asset_resources_init(&owner,directory,resource)==.None)
    state:State; init(&state,&owner); defer destroy(&state); testing.expect(t,refresh(&state)==.None)
    source,source_error:=thumbnail_source(&state); testing.expect(t,source_error==.None && source.identity!=0 && source.revision>0)
    index,found:=entry_index(&state,"image.png"); testing.expect(t,found); if !found { return }
    state.entries[index].thumbnail=.Ready; state.entries[index].thumbnail_texture=42; state.entries[index].thumbnail_width=10; state.entries[index].thumbnail_height=8; state.entries[index].thumbnail_revision=source.revision
    next,next_error:=thumbnail_source(&state,.99); testing.expect(t,next_error==.None && next.identity==source.identity && next.revision==source.revision)
    next,next_error=thumbnail_source(&state,.01); testing.expect(t,next_error==.None && next.revision>source.revision && state.entries[index].thumbnail_texture==42)
    stalled,_:=thumbnail_source(&state,2,false); testing.expect(t,stalled.revision==next.revision)
    stalled,_=thumbnail_source(&state,2,false); testing.expect(t,stalled.revision==next.revision)
    resumed,_:=thumbnail_source(&state,0,true); testing.expect(t,resumed.revision>stalled.revision)
    testing.expect(t,refresh(&state)==.None); refreshed,_:=thumbnail_source(&state); testing.expect(t,refreshed.revision>next.revision && refreshed.identity==next.identity && state.entries[index].thumbnail_texture==42)
    testing.expect(t,navigate(&state,"missing")!=.None); failed,_:=thumbnail_source(&state); testing.expect(t,failed.identity==refreshed.identity && failed.revision==refreshed.revision && state.entries[index].thumbnail_texture==42)
    testing.expect(t,navigate_root(&state,.Project,"")==.None); project,_:=thumbnail_source(&state); testing.expect(t,project.identity!=refreshed.identity)
    project_index,has_image:=entry_index(&state,"image.png"); testing.expect(t,has_image && state.entries[project_index].thumbnail_texture==0 && state.entries[project_index].thumbnail==.Pending)
    testing.expect(t,back(&state)==.None); returned,_:=thumbnail_source(&state); testing.expect(t,returned.identity==refreshed.identity)
    _,invalid:=thumbnail_source(&state,math.nan_f64()); testing.expect(t,invalid!=.None)
    roots:=ecs.get_resource_mut(&owner.world,app.Asset_Roots); resources.root_destroy(&roots.resource); replacement,replacement_error:=resources.root_open(directory,owner.world.allocator); testing.expect(t,replacement_error==.None); roots.resource=replacement
    replaced,_:=thumbnail_source(&state); testing.expect(t,replaced.identity!=returned.identity)
    second:State; init(&second,&owner); defer destroy(&second); other,_:=thumbnail_source(&second); testing.expect(t,other.identity!=replaced.identity)
}
@(test)
test_thumbnail_source_refresh_clock_failed_navigation_and_capability_generations :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    thumbnail_source_fixture(t); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
