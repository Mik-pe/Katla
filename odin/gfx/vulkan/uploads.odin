//! Slot-owned coherent blocks keep per-phase constants aligned and immutable until retirement.
package katla_vulkan

import gfx ".."

@(private="package")
Native_Upload_Block :: struct { buffer:^Native_Buffer, cursor:u64 }
@(private="package")
upload_constant :: proc(r:^Renderer,slot:^Native_Frame,bytes:[]byte)->(^Native_Buffer,u64,gfx.Gpu_Error) {
    alignment:=max(u64(1),max(u64(r.limits.minUniformBufferOffsetAlignment),u64(r.limits.minStorageBufferOffsetAlignment)))
    for &block in slot.uploads {
        offset:=(block.cursor+alignment-1)/alignment*alignment
        if offset>block.buffer.desc.size || u64(len(bytes))>block.buffer.desc.size-offset { continue }
        block.cursor=offset+u64(len(bytes))
        copy((cast([^]byte)block.buffer.mapped)[int(offset):int(block.cursor)],bytes)
        retained:=false; for buffer in slot.buffers { if buffer==block.buffer { retained=true; break } }
        if !retained { block.buffer.refs+=1; append(&slot.buffers,block.buffer) }
        return block.buffer,offset,.None
    }
    capacity:=max(u64(16384),u64(len(bytes))+alignment)
    handle,error:=create_buffer(r,{size=capacity,usage={.Uniform,.Storage},memory=.CPU_Visible})
    if error!=.None { return nil,0,error }
    buffer,_:=gfx.storage_remove(&r.buffers,handle)
    append(&slot.uploads,Native_Upload_Block{buffer,0})
    return upload_constant(r,slot,bytes)
}
