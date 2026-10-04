//! Only canonical known panels are admitted before dock layout publication.
package editor_app
import prefs "../preferences"
import ui "../../ui"

@(private="package")
dock_layout_valid :: proc(state:rawptr,bytes:[]byte)->bool {
    shell:=cast(^Shell)state
    tree:ui.Dock_Tree; if ui.dock_init(&tree,{ui.Tab_Id(Panel.Viewport)},shell.allocator)!=.None { return false }; defer ui.dock_destroy(&tree)
    if ui.dock_restore(&tree,string(bytes))!=.None { return false }
    for _,node in tree.nodes { for tab in node.tabs { if u64(tab)<1 || u64(tab)>u64(len(shell.tabs)) { return false } } }; return true
}
/// A malformed or obsolete layout leaves the initialized desktop dock unchanged.
shell_dock_load :: proc(shell:^Shell)->bool {
    if shell.preference_store==nil { return false }
    bytes,error:=prefs.dock_load(shell.preference_store); defer delete(bytes,shell.allocator)
    if error!=.None || !dock_layout_valid(shell,bytes) { return false }
    return ui.dock_restore(&shell.dock,string(bytes))==.None
}
/// Exact split ratios, active tabs and closed panels survive application restarts.
shell_dock_save :: proc(shell:^Shell)->bool {
    if shell.preference_store==nil { return false }
    encoded:=ui.dock_snapshot(&shell.dock,allocator=shell.allocator); defer delete(encoded,shell.allocator)
    published,error:=prefs.dock_save(shell.preference_store,transmute([]byte)encoded,shell,dock_layout_valid)
    return published && error==.None
}
