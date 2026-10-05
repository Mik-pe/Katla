#+test
package render

import app ".."
import resources "../../resources"
import "core:testing"
import "core:mem"
import "core:hash"
import "core:slice"
import "core:thread"

TEXTURE_TEST_RGBA :: #load("texture_image_fixtures/rgba.png",[]byte)
TEXTURE_TEST_JPEG :: #load("texture_image_fixtures/rgb.jpg",[]byte)
TEXTURE_TEST_EXPANSION :: #load("texture_image_fixtures/expansion.png",[]byte)
TEXTURE_TEST_RESOURCE_ROOT :: #config(TEXTURE_IMAGE_RESOURCE_ROOT,"resources")

@(test)
test_texture_image_rgba_orientation_alpha_and_allocator_ownership :: proc(t:^testing.T) {
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    decoded,error:=texture_image_decode(TEXTURE_TEST_RGBA,captured)
    testing.expect_value(t,error,Texture_Image_Error.None)
    testing.expect(t,decoded.width==2 && decoded.height==2 && len(decoded.pixels)==16)
    expected:=[16]byte{255,0,0,255,0,255,0,128,0,0,255,64,255,255,255,0}
    testing.expect(t,slice.equal(decoded.pixels,expected[:]))
    testing.expect_value(t,len(tracker.allocation_map),1)
    context.allocator=mem.nil_allocator(); texture_image_destroy(&decoded); context.allocator=backing
    testing.expect(t,decoded.width==0 && decoded.height==0 && decoded.pixels==nil)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_texture_image_real_jpeg_samples_and_allocation_failure :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    decoded,error:=texture_image_decode(TEXTURE_TEST_JPEG,captured)
    testing.expect_value(t,error,Texture_Image_Error.None)
    testing.expect(t,decoded.width==2 && decoded.height==2 && len(decoded.pixels)==16)
    expected:=[12]byte{255,0,0,0,255,0,0,0,255,255,255,255}
    if error==.None {
        for i in 0..<4 { for channel in 0..<3 { testing.expect(t,abs(int(decoded.pixels[i*4+channel])-int(expected[i*3+channel]))<=4) }; testing.expect_value(t,decoded.pixels[i*4+3],byte(255)) }
    }
    texture_image_destroy(&decoded)
    failed,failure:=texture_image_decode(TEXTURE_TEST_JPEG,mem.nil_allocator()); testing.expect_value(t,failure,Texture_Image_Error.Allocation); testing.expect(t,failed.pixels==nil)
    failed,failure=texture_image_decode(TEXTURE_TEST_RGBA,mem.nil_allocator()); testing.expect_value(t,failure,Texture_Image_Error.Allocation); testing.expect(t,failed.pixels==nil)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(private="package")
texture_test_be32 :: proc(data:[]byte,value:u32) { for i in 0..<4 { data[i]=byte(value>>u32((3-i)*8)) } }
@(test)
test_texture_image_corrupt_truncated_and_expansion_rejection :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    png,jpeg:=TEXTURE_TEST_RGBA,TEXTURE_TEST_JPEG
    for input in ([]([]byte){nil,{'b','a','d'},png[:32],png[:len(png)-1],jpeg[:len(jpeg)-2],TEXTURE_TEST_EXPANSION}) {
        value,error:=texture_image_decode(input,captured)
        testing.expect(t,error!=.None && value.pixels==nil)
    }
    corrupt:=make([]byte,len(TEXTURE_TEST_RGBA)); defer delete(corrupt); copy(corrupt,TEXTURE_TEST_RGBA)
    corrupt[45]~=1
    value,error:=texture_image_decode(corrupt,captured); testing.expect_value(t,error,Texture_Image_Error.Invalid_Data); testing.expect(t,value.pixels==nil)
    copy(corrupt,TEXTURE_TEST_RGBA); corrupt[29]~=1
    value,error=texture_image_decode(corrupt,captured); testing.expect_value(t,error,Texture_Image_Error.Invalid_Data)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
    testing.expect(t,tracker.peak_memory_allocated<4096,"A 2×2 PNG must reject oversized inflated payloads within its scanline budget")
}
@(test)
test_texture_image_huge_headers_fail_before_owned_pixel_allocation :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    header:=make([]byte,len(TEXTURE_TEST_RGBA)); defer delete(header)
    for size in ([2][2]u32{{TEXTURE_IMAGE_MAX_DIMENSION+1,1},{TEXTURE_IMAGE_MAX_DIMENSION,TEXTURE_IMAGE_MAX_DIMENSION}}) {
        copy(header,TEXTURE_TEST_RGBA); texture_test_be32(header[16:20],size[0]); texture_test_be32(header[20:24],size[1]); texture_test_be32(header[29:33],hash.crc32(header[12:29]))
        value,error:=texture_image_decode(header,captured); testing.expect_value(t,error,Texture_Image_Error.Limit); testing.expect(t,value.pixels==nil && len(tracker.allocation_map)==0)
    }
    jpeg:=make([]byte,len(TEXTURE_TEST_JPEG)); defer delete(jpeg); copy(jpeg,TEXTURE_TEST_JPEG)
    for i in 0..<len(jpeg)-8 {
        if jpeg[i]==0xff && jpeg[i+1]==0xc0 { jpeg[i+7]=0x20; jpeg[i+8]=1; break }
    }
    value,error:=texture_image_decode(jpeg,captured); testing.expect_value(t,error,Texture_Image_Error.Limit); testing.expect(t,value.pixels==nil && len(tracker.allocation_map)==0)
    oversized:=make([]byte,TEXTURE_IMAGE_MAX_ENCODED_BYTES+1); defer delete(oversized)
    value,error=texture_image_decode(oversized,captured); testing.expect_value(t,error,Texture_Image_Error.Limit)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_texture_image_actual_gltf_embedded_png_and_jpeg :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    root,root_error:=resources.root_open(TEXTURE_TEST_RESOURCE_ROOT); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    png,jpeg:int
    for source in ([]string{"models/DamagedHelmet.glb","models/Avocado.glb"}) {
        model,model_error:=app.gltf_load(&root,source); testing.expect_value(t,model_error,app.Gltf_Error.None); if model_error!=.None { continue }
        for encoded in model.images {
            decoded,error:=texture_image_decode(encoded.encoded,captured); testing.expect_value(t,error,Texture_Image_Error.None)
            if error==.None {
                testing.expect(t,decoded.width>0 && decoded.height>0 && len(decoded.pixels)==int(decoded.width)*int(decoded.height)*4)
                if encoded.mime=="image/png" { png+=1 }; if encoded.mime=="image/jpeg" { jpeg+=1 }
            }
            texture_image_destroy(&decoded)
        }
        app.gltf_model_destroy(&model)
    }
    testing.expect(t,png>0 && jpeg>0,"Decode both actual embedded glTF PNG and JPEG images")
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(private="package")
Texture_Thread_Test :: struct { success:bool }
@(private="package")
texture_test_worker :: proc(worker:^thread.Thread) {
    state:=cast(^Texture_Thread_Test)worker.data; state.success=true
    for _ in 0..<64 {
        decoded,error:=texture_image_decode(TEXTURE_TEST_RGBA)
        state.success=state.success && error==.None && len(decoded.pixels)==16 && decoded.pixels[0]==255 && decoded.pixels[8]==0 && decoded.pixels[10]==255
        texture_image_destroy(&decoded)
    }
}
@(test)
test_texture_image_parallel_orientation_is_consistent :: proc(t:^testing.T) {
    states:[2]Texture_Thread_Test; workers:[2]^thread.Thread
    for &state,i in states { workers[i]=thread.create(texture_test_worker); workers[i].data=&state; thread.start(workers[i]) }
    for worker,i in workers { thread.join(worker); thread.destroy(worker); testing.expect(t,states[i].success) }
}
