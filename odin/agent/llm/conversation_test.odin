package llm

import editor "../../editor"
import "core:testing"
import "core:encoding/json"
import "core:mem"
import "core:strings"
import "core:sync"
import "core:thread"

TEST_SCHEMAS :: `[{"name":"query_entities","description":"Read entities","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}]`

@(test)
test_schema_restriction_precedes_mailbox_admission :: proc(t:^testing.T) {
    config:=config_default(); defer config_destroy(&config)
    transport:Runtime; testing.expect(t,runtime_init(&transport,config)==.None); defer runtime_destroy(&transport)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox); defer editor.agent_harness_destroy(&mailbox)
    conversation:Conversation; testing.expect(t,conversation_init(&conversation,&transport,&config,&mailbox,TEST_SCHEMAS,"Read-only context")==.None)
    defer conversation_destroy(&conversation)
    args:=make([dynamic]byte); append(&args,'{','}'); defer delete(args)
    testing.expect(t,execute_tool(&conversation,Call{id="unadvertised",name="spawn_entity",arguments=args},nil)==.None)
    testing.expect(t,mailbox.outstanding==0 && len(mailbox.requests)==0 && len(mailbox.session.actions)==0)
    last,valid:=parse_json(conversation.history[len(conversation.history)-1],context.allocator); testing.expect(t,valid); defer json.destroy_value(last)
    output:=last.(json.Object)["output"].(string)
    testing.expect(t,output==`{"error":"Tool_Not_Allowed"}`)
}
@(test)
test_conversation_serialization_preserves_tool_history_and_supplied_model :: proc(t:^testing.T) {
    for api in ([2]API{.Responses,.Chat_Completions}) {
        config:=config_default(); defer config_destroy(&config); config.api=api
        delete(config.model,config.allocator); config.model=strings.clone("chosen-model"); config.has_temperature=false
        transport:Runtime; testing.expect(t,runtime_init(&transport,config)==.None); defer runtime_destroy(&transport)
        mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox); defer editor.agent_harness_destroy(&mailbox)
        conversation:Conversation; testing.expect(t,conversation_init(&conversation,&transport,&config,&mailbox,TEST_SCHEMAS,"System 🦊")==.None); defer conversation_destroy(&conversation)
        delete(config.model,config.allocator); config.model=strings.clone("changed-after-admission"); config.has_temperature=true
        response:=Response{text=make([dynamic]byte),calls=make([dynamic]Call),allocator=context.allocator}
        args:=make([dynamic]byte); append(&args,'{','}')
        append(&response.calls,Call{id=strings.clone("exact-call-id"),name=strings.clone("query_entities"),arguments=args})
        testing.expect(t,record_assistant(&conversation,&response)==.None); response_destroy(&response)
        testing.expect(t,record_tool(&conversation,"exact-call-id",`{"entities":["18446744073709551615"]}`)==.None)
        request,err:=conversation_request(&conversation); testing.expect(t,err==.None); defer delete(request)
        tree,valid:=parse_json(request,context.allocator); testing.expect(t,valid); defer json.destroy_value(tree)
        object:=tree.(json.Object); testing.expect(t,object["model"].(string)=="chosen-model")
        testing.expect(t,"temperature" not_in object)
        field:="input" if api==.Responses else "messages"
        history:=object[field].(json.Array); last:=history[len(history)-1].(json.Object)
        id_field:="call_id" if api==.Responses else "tool_call_id"
        content_field:="output" if api==.Responses else "content"
        testing.expect(t,last[id_field].(string)=="exact-call-id" && last[content_field].(string)==`{"entities":["18446744073709551615"]}`)
    }
}
@(test)
test_reused_provider_call_ids_and_busy_conversation_fail_explicitly :: proc(t:^testing.T) {
    config:=config_default(); defer config_destroy(&config)
    transport:Runtime; testing.expect(t,runtime_init(&transport,config)==.None); defer runtime_destroy(&transport)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox); defer editor.agent_harness_destroy(&mailbox)
    conversation:Conversation; testing.expect(t,conversation_init(&conversation,&transport,&config,&mailbox,TEST_SCHEMAS,"System")==.None); defer conversation_destroy(&conversation)
    sync.mutex_lock(&conversation.mutex)
    response,error:=conversation_turn(&conversation,"Prompt"); testing.expect(t,error==.Busy); response_destroy(&response)
    sync.mutex_unlock(&conversation.mutex)
    args:=make([dynamic]byte); append(&args,'{','}')
    reply:=Response{calls=make([dynamic]Call),allocator=context.allocator}; defer response_destroy(&reply)
    append(&reply.calls,Call{id=strings.clone("reused"),name=strings.clone("query_entities"),arguments=args})
    testing.expect(t,record_assistant(&conversation,&reply)==.None)
    messages:=len(conversation.history)
    testing.expect(t,record_assistant(&conversation,&reply)==.Protocol && conversation.failed && len(conversation.history)==messages && mailbox.outstanding==0)
}
@(test)
test_joined_async_job_has_one_terminal_transfer_and_no_disabled_fallback :: proc(t:^testing.T) {
    config:=config_default(); defer config_destroy(&config)
    transport:Runtime; testing.expect(t,runtime_init(&transport,config)==.None); defer runtime_destroy(&transport)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox); defer editor.agent_harness_destroy(&mailbox)
    conversation:Conversation; testing.expect(t,conversation_init(&conversation,&transport,&config,&mailbox,TEST_SCHEMAS,"System")==.None); defer conversation_destroy(&conversation)
    job:Job; testing.expect(t,job_start(&job,&conversation,"Prompt")==.None)
    response:Response; error:Error; done:bool
    for !done { response,error,done=job_poll(&job); thread.yield() }
    testing.expect(t,error==.Disabled && mailbox.outstanding==0)
    repeated,repeated_error,available:=job_poll(&job); testing.expect(t,!available && repeated_error==.None); response_destroy(&repeated)
    job_destroy(&job); response_destroy(&response)
}
@(test)
test_stream_progress_backpressure_cancels_without_dropping_into_success :: proc(t:^testing.T) {
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    allocator:=mem.tracking_allocator(&tracker)
    job:=Job{capacity=1,chunks=make([dynamic]string,allocator),allocator=allocator}
    job_text(&job,"first"); job_text(&job,"second")
    testing.expect(t,job.overflow && is_cancelled(&job.cancellation) && len(job.chunks)==1)
    chunk,available:=job_poll_text(&job); testing.expect(t,available && chunk=="first"); delete(chunk,allocator)
    job_destroy(&job)
    testing.expect(t,len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker)
}
@(test)
test_strict_provider_json_rejects_dropped_keys_and_numeric_normalization :: proc(t:^testing.T) {
    for text in ([]string{`{"":1}`,`{"a":1,"a":2}`,`{"a":01}`,`{"a":01.5}`,`{"a":1e9999}`,`{"a":9223372036854775808}`,`{"a":18446744073709551617}`,`{"a":36893488147419103233}`,`{"a":-18446744073709551617}`,`{"a":1} {}`,`{"a":NaN}`}) {
        tree,valid:=parse_json(text,context.allocator); testing.expect(t,!valid); json.destroy_value(tree)
    }
    deep:=make([dynamic]byte); defer delete(deep)
    for _ in 0..<65 { append(&deep,'[') }; append(&deep,'0'); for _ in 0..<65 { append(&deep,']') }
    tree,valid:=parse_json(string(deep[:]),context.allocator); testing.expect(t,!valid); json.destroy_value(tree)
}

@(test)
test_runtime_rejects_changed_admission_configuration_before_network :: proc(t:^testing.T) {
    config:=config_default(); defer config_destroy(&config)
    transport:Runtime; testing.expect(t,runtime_init(&transport,config)==.None); defer runtime_destroy(&transport)
    config.max_calls+=1
    response,err:=complete(&transport,config,"{}")
    testing.expect(t,err==.Config); response_destroy(&response)
}
