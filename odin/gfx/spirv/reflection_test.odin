#+test
package spirv

import "core:testing"

fixture :: []u32 {
    0x07230203,0x00010300,0,12,0,
    2<<16|17,1,
    3<<16|14,0,1,
    5<<16|15,5,10,0x6e69616d,0,
    6<<16|16,10,17,64,1,1,
    3<<16|71,5,2,
    4<<16|71,7,33,1,
    4<<16|71,7,34,0,
    5<<16|72,5,0,35,0,
    2<<16|19,1,
    3<<16|33,2,1,
    4<<16|21,3,32,0,
    4<<16|23,4,3,4,
    3<<16|30,5,4,
    4<<16|32,6,2,5,
    4<<16|59,6,7,2,
    5<<16|54,1,10,0,2,
    2<<16|248,11,
    1<<16|253,
    1<<16|56,
}

@(test)
test_reflect_declared_uniform_span_and_local_size :: proc(t:^testing.T) {
    reflection,err:=reflect(fixture,"main"); defer destroy(&reflection)
    testing.expect_value(t,err,Error.None)
    testing.expect_value(t,reflection.local_size,([3]u32{64,1,1}))
    testing.expect_value(t,len(reflection.buffers),1)
    testing.expect_value(t,reflection.buffers[0],(Buffer{1,false,16}))
}

@(test)
test_reflect_truncation_and_invalid_instruction_lengths :: proc(t:^testing.T) {
    source:=fixture
    for size in 0..<len(source) {
        reflection,err:=reflect(source[:size],"main")
        testing.expect(t,err!=.None)
        destroy(&reflection)
    }
    for count in ([]u32{0,1,2,65535}) {
        words:=make([]u32,len(fixture)); copy(words,fixture)
        words[7]=count<<16|14
        reflection,err:=reflect(words,"main")
        testing.expect(t,err!=.None)
        destroy(&reflection); delete(words)
    }
}

@(test)
test_reflect_rejects_foreign_sets_and_invalid_result_ids :: proc(t:^testing.T) {
    for mode in 0..<4 {
        words:=make([]u32,len(fixture)); copy(words,fixture)
        for cursor:=5; cursor<len(words); {
            count:=int(words[cursor]>>16); opcode:=words[cursor]&0xffff
            args:=words[cursor+1:cursor+count]
            if mode==0 && opcode==71 && args[1]==34 { args[2]=1 }
            if mode==1 && opcode==59 { args[1]=0 }
            if mode==2 && opcode==59 { args[1]=words[3] }
            if mode==3 && opcode==72 { words[cursor]=4<<16|72 }
            cursor+=count
        }
        reflection,err:=reflect(words,"main")
        testing.expect_value(t,err,Error.Unsupported if mode==0 else Error.Invalid_Module)
        destroy(&reflection); delete(words)
    }
}

@(test)
test_reflect_matrix_layout_and_recursive_type_rejection :: proc(t:^testing.T) {
    module:=Module{nodes=make(map[u32]Node),decorations=make([dynamic]Decoration)}
    defer delete(module.nodes); defer delete(module.decorations)
    module.nodes[1]={22,{1,32}}
    module.nodes[2]={23,{2,1,3}}
    module.nodes[3]={24,{3,2,2}}
    column,column_ok:=span(&module,3,0,16)
    row,row_ok:=span(&module,3,0,16,true)
    testing.expect(t,column_ok && row_ok)
    testing.expect_value(t,column,u64(28))
    testing.expect_value(t,row,u64(40))
    _,undersized:=span(&module,3,0,4)
    testing.expect(t,!undersized)
    module.nodes[4]={30,{4,4}}
    append(&module.decorations,Decoration{target=4,member=0,kind=35,is_member=true})
    _,recursive:=span(&module,4,0)
    testing.expect(t,!recursive)
}
