#+test
package render

import gfx "../../gfx"
import "core:testing"

@(test)
test_scene_empty_world_replaces_reads_and_recompiles_without_dummy_work :: proc(t:^testing.T) {
    owner:int
    scene:Scene_Graph
    testing.expect_value(t,scene_graph_init(&scene,{&owner,0,0},3,2,32,24),Scene_Error.None)
    defer scene_graph_destroy(&scene)
    initial_revision:=scene.graph.revision
    testing.expect_value(t,scene_graph_draws(&scene,nil),gfx.Packet_Error.None)
    testing.expect_value(t,scene.graph.revision,initial_revision+1)
    testing.expect_value(t,scene.plan.revision,scene.graph.revision)
    pass:=&scene.graph.passes[scene.pass.index]
    packet:=pass.packet.(gfx.Render)
    testing.expect_value(t,len(pass.accesses),0)
    testing.expect_value(t,len(pass.images),2)
    testing.expect_value(t,len(packet.buffers),0)
    testing.expect_value(t,len(packet.phases),0)
    testing.expect(t,packet.colors[0].load==.Clear && packet.depth.load==.Clear)
    clear_revision:=scene.graph.revision
    testing.expect_value(t,scene_graph_draws(&scene,nil),gfx.Packet_Error.None)
    testing.expect_value(t,scene.graph.revision,clear_revision)
    testing.expect_value(t,scene_graph_draws(&scene,{gfx.Draw{4,1,0,0}}),gfx.Packet_Error.Invalid_Packet)
    testing.expect_value(t,scene.graph.revision,clear_revision)
    testing.expect_value(t,len(pass.accesses),0)
    testing.expect_value(t,scene_graph_draws(&scene,{gfx.Draw{3,2,0,0}}),gfx.Packet_Error.None)
    testing.expect_value(t,scene.graph.revision,clear_revision+1)
    testing.expect_value(t,scene.plan.revision,scene.graph.revision)
    testing.expect_value(t,len(pass.accesses),3)
    testing.expect_value(t,len(pass.packet.(gfx.Render).phases),1)
}
