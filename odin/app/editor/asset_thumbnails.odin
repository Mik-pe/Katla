//! Visible retained rows request bounded thumbnail work before UI frame preparation.
package editor_app
import assets "../assets"
import render "../render"

/// Borrows genuine mounted image sources; the caller owns the request array.
shell_asset_thumbnail_requests :: proc(shell:^Shell,dt:f64=0)->[]render.Thumbnail_Request {
    requests:=make([dynamic]render.Thumbnail_Request,shell.allocator); defer delete(requests)
    if shell.browser==nil || shell.ctx==nil { return nil }
    periodic_idle:=true
    for entry in shell.browser.entries {
        if entry.kind!=.Image || entry.thumbnail!=.Loading { continue }
        node,present:=shell.ctx.nodes[key(51,entry.path,u64(shell.browser.root))]
        if present && node.mounted && node.seen==shell.ctx.frame_index && node.clip.width>0 && node.clip.height>0 { periodic_idle=false; break }
    }
    source,error:=assets.thumbnail_source(shell.browser,dt,periodic_idle); if error!=.None { return nil }
    for entry in shell.browser.entries {
        if entry.kind!=.Image { continue }
        node,present:=shell.ctx.nodes[key(51,entry.path,u64(shell.browser.root))]
        if !present || !node.mounted || node.seen!=shell.ctx.frame_index || node.clip.width<=0 || node.clip.height<=0 { continue }
        append(&requests,render.Thumbnail_Request{source.root,source.identity,entry.path,source.revision})
    }
    owned:=make([]render.Thumbnail_Request,len(requests),shell.allocator); copy(owned,requests[:]); return owned
}
/// Publishes only actual cache receipts; the render cache owns source jobs, native images and registration.
shell_asset_thumbnails_update :: proc(shell:^Shell,cache:^render.Thumbnail_Cache($R),dt:f64=0)->render.Thumbnail_Receipt {
    requests:=shell_asset_thumbnail_requests(shell,dt); defer delete(requests,shell.allocator)
    receipt:=render.thumbnail_cache_update(cache,requests,4)
    if shell.browser==nil { return receipt }
    for &entry in shell.browser.entries {
        if entry.kind!=.Image { continue }
        view:=render.thumbnail_cache_lookup(cache,shell.browser.thumbnail_inventory_identity,entry.path)
        entry.thumbnail_texture=u64(view.texture); entry.thumbnail_width=view.width; entry.thumbnail_height=view.height; entry.thumbnail_revision=view.accepted_revision; entry.thumbnail_error=view.error!={}
        entry.thumbnail=.Loading if view.loading else .Ready if view.ready else .Failed if entry.thumbnail_error else .Pending
    }
    return receipt
}
