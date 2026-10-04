//! Full default Unicode lowercase matches scene name searches without locale-dependent settings.
package app

import "core:strings"

@(private="package")
scene_case_property :: proc(r:rune,ranges:[][2]rune)->bool {
    low,high:=0,len(ranges)
    for low<high {
        middle:=low+(high-low)/2
        if r<ranges[middle][0] { high=middle }
        else if r>ranges[middle][1] { low=middle+1 }
        else { return true }
    }
    return false
}
@(private="package")
scene_query_lower :: proc(text:string,allocator:=context.allocator)->string {
    runes:=make([dynamic]rune,allocator); defer delete(runes)
    for r in text { append(&runes,r) }
    after:=make([]bool,len(runes),allocator); defer delete(after,allocator)
    following_cased:=false
    for i:=len(runes)-1;i>=0;i-=1 {
        after[i]=following_cased
        if !scene_case_property(runes[i],SCENE_CASE_IGNORABLE_RANGES[:]) { following_cased=scene_case_property(runes[i],SCENE_CASED_RANGES[:]) }
    }
    builder:strings.Builder; strings.builder_init(&builder,allocator); defer strings.builder_destroy(&builder)
    preceding_cased:=false
    for r,i in runes {
        if r==0x0130 { strings.write_string(&builder,"i\u0307") }
        else if r==0x03A3 && preceding_cased && !after[i] { strings.write_rune(&builder,0x03C2) }
        else { strings.write_rune(&builder,scene_simple_lower(r)) }
        if !scene_case_property(r,SCENE_CASE_IGNORABLE_RANGES[:]) { preceding_cased=scene_case_property(r,SCENE_CASED_RANGES[:]) }
    }
    return strings.clone(strings.to_string(builder),allocator)
}
