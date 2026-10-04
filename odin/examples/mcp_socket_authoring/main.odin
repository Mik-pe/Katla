//! A real CPU scene owner used to validate independent stdio clients against one running world.
package main

import app "../../app"
import mcp "../../agent/mcp"
import resources "../../resources"
import editor "../../editor"
import "core:os"
import "core:fmt"
import "core:time"
import "core:mem"
import "core:strings"

main :: proc() {
    assert(len(os.args)==3)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator)
    allocator:=mem.tracking_allocator(&tracker); context.allocator=allocator
    resource_root:=strings.concatenate({os.args[2],"/resources"},allocator)
    owner:app.Authoring; app.authoring_init(&owner,allocator)
    assert(app.authoring_services_init(&owner)==editor.Scene_Error.None)
    assert(app.asset_resources_init(&owner,os.args[2],resource_root)==resources.Error.None)
    server:mcp.Socket_Server
    assert(mcp.socket_server_init(&server,os.args[1],&owner.agent,allocator)==.None)
    fmt.eprintln("READY private MCP world")
    started:=time.tick_now(); idle_started:=time.tick_now(); had_client:bool
    for time.tick_since(started)<20*time.Second {
        now:=time.tick_since(started)
        assert(mcp.socket_server_tick(&server,now)==.None)
        app.authoring_tick(&owner)
        assert(mcp.socket_server_tick(&server,now)==.None)
        if mcp.socket_server_client_count(&server)>0 { had_client=true; idle_started=time.tick_now() }
        if had_client && mcp.socket_server_client_count(&server)==0 && time.tick_since(idle_started)>time.Second { break }
        time.sleep(time.Millisecond)
    }
    assert(had_client && owner.world.live_count==2 && owner.agent.outstanding==0)
    fmt.eprintln("PASS two independent clients share the actual world and history")
    mcp.socket_server_destroy(&server); app.authoring_destroy(&owner); delete(resource_root,allocator)
    assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker)
}
