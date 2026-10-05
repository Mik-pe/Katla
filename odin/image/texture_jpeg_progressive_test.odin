#+test
package katla_image
import "core:testing"
import "core:mem"
import "core:fmt"

@(test)
test_progressive_jpeg_sampling_and_restart_pixels :: proc(t:^testing.T) {
    encoded:=[]([]byte){#load("fixtures/progressive-0.jpg",[]byte),#load("fixtures/progressive-1.jpg",[]byte),#load("fixtures/progressive-2.jpg",[]byte),#load("fixtures/progressive-restarts.jpg",[]byte)}
    expected:=[]([]byte){#load("fixtures/progressive-0.rgba",[]byte),#load("fixtures/progressive-1.rgba",[]byte),#load("fixtures/progressive-2.rgba",[]byte),#load("fixtures/progressive-restarts.rgba",[]byte)}
    for input,index in encoded {
        decoded,error:=texture_image_decode(input)
        testing.expect(t,error==.None,fmt.tprintf("progressive fixture %d: %v",index,error)); if error!=.None { continue }; defer texture_image_destroy(&decoded)
        testing.expect(t,decoded.width==31 && decoded.height==19 && decoded.format==.RGBA8 && len(decoded.pixels)==len(expected[index]))
        maximum,total:int
        for value,i in decoded.pixels { difference:=abs(int(value)-int(expected[index][i])); maximum=max(maximum,difference); total+=difference }
        testing.expect(t,maximum<=3 && total<=len(decoded.pixels),fmt.tprintf("fixture %d max %d total %d",index,maximum,total))
    }
}
@(test)
test_progressive_jpeg_rejects_truncation_and_failed_allocation :: proc(t:^testing.T) {
    input:=#load("fixtures/progressive-restarts.jpg",[]byte)
    for length in 0..<len(input) {
        decoded,error:=texture_image_decode(input[:length]); testing.expect(t,error!=.None && decoded.pixels==nil)
    }
    decoded,error:=texture_image_decode(input,mem.nil_allocator()); testing.expect(t,error==.Allocation && decoded.pixels==nil)
}
