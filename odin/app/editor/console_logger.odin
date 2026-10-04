//! A bounded owner-thread Console drain receives engine logs without changing terminal diagnostics.
package editor_app

import "core:log"
import "core:mem"
import "core:sync"
import "core:fmt"

/// Installation and teardown require the same context and a stationary sink.
Console_Logger_Error :: enum { None,Already_Installed,Not_Installed,Wrong_Context }
@(private="package")
CONSOLE_LOG_RECORDS :: 256
@(private="package")
CONSOLE_LOG_BYTES :: 8192
@(private="package")
Console_Log_Record :: struct { level:Console_Level,length:int,bytes:[CONSOLE_LOG_BYTES]byte }
/// Producers retain this owner until every worker using its logger has joined.
/// Fixed storage bounds capture memory; forwarding retains the previous logger's ownership.
Console_Logger :: struct {
    previous:log.Logger,
    records:[]Console_Log_Record,
    queue_mutex,terminal_mutex:sync.Mutex,
    first,count:int,
    dropped,truncated:u64,
    installed:bool,
    allocator:mem.Allocator,
}
@(private="package")
console_engine_level :: proc(level:log.Level)->Console_Level {
    if level>=.Error { return .Error };if level>=.Warning { return .Warn };if level>=.Info { return .Info };return .Debug
}
@(private="package")
console_engine_logger :: proc(state:rawptr,level:log.Level,text:string,options:log.Options,location:=#caller_location) {
    sink:=cast(^Console_Logger)state
    sync.mutex_lock(&sink.queue_mutex)
    if sink.count==len(sink.records) {
        sink.first=(sink.first+1)%len(sink.records);sink.count-=1
        if sink.dropped!=max(u64) { sink.dropped+=1 }
    }
    record:=&sink.records[(sink.first+sink.count)%len(sink.records)]
    cut:=min(len(text),CONSOLE_LOG_BYTES)
    for cut>0 && cut<len(text) && text[cut]&0xc0==0x80 { cut-=1 }
    record.level=console_engine_level(level);record.length=cut;copy(record.bytes[:cut],transmute([]byte)text[:cut]);sink.count+=1
    if cut<len(text) && sink.truncated!=max(u64) { sink.truncated+=1 }
    sync.mutex_unlock(&sink.queue_mutex)
    previous:=sink.previous
    if previous.procedure!=nil && previous.procedure!=log.nil_logger_proc && level>=previous.lowest_level {
        sync.mutex_lock(&sink.terminal_mutex)
        previous.procedure(previous.data,level,text,options,location)
        sync.mutex_unlock(&sink.terminal_mutex)
    }
}
/// Captures the exact prior logger and returns an allocation-free producer callback.
/// The owner assigns the returned logger to context.logger after successful installation.
console_logger_install :: proc(sink:^Console_Logger,previous:log.Logger,allocator:=context.allocator)->(log.Logger,Console_Logger_Error) {
    if sink.installed { return previous,.Already_Installed }
    sink.previous=previous;sink.allocator=allocator;sink.records=make([]Console_Log_Record,CONSOLE_LOG_RECORDS,allocator);sink.installed=true
    return {console_engine_logger,sink,.Debug,sink.previous.options},.None
}
/// Workers explicitly install this handle in their own context; Odin threads do not inherit it.
console_logger_handle :: proc(sink:^Console_Logger)->(log.Logger,Console_Logger_Error) {
    if !sink.installed { return {},.Not_Installed }
    return {console_engine_logger,sink,.Debug,sink.previous.options},.None
}
/// Drains at most one fixed queue capacity and allocates display history only on its owner thread.
/// Script, animation and action rows remain exclusively owned by their existing manual drains.
console_logger_drain :: proc(sink:^Console_Logger,console:^Console_State)->int {
    if !sink.installed { return 0 }
    sync.mutex_lock(&sink.queue_mutex)
    dropped,truncated:=sink.dropped,sink.truncated;sink.dropped=0;sink.truncated=0
    sync.mutex_unlock(&sink.queue_mutex)
    if dropped>0 || truncated>0 {
        message:=fmt.aprintf("Engine log buffer: retired %d older records; truncated %d messages at %d bytes",dropped,truncated,CONSOLE_LOG_BYTES,allocator=console.allocator)
        console_append(console,.Warn,message);delete(message,console.allocator)
    }
    drained:=0
    for _ in 0..<CONSOLE_LOG_RECORDS {
        record:Console_Log_Record
        sync.mutex_lock(&sink.queue_mutex)
        if sink.count==0 { sync.mutex_unlock(&sink.queue_mutex);break }
        record=sink.records[sink.first];sink.first=(sink.first+1)%len(sink.records);sink.count-=1
        sync.mutex_unlock(&sink.queue_mutex)
        console_append(console,record.level,string(record.bytes[:record.length]));drained+=1
    }
    return drained
}
/// Call only after all producer workers have joined, then assign the returned prior logger.
/// The previous logger remains caller-owned; rejected teardown returns the current logger unchanged.
console_logger_destroy :: proc(sink:^Console_Logger,current:log.Logger)->(log.Logger,Console_Logger_Error) {
    if !sink.installed { return current,.Not_Installed }
    if current.procedure!=console_engine_logger || current.data!=sink { return current,.Wrong_Context }
    previous:=sink.previous;delete(sink.records,sink.allocator);sink^={};return previous,.None
}
