//! Geometry tests cover the usable interior, doorway and placement admission boundaries.
package main
import "core:testing"
import "core:mem/virtual"
@(test)
test_room_plan_doorway_and_usable_interior :: proc(t:^testing.T) {
    arena:virtual.Arena; testing.expect(t,virtual.arena_init_growing(&arena)==nil); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    options:=Options{name="Study",size={6,3,8},origin={10,0,-4},doorway={1.1,2.2},thickness=0.15,ceiling=true}
    parts,ok:=room_plan(options); testing.expect(t,ok && len(parts)==8)
    floor:=parts[0]; testing.expect(t,floor.position==[3]f64{10,-0.075,-4} && floor.scale==[3]f64{6.3,0.15,8.3})
    left,right,lintel:=parts[4],parts[5],parts[6]
    testing.expect(t,abs((left.position[0]+left.scale[0]/2)-(10-0.55))<1e-10)
    testing.expect(t,abs((right.position[0]-right.scale[0]/2)-(10+0.55))<1e-10)
    testing.expect(t,abs(lintel.position[1]-lintel.scale[1]/2-2.2)<1e-10)
    for bad in ([]Options{{name="",size={6,3,8},doorway={1.1,2.2},thickness=0.15},{name="Study",size={6,3,8},doorway={6,2.2},thickness=0.15},{name="Study",size={6,0,8},doorway={1.1,2.2},thickness=0.15},{name="Study",size={6,3,8},doorway={1.1,2.2},thickness=-1}}) { _,accepted:=room_plan(bad); testing.expect(t,!accepted) }
}
@(test)
test_blockout_passage_and_door_approach_rejection :: proc(t:^testing.T) {
    testing.expect(t,placement_valid({-3,0.5,0},{1,1,1}))
    testing.expect(t,!placement_valid({0,0.5,0},{1,1,1}))
    testing.expect(t,!placement_valid({-2.2,0.5,-6},{1,1,1}))
    testing.expect(t,!placement_valid({-3.8,0.5,0},{1,1,1}))
}
