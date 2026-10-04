#+test
package app
import ecs "../ecs"
import "core:testing"
import "core:mem"

@(test)
test_gltf_animation_inherits_nearest_explicit_player_through_full_hierarchy :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); authoring_services_init(&owner)
    outer:=ecs.create_entity(&owner.world); group:=ecs.create_entity(&owner.world); middle:=ecs.create_entity(&owner.world); child:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,group,Scene_Parent{outer}); ecs.add_component(&owner.world,middle,Scene_Parent{group}); ecs.add_component(&owner.world,child,Scene_Parent{middle})
    ecs.add_component(&owner.world,group,Scene_Model{source={kind=.Group,path=""}}); ecs.add_component(&owner.world,child,Scene_Model{source={kind=.Primitive,path=""}})
    ecs.add_component(&owner.world,outer,Animation_Player{time=7})
    testing.expect(t,scene_model_animation_player(&owner,child)==ecs.get_component_mut(&owner.world,outer,Animation_Player))
    ecs.add_component(&owner.world,middle,Animation_Player{time=3})
    testing.expect(t,scene_model_animation_player(&owner,child)==ecs.get_component_mut(&owner.world,middle,Animation_Player))
    ecs.add_component(&owner.world,child,Animation_Player{time=1})
    testing.expect(t,scene_model_animation_player(&owner,child)==ecs.get_component_mut(&owner.world,child,Animation_Player))
    ecs.remove_component(&owner.world,child,Animation_Player); ecs.remove_component(&owner.world,middle,Animation_Player); ecs.remove_component(&owner.world,outer,Animation_Player)
    testing.expect(t,scene_model_animation_player(&owner,child)==nil)
    ecs.add_component(&owner.world,outer,Scene_Parent{child})
    testing.expect(t,scene_model_animation_player(&owner,child)==nil)
    ecs.remove_component(&owner.world,outer,Scene_Parent); ecs.destroy_entity(&owner.world,middle)
    testing.expect(t,scene_model_animation_player(&owner,child)==nil)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
