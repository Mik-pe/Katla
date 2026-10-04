//! Checked transfer layouts preserve exact source rows, slices and compressed blocks.
package gfx

/// Pitches and the byte span through the final useful row, excluding trailing padding.
Image_Layout :: struct { row_bytes,block_rows,bytes_per_row,bytes_per_image,required_bytes:u64 }
/// Resolves zero pitches to tightly packed data; never multiplies unchecked dimensions.
image_region_layout :: proc(region:Image_Region,desc:Texture_Desc)->(Image_Layout,bool) {
    if !image_region_valid(region,desc) { return {},false }
    bw,bh,bytes:=texture_block_layout(desc.format)
    if region.aspect!=.Color { bw=1; bh=1; bytes=image_region_pixel_size(desc.format,region.aspect) }
    blocks:=(u64(region.width)+u64(bw)-1)/u64(bw)
    rows:=(u64(region.height)+u64(bh)-1)/u64(bh)
    if blocks>max(u64)/u64(bytes) { return {},false }
    row_bytes:=blocks*u64(bytes)
    row:=region.bytes_per_row; if row==0 { row=row_bytes }
    if row<row_bytes || row%u64(bytes)!=0 || rows>max(u64)/row { return {},false }
    image:=region.bytes_per_image; if image==0 { image=row*rows }
    if image<row*rows || image%u64(bytes)!=0 { return {},false }
    slices:=u64(region.depth)-1
    if slices>max(u64)/image { return {},false }
    required:=slices*image
    tail:=row*(rows-1)
    if tail>max(u64)-row_bytes { return {},false }; tail+=row_bytes
    if required>max(u64)-tail { return {},false }; required+=tail
    return {row_bytes,rows,row,image,required},true
}
