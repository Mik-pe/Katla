#+build darwin
//! Each native owner frame bounds autoreleased platform objects independently of application policy.
package window
import NS "core:sys/darwin/Foundation"
import gfx "../../gfx"

frame_begin :: proc()->rawptr { return NS.AutoreleasePool.alloc()->init() }
frame_end :: proc(pool:rawptr) { value:=cast(^NS.AutoreleasePool)pool; value->drain() }
window_surface :: proc(owner:^Window)->gfx.Surface_Desc { state:=window_state(owner); return {view=window_view(owner),width=state.width,height=state.height} }
