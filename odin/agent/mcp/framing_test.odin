#+test
package mcp

import "core:testing"
import "core:strings"
import "core:bytes"
import "core:io"

@(test)
test_framing_preserves_fragmented_and_coalesced_lines_and_rejects_partial_eof :: proc(t:^testing.T) {
    buffer:bytes.Reader
    reader:Line_Reader; line_reader_init(&reader,bytes.reader_init(&buffer,transmute([]byte)string("first\nsecond\r\npartial")))
    first:=line_reader_next(&reader); defer frame_destroy(&first)
    second:=line_reader_next(&reader); defer frame_destroy(&second)
    partial:=line_reader_next(&reader); defer frame_destroy(&partial)
    eof:=line_reader_next(&reader); defer frame_destroy(&eof)
    testing.expect(t,string(first.data)=="first" && string(second.data)=="second\r" && first.error==.None && second.error==.None)
    testing.expect(t,partial.error==.Unterminated && len(partial.data)==0 && eof.error==.EOF)
}
@(test)
test_oversized_frame_discards_input_until_newline_and_recovers :: proc(t:^testing.T) {
    prefix:=strings.repeat("x",MAX_MESSAGE_BYTES+1); defer delete(prefix)
    data:=strings.concatenate({prefix,"\n{}\n"}); defer delete(data)
    buffer:bytes.Reader
    reader:Line_Reader; line_reader_init(&reader,bytes.reader_init(&buffer,transmute([]byte)data))
    oversized:=line_reader_next(&reader); defer frame_destroy(&oversized)
    valid:=line_reader_next(&reader); defer frame_destroy(&valid)
    testing.expect(t,oversized.error==.Oversized && len(oversized.data)==0 && valid.error==.None && string(valid.data)=="{}")
}
failed_reader :: proc(data:rawptr,mode:io.Stream_Mode,p:[]byte,offset:i64,whence:io.Seek_From)->(i64,io.Error) { return 0,.Unknown }
@(test)
test_framing_reports_terminal_read_failure :: proc(t:^testing.T) {
    reader:Line_Reader; line_reader_init(&reader,io.Stream{procedure=failed_reader})
    failed:=line_reader_next(&reader); eof:=line_reader_next(&reader)
    testing.expect(t,failed.error==.Read_Failed && eof.error==.EOF)
}
