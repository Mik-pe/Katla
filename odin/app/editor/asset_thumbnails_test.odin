#+test
#+build darwin, linux
//! Real confined image jobs feed retained rows without stealing selection or uploading clipped entries.
package editor_app
import app ".."
import assets "../assets"
import render "../render"
import gfx "../../gfx"
import ui "../../ui"
import "core:testing"
import "core:os"
import "core:strings"
import "core:fmt"
import "core:mem"
import "core:time"

@(private="file")
ASSET_THUMBNAIL_PNG :: #load("../render/texture_image_fixtures/rgba.png",[]byte)
@(private="file")
Asset_Thumbnail_Renderer :: struct { created,released:u32,first_pixel:[4]byte }
@(private="file")
asset_thumbnail_create :: proc(renderer:^Asset_Thumbnail_Renderer,desc:gfx.Texture_Desc,bytes:[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    if desc.width!=2 || desc.height!=2 || desc.format!=.RGBA8_Srgb || len(bytes)!=16 { return {},.Invalid_Range }
    renderer.created+=1; copy(renderer.first_pixel[:],bytes[:4]); return {renderer,renderer.created,1},.None
}
@(private="file")
asset_thumbnail_release :: proc(renderer:^Asset_Thumbnail_Renderer,handle:gfx.Texture_Handle)->gfx.Gpu_Error { if handle.owner!=renderer { return .Invalid_Resource }; renderer.released+=1; return .None }
@(private="file")
asset_thumbnail_frame :: proc(t:^testing.T,shell:^Shell,input:[]ui.Input_Event=nil) {
    shell_frame_destroy(shell); descriptor:=shell_assets(shell); _,result:=ui.frame(shell.ctx,descriptor,{events=input},{1000,390}); testing.expect(t,result.error==.None); shell_actions(shell)
}
@(private="file")
asset_thumbnail_wait :: proc(t:^testing.T,shell:^Shell,cache:^render.Thumbnail_Cache(Asset_Thumbnail_Renderer),path:string,expect_failure:bool=false) {
    finished:=false
    for _ in 0..<1000 {
        shell_asset_thumbnails_update(shell,cache)
        for entry in shell.browser.entries { if entry.path==path && entry.thumbnail!=.Loading && (entry.thumbnail_error if expect_failure else entry.thumbnail==.Ready) { finished=true } }
        if finished { break }; time.sleep(time.Millisecond)
    }
    testing.expect(t,finished)
}
@(private="file")
asset_thumbnail_fixture :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-asset-thumbnail-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for i in 0..<12 { path:=fmt.aprintf("%s/image%02d.png",resource,i); testing.expect(t,os.write_entire_file(path,ASSET_THUMBNAIL_PNG)==nil); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect(t,app.asset_resources_init(&owner,directory,resource)==.None)
    browser:assets.State; assets.init(&browser,&owner); defer assets.destroy(&browser); testing.expect(t,assets.refresh(&browser)==.None)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; testing.expect(t,ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme})==.None); defer ui.context_destroy(&ctx)
    shell:=Shell{state=&state,ctx=&ctx,browser=&browser,allocator=owner.world.allocator}; defer shell_destroy(&shell)
    renderer:Asset_Thumbnail_Renderer
    gpu:=render.UI_GPU(Asset_Thumbnail_Renderer){renderer=&renderer,ops={create_texture=asset_thumbnail_create,destroy_texture=asset_thumbnail_release},textures=make(map[ui.Texture_Id]render.UI_Texture),allocator=context.allocator}; defer delete(gpu.textures)
    cache:render.Thumbnail_Cache(Asset_Thumbnail_Renderer); testing.expect(t,render.thumbnail_cache_init(&cache,&gpu)=={}); defer { testing.expect(t,render.thumbnail_cache_destroy(&cache)=={}); testing.expect(t,renderer.created==renderer.released) }
    asset_thumbnail_frame(t,&shell)
    requests:=shell_asset_thumbnail_requests(&shell); testing.expect(t,len(requests)>0 && len(requests)<len(browser.entries)); visible_count:=len(requests); delete(requests)
    queued:=shell_asset_thumbnails_update(&shell,&cache); testing.expect(t,queued.attempted==min(4,visible_count) && renderer.created==0 && browser.entries[0].thumbnail==.Loading)
    asset_thumbnail_wait(t,&shell,&cache,"image00.png")
    accepted:=browser.entries[0].thumbnail_texture; testing.expect(t,accepted!=0 && browser.entries[0].thumbnail_width==2 && browser.entries[0].thumbnail_height==2 && renderer.first_pixel==([4]byte{255,0,0,255}))
    asset_thumbnail_frame(t,&shell); image_node:=ctx.nodes[key(55,"image00.png",u64(browser.root))]; testing.expect(t,image_node!=nil && image_node.descriptor.texture==ui.Texture_Id(accepted))
    commands_found:=false; for command in ctx.commands { if draw,ok:=command.(ui.Image_Draw); ok && draw.texture==ui.Texture_Id(accepted) && draw.clip.width>0 { commands_found=true } }; testing.expect(t,commands_found)
    point:=ui.Vec2{image_node.bounds.x+image_node.bounds.width/2,image_node.bounds.y+image_node.bounds.height/2}
    asset_thumbnail_frame(t,&shell,{ui.Pointer_Down{position=point},ui.Pointer_Up{position=point}}); testing.expect(t,browser.selected=="image00.png")
    revision:=browser.entries[0].thumbnail_revision; testing.expect(t,assets.refresh(&browser)==.None); shell_asset_thumbnails_update(&shell,&cache); asset_thumbnail_wait(t,&shell,&cache,"image00.png"); testing.expect(t,browser.entries[0].thumbnail_texture==accepted && browser.entries[0].thumbnail_revision>revision)
    path:=strings.concatenate({resource,"/image00.png"}); defer delete(path); testing.expect(t,os.write_entire_file(path,"invalid replacement")==nil); testing.expect(t,assets.refresh(&browser)==.None)
    shell_asset_thumbnails_update(&shell,&cache); asset_thumbnail_wait(t,&shell,&cache,"image00.png",true); testing.expect(t,browser.entries[0].thumbnail_texture==accepted && browser.entries[0].thumbnail_error)
    asset_thumbnail_frame(t,&shell); testing.expect(t,ctx.nodes[key(57,"image00.png",u64(browser.root))].descriptor.text=="Update failed · showing previous preview")
    // An inactive retained panel cannot enqueue work for invisible image rows.
    _,frame_error:=ui.frame(&ctx,ui.Descriptor{key=999,kind=.Text,text="Other panel"},{},{1000,390}); testing.expect(t,frame_error.error==.None)
    hidden:=shell_asset_thumbnail_requests(&shell); testing.expect(t,len(hidden)==0); delete(hidden)
    shell_asset_thumbnails_update(&shell,&cache)
}
@(test)
test_asset_thumbnail_background_pixels_visible_rows_selection_refresh_and_failed_replacement :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    asset_thumbnail_fixture(t); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
