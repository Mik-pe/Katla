package image
import "core:testing"
import "core:math"
import "core:fmt"
import "core:mem"

PNG16 :: [4][]byte{#load("fixtures/png16_1.png",[]byte),#load("fixtures/png16_2.png",[]byte),#load("fixtures/png16_3.png",[]byte),#load("fixtures/png16_4.png",[]byte)}
TIFF16 :: [4][]byte{#load("fixtures/rgba16.tiff",[]byte),#load("fixtures/rgba16_be.tiff",[]byte),#load("fixtures/rgba16_planar.tiff",[]byte),#load("fixtures/rgba16_tile_rotated.tiff",[]byte)}
TIFF_FLOAT :: [2][]byte{#load("fixtures/rgba32f.tiff",[]byte),#load("fixtures/rgba32f_tile.tiff",[]byte)}

@(test)
test_png_native_16_bit_channel_expansion_preserves_samples :: proc(t:^testing.T) {
    for encoded,index in PNG16 {
        decoded,error:=texture_image_decode(encoded); testing.expect(t,error==.None,fmt.tprintf("fixture %d error %v",index,error)); if error!=.None { continue }; defer texture_image_destroy(&decoded)
        testing.expect(t,decoded.width==2 && decoded.height==1 && decoded.format==.RGBA16 && len(decoded.pixels)==16)
        first:=texture_image_sample(&decoded,0)
        expected:=[4]u16{12487,12487,12487,65535}
        if index==1 { expected[3]=23333 }; if index>=2 { expected[1]=23333; expected[2]=65535 }; if index==3 { expected[3]=32768 }
        for value,c in expected { testing.expect(t,first[c]==f32(value)/65535) }
    }
}

@(test)
test_tiff_native_16_bit_endian_planar_tile_and_orientation :: proc(t:^testing.T) {
    for encoded,index in TIFF16 {
        decoded,error:=texture_image_decode(encoded); testing.expect(t,error==.None,fmt.tprintf("fixture %d error %v",index,error)); if error!=.None { continue }; defer texture_image_destroy(&decoded)
        testing.expect(t,decoded.width==2 && decoded.height==2 && decoded.format==.RGBA16 && len(decoded.pixels)==32)
        first_index:=1 if index==3 else 0
        first:=texture_image_sample(&decoded,first_index)
        for value,c in ([4]u16{12487,23333,65535,32768}) { testing.expect(t,first[c]==f32(value)/65535) }
        last_index:=2 if index==3 else 3
        last:=texture_image_sample(&decoded,last_index)
        for value,c in ([4]u16{55555,43210,32109,21098}) { testing.expect(t,last[c]==f32(value)/65535) }
    }
}

@(test)
test_tiff_native_hdr_precision_and_explicit_display_preview :: proc(t:^testing.T) {
    for encoded in TIFF_FLOAT {
        decoded,error:=texture_image_decode(encoded); testing.expect(t,error==.None); if error!=.None { continue }; defer texture_image_destroy(&decoded)
        testing.expect(t,decoded.format==.RGBA32_Float && len(decoded.pixels)==64)
        testing.expect(t,texture_image_sample(&decoded,0)==[4]f32{0.5,1.25,8,0.25})
        testing.expect(t,texture_image_sample(&decoded,1)==[4]f32{2,0.125,65504,1})
        testing.expect(t,texture_image_sample(&decoded,2)==[4]f32{-1,3,4,0.5})
        display,display_error:=texture_image_rgba8(&decoded); testing.expect(t,display_error==.None); defer texture_image_destroy(&display)
        testing.expect(t,display.format==.RGBA8 && display.pixels[0]==188 && display.pixels[1]==255 && display.pixels[2]==255 && display.pixels[3]==64)
        testing.expect(t,texture_image_sample(&decoded,0).r==0.5)
        testing.expect(t,math.abs(texture_image_sample(&display,0).a-0.25)<0.005)
    }
}

@(test)
test_float_image_rejects_nonfinite_and_half_range_overflow :: proc(t:^testing.T) {
    for encoded in ([]([]byte){#load("fixtures/rgba32f_nan.tiff",[]byte),#load("fixtures/rgba32f_overflow.tiff",[]byte)}) {
        decoded,error:=texture_image_decode(encoded); testing.expect(t,error==.Invalid_Data && len(decoded.pixels)==0)
    }
}

@(test)
test_precise_image_owned_allocation_failure_releases_native_samples :: proc(t:^testing.T) {
    for encoded in ([]([]byte){PNG16[3],TIFF16[0],TIFF_FLOAT[0]}) {
        decoded,error:=texture_image_decode(encoded,mem.nil_allocator())
        testing.expect(t,error==.Allocation && decoded.pixels==nil && decoded.width==0)
    }
}
