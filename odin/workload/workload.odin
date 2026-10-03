// Matched compile workload: eight query arities, typed scheduling and lifecycle.
package workload

import ecs "../ecs"

STEP :: f32(1.0)
C1 :: struct { value:f32 }
C2 :: struct { value:f32 }
C3 :: struct { value:f32 }
C4 :: struct { value:f32 }
C5 :: struct { value:f32 }
C6 :: struct { value:f32 }
C7 :: struct { value:f32 }
C8 :: struct { value:f32 }

Move_Row :: struct { a:ecs.Write(C1), b:ecs.Read(C2) }
Move_Params :: struct { query:ecs.Query(Move_Row,ecs.No_Filter) }
move_tick :: proc(_:^int,p:^Move_Params,dt:f32)->ecs.System_Error {
    for id in ecs.query_entities(&p.query) { row,ok:=ecs.query_row(&p.query,id); assert(ok); ecs.write(row.a).value+=ecs.read(row.b).value*dt }
    return .None
}
run :: proc()->f64 {
    world:ecs.World; ecs.world_init(&world); defer ecs.world_destroy(&world)
    ids:=make([dynamic]ecs.Entity_Id); defer delete(ids)
    Bundle :: struct {c1:C1,c2:C2,c3:C3,c4:C4,c5:C5,c6:C6,c7:C7,c8:C8}
    for i in 0..<2048 {
        append(&ids,ecs.spawn(&world,Bundle{C1{f32(i)},C2{2},C3{3},C4{4},C5{5},C6{6},C7{7},C8{8}}))
    }
    checksum:f64
    Row1 :: struct {c1:ecs.Read(C1)}
    q1:=ecs.query_begin(&world,Row1,ecs.No_Filter)
    for id in ecs.query_entities(&q1) { row,ok:=ecs.query_row(&q1,id); assert(ok); checksum+=f64(ecs.read(row.c1).value) }
    ecs.query_end(&world,&q1)
    Row2 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2)}
    q2:=ecs.query_begin(&world,Row2,ecs.No_Filter)
    for id in ecs.query_entities(&q2) { row,ok:=ecs.query_row(&q2,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value) }
    ecs.query_end(&world,&q2)
    Row3 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3)}
    q3:=ecs.query_begin(&world,Row3,ecs.No_Filter)
    for id in ecs.query_entities(&q3) { row,ok:=ecs.query_row(&q3,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value) }
    ecs.query_end(&world,&q3)
    Row4 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3),c4:ecs.Read(C4)}
    q4:=ecs.query_begin(&world,Row4,ecs.No_Filter)
    for id in ecs.query_entities(&q4) { row,ok:=ecs.query_row(&q4,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value)+f64(ecs.read(row.c4).value) }
    ecs.query_end(&world,&q4)
    Row5 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3),c4:ecs.Read(C4),c5:ecs.Read(C5)}
    q5:=ecs.query_begin(&world,Row5,ecs.No_Filter)
    for id in ecs.query_entities(&q5) { row,ok:=ecs.query_row(&q5,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value)+f64(ecs.read(row.c4).value)+f64(ecs.read(row.c5).value) }
    ecs.query_end(&world,&q5)
    Row6 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3),c4:ecs.Read(C4),c5:ecs.Read(C5),c6:ecs.Read(C6)}
    q6:=ecs.query_begin(&world,Row6,ecs.No_Filter)
    for id in ecs.query_entities(&q6) { row,ok:=ecs.query_row(&q6,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value)+f64(ecs.read(row.c4).value)+f64(ecs.read(row.c5).value)+f64(ecs.read(row.c6).value) }
    ecs.query_end(&world,&q6)
    Row7 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3),c4:ecs.Read(C4),c5:ecs.Read(C5),c6:ecs.Read(C6),c7:ecs.Read(C7)}
    q7:=ecs.query_begin(&world,Row7,ecs.No_Filter)
    for id in ecs.query_entities(&q7) { row,ok:=ecs.query_row(&q7,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value)+f64(ecs.read(row.c4).value)+f64(ecs.read(row.c5).value)+f64(ecs.read(row.c6).value)+f64(ecs.read(row.c7).value) }
    ecs.query_end(&world,&q7)
    Row8 :: struct {c1:ecs.Read(C1),c2:ecs.Read(C2),c3:ecs.Read(C3),c4:ecs.Read(C4),c5:ecs.Read(C5),c6:ecs.Read(C6),c7:ecs.Read(C7),c8:ecs.Read(C8)}
    q8:=ecs.query_begin(&world,Row8,ecs.No_Filter)
    for id in ecs.query_entities(&q8) { row,ok:=ecs.query_row(&q8,id); assert(ok); checksum+=f64(ecs.read(row.c1).value)+f64(ecs.read(row.c2).value)+f64(ecs.read(row.c3).value)+f64(ecs.read(row.c4).value)+f64(ecs.read(row.c5).value)+f64(ecs.read(row.c6).value)+f64(ecs.read(row.c7).value)+f64(ecs.read(row.c8).value) }
    ecs.query_end(&world,&q8)
    _,err:=ecs.register_typed_system(&world,0,Move_Params,move_tick); assert(err==.None)
    for _ in 0..<3 { assert(ecs.world_update(&world,STEP)==.None) }
    for id,i in ids { if i%3==0 { assert(ecs.destroy_entity(&world,id)) } }
    for _ in 0..<100 { ecs.spawn(&world,struct{a:C1,b:C2}{C1{10},C2{2}}) }
    assert(ecs.world_update(&world,STEP)==.None)
    q:=ecs.query_begin(&world,struct{a:ecs.Read(C1)},ecs.No_Filter)
    for id in ecs.query_entities(&q) { row,ok:=ecs.query_row(&q,id); assert(ok); checksum+=f64(ecs.read(row.a).value) }
    ecs.query_end(&world,&q)
    assert(ecs.validate(&world) && world.live_count==1465)
    return checksum
}
