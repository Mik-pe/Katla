//! One stationary UI owner captures font/layout services and all retained allocations.
package ui
import "core:mem"

/// Dark editor tokens are display colors; renderers preserve the declared UI color convention.
theme_default :: proc(font:Font_Id=1)->Theme {
    return {canvas={0.10,0.10,0.11,1},panel={0.145,0.145,0.155,1},control={0.19,0.19,0.205,1},hover={0.24,0.24,0.255,1},active={0.29,0.29,0.305,1},text={0.94,0.94,0.96,1},muted={0.66,0.66,0.69,1},disabled={0.43,0.43,0.45,1},accent={0.97,0.58,0.27,1},selection={0.35,0.78,0.98,1},font=font,font_size=12,row_height=30,padding=8,radius=6}
}
/// Font services must describe real shaped text; a missing provider never invents glyph metrics.
context_init :: proc(ctx:^Context,fonts:Font_Provider,theme:Theme={},allocator:mem.Allocator=context.allocator)->Frame_Error {
    if ctx==nil || ctx.initialized { return .Invalid_State }
    if fonts.measure==nil || fonts.caret==nil || fonts.hit_test==nil || fonts.navigate==nil || fonts.grapheme==nil { return .Font_Unavailable }
    actual_theme:=theme; if actual_theme.font_size<=0 { actual_theme=theme_default() }
    ctx^={fonts=fonts,theme=actual_theme,allocator=allocator,window_focused=true,initialized=true}
    ctx.nodes=make(map[u64]^Node,allocator); ctx.actions=make([dynamic]Action,allocator); ctx.action_snapshots=make([dynamic]string,allocator)
    ctx.commands=make([dynamic]Draw_Command,allocator); ctx.order=make([dynamic]Node_Id,allocator)
    return .None
}
@(private="package")
draw_commands_clear :: proc(ctx:^Context) {
    for command in ctx.commands {
        switch item in command {
        case Text_Draw: delete(item.text,ctx.allocator); delete(item.runs,ctx.allocator)
        case Mesh_Draw: delete(item.vertices,ctx.allocator); delete(item.indices,ctx.allocator)
        case Rect_Draw,Image_Draw:
        }
    }
    clear(&ctx.commands)
}
@(private="package")
node_destroy :: proc(ctx:^Context,node:^Node) {
    descriptor_destroy(&node.descriptor,ctx.allocator)
    for _,cell in node.state { value_destroy(cell.value,ctx.allocator) }
    text_history_clear(ctx,&node.undo); text_history_clear(ctx,&node.redo); delete(node.undo); delete(node.redo)
    delete(node.field_text,ctx.allocator)
    delete(node.preedit,ctx.allocator); delete(node.state); delete(node.children); free(node,ctx.allocator)
}
/// Call after the native renderer has frozen or consumed the last borrowed draw list.
context_destroy :: proc(ctx:^Context) {
    draw_commands_clear(ctx)
    for _,node in ctx.nodes { node_destroy(ctx,node) }
    delete(ctx.clipboard,ctx.allocator); delete(ctx.nodes)
    actions_clear(ctx); delete(ctx.action_snapshots)
    delete(ctx.actions); delete(ctx.commands); delete(ctx.order); ctx^={closed=true}
}
