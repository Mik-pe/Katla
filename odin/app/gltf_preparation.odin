//! A preparation transaction reads each model source once and shares its immutable CPU revision.
package app
import ecs "../ecs"
import "core:mem"
import "core:strings"
import "core:fmt"

@(private="package")
Scene_Model_Preparation :: struct { revisions:map[string]^Scene_Model_Revision,allocator:mem.Allocator }
@(private="package")
scene_model_preparation_destroy :: proc(value:rawptr) {
    cache:=cast(^Scene_Model_Preparation)value
    for key,revision in cache.revisions {
        delete(key,cache.allocator); revision.references-=1
        if revision.references==0 { gltf_model_destroy(&revision.model); free(revision,revision.allocator) }
    }
    delete(cache.revisions); cache^={}
}
@(private="package")
scene_model_preparation_begin :: proc(owner:^Authoring)->bool {
    if ecs.contains_resource(&owner.world,Scene_Model_Preparation) { return false }
    ecs.insert_resource(&owner.world,Scene_Model_Preparation{make(map[string]^Scene_Model_Revision,owner.world.allocator),owner.world.allocator},{destroy=scene_model_preparation_destroy})
    return true
}
@(private="package")
scene_model_preparation_end :: proc(owner:^Authoring,owned:bool) { if owned { ecs.remove_resource(&owner.world,Scene_Model_Preparation) } }
@(private="package")
scene_model_preparation_find :: proc(owner:^Authoring,source:Gltf_Source)->(Scene_Model,bool) {
    cache:=ecs.get_resource_mut(&owner.world,Scene_Model_Preparation); if cache==nil { return {},false }
    key:=fmt.aprintf("%d:%s",int(source.root),source.path); defer delete(key,owner.world.allocator)
    revision,present:=cache.revisions[key]; if !present { return {},false }
    assert(revision.references<max(u32)); revision.references+=1
    result:=Scene_Model{source=source,model=revision.model,revision=revision,allocator=owner.world.allocator}; result.source.path=strings.clone(source.path,owner.world.allocator)
    return result,true
}
@(private="package")
scene_model_preparation_insert :: proc(owner:^Authoring,model:^Scene_Model) {
    cache:=ecs.get_resource_mut(&owner.world,Scene_Model_Preparation); if cache==nil { return }
    scene_model_revision_own(model,owner.world.allocator)
    key:=fmt.aprintf("%d:%s",int(model.source.root),model.source.path)
    assert(!(key in cache.revisions)); assert(model.revision.references<max(u32)); model.revision.references+=1
    cache.revisions[key]=model.revision
}
