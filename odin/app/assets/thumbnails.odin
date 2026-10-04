//! Browser freshness identifies retained capabilities independently of GPU image ownership.
package asset_browser
import resources "../../resources"
import "core:os"
import "core:sync"
import "core:math"

@(private="package")
Thumbnail_Root :: struct { identity,device:u64,inode:u128,file:^os.File }
@(private="package")
thumbnail_next_identity:u64=1
/// A borrowed source belongs to the application root; consumers must finish before root destruction.
Thumbnail_Source :: struct { root:^resources.Root,identity,revision:u64 }
@(private="package")
thumbnail_root_identity :: proc(state:^State,root:^resources.Root)->(u64,resources.Error) {
    if root==nil || root.file==nil || state.root>.Project { return 0,.Invalid_Path }
    info,error:=os.fstat(root.file,state.allocator); if error!=nil { return 0,.IO }; defer os.file_info_delete(info,state.allocator)
    if info.inode==0 { return 0,.IO }
    slot:=&state.thumbnail_roots[int(state.root)]
    if slot.identity==0 || slot.file!=root.file || slot.device!=info.device || slot.inode!=info.inode {
        identity:=sync.atomic_add(&thumbnail_next_identity,u64(1)); if identity==0 || identity==max(u64) { return 0,.Limit }
        slot^={identity=identity,file=root.file,device=info.device,inode=info.inode}
    }
    return slot.identity,.None
}
@(private="package")
thumbnail_freshen :: proc(state:^State) { if state.thumbnail_revision<max(u64) { state.thumbnail_revision+=1 }; state.thumbnail_elapsed=0 }
/// Polls visible source bytes at most once per second; accepted browser refreshes request a new revision immediately.
thumbnail_source :: proc(state:^State,dt:f64=0,periodic_idle:bool=true)->(Thumbnail_Source,resources.Error) {
    if state==nil || state.owner==nil || math.is_nan(dt) || math.is_inf(dt) || dt<0 { return {},.Invalid_Path }
    root:=root_for(state); identity,error:=thumbnail_root_identity(state,root); if error!=.None { return {},error }
    if identity!=state.thumbnail_inventory_identity {
        for &entry in state.entries { entry.thumbnail=.Pending; entry.thumbnail_texture=0; entry.thumbnail_width=0; entry.thumbnail_height=0; entry.thumbnail_revision=0; entry.thumbnail_error=false }
        state.thumbnail_inventory_identity=identity; thumbnail_freshen(state)
    }
    state.thumbnail_elapsed+=dt
    if periodic_idle && state.thumbnail_elapsed>=1 { thumbnail_freshen(state) }
    return {root,identity,state.thumbnail_revision},.None
}
