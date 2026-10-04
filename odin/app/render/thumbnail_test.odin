#+test
package render
import "core:testing"
import "core:mem"

@(test)
test_thumbnail_resize_preserves_aspect_and_bounded_owned_samples :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,context.allocator);defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    image:=Texture_Image{width=512,height=256,pixels=make([]byte,512*256*4,allocator),allocator=allocator}
    for y in 0..<256 { for x in 0..<512 { offset:=(y*512+x)*4;image.pixels[offset]=byte(x%256);image.pixels[offset+1]=byte(y);image.pixels[offset+3]=255 } }
    small,error:=thumbnail_resize(&image,128,allocator);testing.expect_value(t,error,Texture_Image_Error.None)
    testing.expect(t,small.width==128&&small.height==64&&len(small.pixels)==128*64*4)
    testing.expect(t,small.pixels[0]==2&&small.pixels[1]==2&&small.pixels[3]==255)
    testing.expect(t,raw_data(small.pixels)!=raw_data(image.pixels))
    unchanged,unchanged_error:=thumbnail_resize(&small,128,allocator);testing.expect_value(t,unchanged_error,Texture_Image_Error.None);testing.expect(t,raw_data(unchanged.pixels)==raw_data(small.pixels))
    rejected,rejected_error:=thumbnail_resize(&image,128,mem.nil_allocator());testing.expect_value(t,rejected_error,Texture_Image_Error.Allocation);testing.expect(t,len(rejected.pixels)==0)
    texture_image_destroy(&small);texture_image_destroy(&image)
    testing.expect(t,len(tracker.allocation_map)==0&&len(tracker.bad_free_array)==0)
}
