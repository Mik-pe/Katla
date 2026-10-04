#+test
package editor_app

import "core:log"
import "core:testing"
import "core:strings"
import "core:unicode/utf8"
import "core:thread"
import "core:sync"
import "base:runtime"

@(private="file")
Console_Log_Spy :: struct { count:int,last_level:log.Level,last_options:log.Options,last_line:i32,last_length:int,last_text:[128]byte }
@(private="file")
console_log_spy :: proc(state:rawptr,level:log.Level,text:string,options:log.Options,location:=#caller_location) {
    spy:=cast(^Console_Log_Spy)state;spy.count+=1;spy.last_level=level;spy.last_options=options;spy.last_line=location.line
    spy.last_length=min(len(text),len(spy.last_text));copy(spy.last_text[:spy.last_length],transmute([]byte)text[:spy.last_length])
}
@(test)
test_console_logger_forwards_prior_threshold_location_and_restores_exact_context :: proc(t:^testing.T) {
    original:=context.logger;defer { context.logger=original }
    spy:Console_Log_Spy;prior:=log.Logger{console_log_spy,&spy,.Info,{.Line,.Procedure}}
    context.logger=prior
    sink:Console_Logger;next,install_error:=console_logger_install(&sink,context.logger);testing.expect_value(t,install_error,Console_Logger_Error.None);context.logger=next
    console:Console_State;console_init(&console,context.allocator);defer console_destroy(&console)
    log.debug("engine debug");log.warn("faktisk shader-varning åäö")
    testing.expect(t,spy.count==1 && spy.last_level==.Warning && spy.last_options==prior.options && spy.last_line>0 && string(spy.last_text[:spy.last_length])=="faktisk shader-varning åäö")
    testing.expect_value(t,console_logger_drain(&sink,&console),2)
    testing.expect(t,len(console.rows)==2 && console.rows[0].level==.Debug && console.rows[1].level==.Warn && console.rows[1].message=="faktisk shader-varning åäö")
    testing.expect_value(t,console_logger_drain(&sink,&console),0)
    console_append(&console,.Info,"manual script row");testing.expect_value(t,console_logger_drain(&sink,&console),0);testing.expect_value(t,len(console.rows),3)
    restored,destroy_error:=console_logger_destroy(&sink,context.logger);testing.expect_value(t,destroy_error,Console_Logger_Error.None);context.logger=restored;testing.expect(t,context.logger==prior)
    log.error("after restoration");testing.expect(t,spy.count==2 && spy.last_level==.Error)
}
@(test)
test_console_logger_bounds_old_records_and_truncates_only_utf8_boundaries :: proc(t:^testing.T) {
    original:=context.logger;defer { context.logger=original }
    spy:Console_Log_Spy;context.logger={console_log_spy,&spy,.Debug,nil}
    sink:Console_Logger;next,install_error:=console_logger_install(&sink,context.logger);assert(install_error==.None);context.logger=next;defer { restored,error:=console_logger_destroy(&sink,context.logger);assert(error==.None);context.logger=restored }
    console:Console_State;console_init(&console,context.allocator);defer console_destroy(&console)
    for i in 0..<CONSOLE_LOG_RECORDS+5 { log.infof("engine row %d",i) }
    testing.expect(t,sink.count==CONSOLE_LOG_RECORDS && sink.dropped==5 && spy.count==CONSOLE_LOG_RECORDS+5)
    testing.expect_value(t,console_logger_drain(&sink,&console),CONSOLE_LOG_RECORDS)
    testing.expect(t,console.rows[0].level==.Warn && strings.contains(console.rows[0].message,"retired 5") && console.rows[1].message=="engine row 5")
    prefix:=strings.repeat("x",CONSOLE_LOG_BYTES-1);defer delete(prefix)
    value:=strings.concatenate({prefix,"åäö"});defer delete(value)
    log.error(value);testing.expect_value(t,console_logger_drain(&sink,&console),1)
    latest:=console.rows[len(console.rows)-1]
    testing.expect(t,latest.level==.Error && len(latest.message)==CONSOLE_LOG_BYTES-1 && utf8.valid_string(latest.message))
}
@(private="file")
Console_Log_Producer :: struct { logger:log.Logger,done:^i32,index:int }
@(private="file")
console_log_producer :: proc(worker:^thread.Thread) {
    task:=cast(^Console_Log_Producer)worker.data;context.logger=task.logger
    for i in 0..<1000 { log.warnf("worker %d warning %d",task.index,i) }
    sync.atomic_add(task.done,1)
}
@(test)
test_console_logger_concurrent_workers_and_owner_drain_keep_bounded_storage :: proc(t:^testing.T) {
    original:=context.logger;defer { context.logger=original }
    spy:Console_Log_Spy;context.logger={console_log_spy,&spy,.Debug,nil}
    sink:Console_Logger;next,install_error:=console_logger_install(&sink,context.logger);assert(install_error==.None);context.logger=next
    logger,error:=console_logger_handle(&sink);assert(error==.None)
    console:Console_State;console_init(&console,context.allocator);defer console_destroy(&console)
    completed:i32;tasks:[8]Console_Log_Producer;workers:[8]^thread.Thread
    for &task,index in tasks { task={logger,&completed,index};workers[index]=thread.create(console_log_producer);assert(workers[index]!=nil);workers[index].data=&task;thread.start(workers[index]) }
    for sync.atomic_load(&completed)<8 { console_logger_drain(&sink,&console);thread.yield() }
    for worker in workers { thread.join(worker);thread.destroy(worker) }
    console_logger_drain(&sink,&console)
    testing.expect(t,spy.count==8000 && sink.count==0 && len(sink.records)==CONSOLE_LOG_RECORDS && console.bytes<=4<<20 && len(console.rows)<=4096)
    restored,destroy_error:=console_logger_destroy(&sink,context.logger);testing.expect_value(t,destroy_error,Console_Logger_Error.None);context.logger=restored
}
@(test)
test_console_logger_rejects_teardown_from_another_context_without_freeing_worker_storage :: proc(t:^testing.T) {
    original:=context.logger;defer { context.logger=original }
    context.logger={log.nil_logger_proc,nil,.Warning,{.Long_File_Path}}
    prior:=context.logger;sink:Console_Logger
    next,install_error:=console_logger_install(&sink,context.logger);testing.expect_value(t,install_error,Console_Logger_Error.None);context.logger=next
    active:=context.logger;records:=raw_data(sink.records)
    rejected,reinstall_error:=console_logger_install(&sink,context.logger);testing.expect_value(t,reinstall_error,Console_Logger_Error.Already_Installed);testing.expect(t,rejected==active)
    testing.expect(t,raw_data(sink.records)==records && context.logger==active)
    context.logger=prior
    rejected_restore,wrong_error:=console_logger_destroy(&sink,context.logger);testing.expect_value(t,wrong_error,Console_Logger_Error.Wrong_Context);testing.expect(t,rejected_restore==prior)
    testing.expect(t,sink.installed && raw_data(sink.records)==records && context.logger==prior)
    context.logger=active
    restored,destroy_error:=console_logger_destroy(&sink,context.logger);testing.expect_value(t,destroy_error,Console_Logger_Error.None);context.logger=restored
    testing.expect(t,context.logger==prior)
    _,error:=console_logger_handle(&sink);testing.expect_value(t,error,Console_Logger_Error.Not_Installed)
    _,missing_error:=console_logger_destroy(&sink,context.logger);testing.expect_value(t,missing_error,Console_Logger_Error.Not_Installed)
}
@(private="file")
Console_Log_Wrapper_Worker :: struct { logger:log.Logger,release:^i32 }
@(private="file")
console_log_wrapper_worker :: proc(worker:^thread.Thread) {
    task:=cast(^Console_Log_Wrapper_Worker)worker.data;context.logger=task.logger
    for sync.atomic_load(task.release)==0 { thread.yield() }
    log.warn("engine worker shutdown warning")
}
@(private="file")
Console_Log_Wrapper_Result :: struct { accepted,joined,captured_worker,captured_outcome:bool,records:int,restored:runtime.Context }
@(private="file")
console_log_wrapper_inner :: proc(result:^$Result,sink:^Console_Logger,fail:bool)->bool {
    logger,error:=console_logger_handle(sink);assert(error==.None)
    release:i32;task:=Console_Log_Wrapper_Worker{logger,&release};worker:=thread.create(console_log_wrapper_worker);assert(worker!=nil);worker.data=&task;thread.start(worker)
    defer { sync.atomic_store(&release,1);thread.join(worker);thread.destroy(worker);result.joined=true;log.info("owner retirement completed") }
    log.info("owner frame entered")
    if fail { log.error("engine family retained after failure");return false }
    log.warn("engine accepted warning");return true
}
@(private="file")
console_log_wrapper_owner :: proc(fail:bool)->Console_Log_Wrapper_Result {
    previous:=context.logger;sink:Console_Logger;captured,install_error:=console_logger_install(&sink,previous);assert(install_error==.None);context.logger=captured
    result:Console_Log_Wrapper_Result;result.accepted=console_log_wrapper_inner(&result,&sink,fail)
    console:Console_State;console_init(&console,context.allocator);defer console_destroy(&console)
    result.records=console_logger_drain(&sink,&console)
    for row in console.rows { if row.message=="engine worker shutdown warning" && row.level==.Warn { result.captured_worker=true };if row.message==("engine family retained after failure" if fail else "engine accepted warning") && row.level==(.Error if fail else .Warn) { result.captured_outcome=true } }
    restored,destroy_error:=console_logger_destroy(&sink,context.logger);assert(destroy_error==.None);context.logger=restored;result.restored=context
    return result
}
@(test)
test_console_logger_stationary_owner_captures_success_failure_and_joined_worker_then_restores_context :: proc(t:^testing.T) {
    original:=context.logger;defer { context.logger=original }
    spy:Console_Log_Spy;context.logger={console_log_spy,&spy,.Info,{.Thread_Id,.Procedure}}
    previous:=context
    for fail in ([2]bool{false,true}) {
        result:=console_log_wrapper_owner(fail)
        testing.expect(t,result.accepted==!fail && result.joined && result.captured_worker && result.captured_outcome && result.records==4)
        testing.expect(t,result.restored==previous && context==previous)
    }
    testing.expect_value(t,spy.count,8)
}
