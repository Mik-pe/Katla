// A runnable ECS consumer using the shared Katla math package.
package main

import "core:fmt"
import ecs "../../ecs"
import km "../../math"

Spatial :: struct { transform: km.Transform }
Velocity :: struct { linear: km.Vec3 }
Row :: struct { spatial: ecs.Write(Spatial), velocity: ecs.Read(Velocity) }
Params :: struct { query: ecs.Query(Row,ecs.No_Filter) }

movement :: proc(_: ^int,params: ^Params,dt: f32) -> ecs.System_Error {
    for id in ecs.query_entities(&params.query) {
        row,ok := ecs.query_row(&params.query,id)
        assert(ok)
        spatial := ecs.write(row.spatial)
        spatial.transform.position += ecs.read(row.velocity).linear * dt
    }
    return .None
}
main :: proc() {
    world: ecs.World
    ecs.world_init(&world,worker_count=2)
    defer ecs.world_destroy(&world)
    id := ecs.spawn(&world,struct { spatial: Spatial, velocity: Velocity }{Spatial{km.TRANSFORM_IDENTITY},Velocity{km.Vec3{2,0,0}}})
    _,err := ecs.register_typed_system(&world,0,Params,movement)
    assert(err == .None)
    for _ in 0..<10 { assert(ecs.world_update(&world,0.1,parallel=true) == .None) }
    value,ok := ecs.get_component(&world,id,Spatial)
    assert(ok && abs(value.transform.position[0]-2) < 1e-5)
    point := km.transform_point(km.transform_to_mat4(value.transform),km.Vec3{1,0,0})
    assert(abs(point[0]-3) < 1e-5 && ecs.validate(&world))
    fmt.println("ECS + math: entity moved to",value.transform.position,"; transformed point:",point)
}
