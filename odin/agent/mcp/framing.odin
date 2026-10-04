//! Newline frames never allocate beyond the configured input limit.
package mcp

import "core:io"
import "core:mem"

/// Distinguishes a complete line, an oversized line and incomplete/error termination.
Frame_Error :: enum { None, Oversized, Unterminated, Read_Failed, EOF }
/// Owns one frame's bytes; failed frames carry no partial request.
Frame :: struct { data:[]byte, error:Frame_Error, allocator:mem.Allocator }
/// Blocking stream reader, confined to one transport input thread.
Line_Reader :: struct { stream:io.Reader, scratch:[4096]byte, used,offset:int, terminal:bool, allocator:mem.Allocator }
/// Initializes a caller-owned stream adapter; destruction of the stream belongs to its owner.
line_reader_init :: proc(reader:^Line_Reader,stream:io.Reader,allocator:=context.allocator) {
    reader^={stream=stream,allocator=allocator}
}
/// Releases a returned line with its captured allocator.
frame_destroy :: proc(frame:^Frame) { delete(frame.data,frame.allocator); frame^={} }
/// Returns one newline-delimited message, retaining at most one MiB while discarding oversized input.
line_reader_next :: proc(reader:^Line_Reader)->Frame {
    if reader.terminal { return {error=.EOF} }
    buffer:=make([dynamic]byte,0,4096,reader.allocator)
    defer delete(buffer)
    oversized:=false
    for {
        if reader.offset==reader.used {
            count,err:=io.read(reader.stream,reader.scratch[:])
            reader.offset=0; reader.used=count
            if err!=nil && err!=.EOF { reader.terminal=true; return {error=.Read_Failed} }
            if count==0 {
                reader.terminal=true
                if oversized { return {error=.Oversized} }
                if len(buffer)>0 { return {error=.Unterminated} }
                return {error=.EOF}
            }
        }
        ch:=reader.scratch[reader.offset]; reader.offset+=1
        if ch=='\n' {
            if oversized { return {error=.Oversized} }
            data:=make([]byte,len(buffer),reader.allocator); copy(data,buffer[:])
            return {data=data,allocator=reader.allocator}
        }
        if len(buffer)==MAX_MESSAGE_BYTES { oversized=true }
        if !oversized { append(&buffer,ch) }
    }
}
