#+test
package render

import "core:testing"
import "core:mem"
import "core:slice"
import "core:fmt"

TEXTURE_TEST_BMP :: #load("texture_image_fixtures/rgb.bmp",[]byte)
TEXTURE_TEST_TIFF :: #load("texture_image_fixtures/rgba.tiff",[]byte)
TEXTURE_TEST_TIFF_DEFLATE :: #load("texture_image_fixtures/deflate.tiff",[]byte)
TEXTURE_TEST_TIFF_PACKBITS :: #load("texture_image_fixtures/packbits.tiff",[]byte)
TEXTURE_TEST_TIFF_LZW :: #load("texture_image_fixtures/lzw.tiff",[]byte)
TEXTURE_TEST_BIGTIFF :: #load("texture_image_fixtures/bigtiff.tiff",[]byte)
TEXTURE_TEST_TIFF_ROTATED :: #load("texture_image_fixtures/rotated.tiff",[]byte)
TEXTURE_TEST_TIFF_OVERSIZE :: #load("texture_image_fixtures/oversize.tiff",[]byte)

@(test)
test_texture_image_bmp_and_compressed_tiff_exact_rgba :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    expected:=[16]byte{255,0,0,255,0,255,0,128,0,0,255,64,255,255,255,0}
    for data,index in ([]([]byte){TEXTURE_TEST_TIFF,TEXTURE_TEST_TIFF_DEFLATE,TEXTURE_TEST_TIFF_PACKBITS,TEXTURE_TEST_TIFF_LZW,TEXTURE_TEST_BIGTIFF}) {
        decoded,error:=texture_image_decode(data,allocator)
        testing.expect(t,error==.None,fmt.tprintf("TIFF fixture %d: %v",index,error))
        testing.expect(t,decoded.width==2 && decoded.height==2 && slice.equal(decoded.pixels,expected[:]))
        texture_image_destroy(&decoded)
    }
    bmp,error:=texture_image_decode(TEXTURE_TEST_BMP,allocator)
    testing.expect_value(t,error,Texture_Image_Error.None)
    bmp_expected:=[16]byte{255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255}
    testing.expect(t,bmp.width==2 && bmp.height==2 && slice.equal(bmp.pixels,bmp_expected[:]))
    texture_image_destroy(&bmp)
    rotated,rotation_error:=texture_image_decode(TEXTURE_TEST_TIFF_ROTATED,allocator)
    testing.expect_value(t,rotation_error,Texture_Image_Error.None)
    rotation_expected:=[16]byte{0,0,255,64,255,0,0,255,255,255,255,0,0,255,0,128}
    testing.expect(t,rotated.width==2 && rotated.height==2 && slice.equal(rotated.pixels,rotation_expected[:]))
    texture_image_destroy(&rotated)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_texture_image_bmp_tiff_invalid_and_limits :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    bmp,tiff:=TEXTURE_TEST_BMP,TEXTURE_TEST_TIFF
    for data in ([]([]byte){bmp[:54],bmp[:len(bmp)-1],tiff[:14],tiff[:len(tiff)-1]}) {
        decoded,error:=texture_image_decode(data,allocator)
        testing.expect(t,error!=.None && decoded.pixels==nil)
    }
    decoded,error:=texture_image_decode(TEXTURE_TEST_TIFF_OVERSIZE,allocator)
    testing.expect_value(t,error,Texture_Image_Error.Limit)
    testing.expect(t,decoded.pixels==nil && len(tracker.allocation_map)==0)
    for data in ([]([]byte){TEXTURE_TEST_BMP,TEXTURE_TEST_TIFF}) {
        rejected,failure:=texture_image_decode(data,mem.nil_allocator())
        testing.expect_value(t,failure,Texture_Image_Error.Allocation)
        testing.expect(t,rejected.pixels==nil)
    }
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="package")
texture_test_le32 :: proc(bytes:[]byte,value:u32) { for i in 0..<4 { bytes[i]=byte(value>>u32(i*8)) } }
@(test)
test_texture_image_tiff_metadata_budget_is_explicit :: proc(t:^testing.T) {
    // A valid optional ICC profile cannot evade the per-handle metadata budget.
    encoded:=make([]byte,8*1024*1024+len(TEXTURE_TEST_TIFF)); defer delete(encoded)
    copy(encoded,TEXTURE_TEST_TIFF)
    encoded[142]=byte(34675&255); encoded[143]=byte(34675>>8); encoded[144]=7; encoded[145]=0
    texture_test_le32(encoded[146:150],8*1024*1024)
    texture_test_le32(encoded[150:154],166)
    decoded,error:=texture_image_decode(encoded,mem.nil_allocator())
    testing.expect_value(t,error,Texture_Image_Error.Limit)
    testing.expect(t,decoded.pixels==nil)
}

TEXTURE_TEST_UPSTREAM_JPEG_TIFF :: #load("texture_image_fixtures/upstream-jpeg.tiff",[]byte)
TEXTURE_TEST_UPSTREAM_GRAY16_TIFF :: #load("texture_image_fixtures/upstream-gray16.tiff",[]byte)
@(test)
test_texture_image_upstream_jpeg_tiff_and_gray16 :: proc(t:^testing.T) {
    gray,error:=texture_image_decode(TEXTURE_TEST_UPSTREAM_GRAY16_TIFF)
    testing.expect_value(t,error,Texture_Image_Error.None)
    defer texture_image_destroy(&gray)
    testing.expect(t,gray.width==157 && gray.height==151)
    low,high:byte=255,0
    for i in 0..<len(gray.pixels)/4 {
        offset:=i*4
        testing.expect(t,gray.pixels[offset]==gray.pixels[offset+1] && gray.pixels[offset]==gray.pixels[offset+2] && gray.pixels[offset+3]==255)
        low=min(low,gray.pixels[offset]); high=max(high,gray.pixels[offset])
    }
    testing.expect(t,low<32 && high>223)
    jpeg,jpeg_error:=texture_image_decode(TEXTURE_TEST_UPSTREAM_JPEG_TIFF)
    testing.expect_value(t,jpeg_error,Texture_Image_Error.None)
    defer texture_image_destroy(&jpeg)
    testing.expect(t,jpeg.width==162 && jpeg.height==20)
    colorful:=false
    for i in 0..<len(jpeg.pixels)/4 {
        offset:=i*4
        colorful=colorful || jpeg.pixels[offset]!=jpeg.pixels[offset+1] || jpeg.pixels[offset+1]!=jpeg.pixels[offset+2]
        testing.expect(t,jpeg.pixels[offset+3]==255)
    }
    testing.expect(t,colorful)
}
