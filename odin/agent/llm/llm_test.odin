package llm

import "core:testing"
import "core:mem"
import "core:strings"
import "core:os"
import "core:fmt"
import "core:encoding/json"

@(test)
test_config_explicit_provider_and_flat_toml :: proc(t:^testing.T) {
    config,err:=config_parse(`provider = "open_ai_compatible"
api_key = '$KATLA_TEST_KEY'
base_url = "http://127.0.0.1:1234/v1" # endpoint
model = "model-selected-by-user"
max_tokens = 128
temperature = 0.25
rate_limit_min_interval_ms = 0
rate_limit_max_calls_per_minute = 3
`)
    testing.expect(t,err==.None); defer config_destroy(&config)
    testing.expect(t,config.provider==.OpenAI_Compatible && config.api==.Chat_Completions && config.model=="model-selected-by-user" && config.max_tokens==128 && config.max_calls==3)
    for input in ([]string{
        `provider = "substitute"`, `max_tokens = 0`, `max_tokens = 4294967296`, `max_tokens = 18446744073709551617`, `rate_limit_max_calls_per_minute = 36893488147419103233`, `max_tokens = 0d10`, `max_tokens = 0z10`, `max_tokens = 1__0`, `max_tokens = _1`, `max_tokens = 1_`, `max_tokens = +0x10`, `temperature = 0__1`, `temperature = 0x1p0`, `temperature = nan`, `temperature = 2.01`, `model = "a"`+"\n"+`model = "b"`, `unknown = 1`,
        `provider = "open_ai_compatible"`+"\n"+`api_key = "x"`+"\n"+`base_url = "http://key@host/v1"`,
        `provider = "open_ai"`+"\n"+`api_key = "x"`+"\n"+`api = "chat_completions"`,
    }) {
        rejected,result:=config_parse(input); testing.expect(t,result==.Config); config_destroy(&rejected)
    }
}
@(test)
test_config_atomic_storage_and_owner_permissions :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-llm-test-*",context.allocator)
    testing.expect(t,dir_error==nil); defer { os.remove(directory); delete(directory) }
    path:=strings.concatenate({directory,"/llm.toml"}); defer delete(path); defer os.remove(path)
    config,error:=config_parse("provider = \"open_ai\"\napi_key = \"$KATLA_TEST_MISSING_VARIABLE\"\nmodel = \"supplied-model\"\n")
    testing.expect(t,error==.None); defer config_destroy(&config)
    testing.expect(t,config_save(config,path)==.None)
    loaded,load_error:=config_load(path); testing.expect(t,load_error==.None); defer config_destroy(&loaded)
    testing.expect(t,loaded.api==.Responses && loaded.api_key=="$KATLA_TEST_MISSING_VARIABLE" && loaded.model=="supplied-model")
    key,key_error:=config_resolve_key(loaded); testing.expect(t,key_error==.Credentials && key==""); delete(key,loaded.allocator)
    info,stat_error:=os.stat(path,context.allocator); testing.expect(t,stat_error==nil)
    when ODIN_OS!=.Windows { testing.expect(t,info.mode=={.Read_User,.Write_User}) }
    os.file_info_delete(info,context.allocator)
    missing_path:=strings.concatenate({directory,"/missing.toml"}); defer delete(missing_path)
    absent,absent_error:=config_load(missing_path,missing_disabled=true)
    testing.expect(t,absent_error==.None && absent.provider==.Disabled); config_destroy(&absent)
    loaded.temperature=3; testing.expect(t,config_save(loaded,path)==.Config)
    intact,intact_error:=config_load(path); testing.expect(t,intact_error==.None && intact.temperature==config.temperature); config_destroy(&intact)
}
@(private="package")
feed_bytewise :: proc(s:^Stream,text:string)->Error {
    for ch in transmute([]byte)text { value:=[1]byte{ch}; if err:=stream_feed(s,value[:]); err!=.None { return err } }
    return .None
}
@(test)
test_chat_fragmented_utf8_and_tool_arguments :: proc(t:^testing.T) {
    stream:Stream; stream_init(&stream,.Chat_Completions); defer stream_destroy(&stream)
    data:=": keepalive\r\n\r\n"+
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hej 🦊\",\"tool_calls\":[{\"index\":0,\"id\":\"call-exact\",\"type\":\"function\",\"function\":{\"name\":\"spawn_entity\",\"arguments\":\"{\\\"name\\\":\"}}]},\"finish_reason\":null}]}\r\n\r\n"+
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"fox\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n"+
        "data: [DONE]\n\n"
    testing.expect(t,feed_bytewise(&stream,data)==.None)
    response,err:=stream_finish(&stream); testing.expect(t,err==.None); defer response_destroy(&response)
    testing.expect(t,string(response.text[:])=="Hej 🦊" && len(response.calls)==1 && response.calls[0].id=="call-exact" && string(response.calls[0].arguments[:])==`{"name":"fox"}`)
}
@(test)
test_stream_rejects_truncation_malformed_and_bounds :: proc(t:^testing.T) {
    inputs:=[]string{
        "data: [DONE]\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[{\"index\":1,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":64}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"id\",\"function\":{\"name\":\"spawn_entity\",\"arguments\":\"not json\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n",
    }
    for input in inputs {
        s:Stream; stream_init(&s,.Chat_Completions); defer stream_destroy(&s)
        feed_error:=stream_feed(&s,transmute([]byte)input)
        response,err:=stream_finish(&s); response_destroy(&response)
        testing.expect(t,feed_error!=.None || err!=.None)
    }
    s:Stream; stream_init(&s,.Chat_Completions); defer stream_destroy(&s)
    line:=make([]byte,MAX_EVENT_BYTES+1); defer delete(line)
    testing.expect(t,stream_feed(&s,line)==.Limit && len(s.line)<=MAX_EVENT_BYTES)
}
@(test)
test_responses_stream_correlates_completed_function_call :: proc(t:^testing.T) {
    s:Stream; stream_init(&s,.Responses); defer stream_destroy(&s)
    data:=`data: {"type":"response.output_text.delta","delta":"Ready"}`+"\n\n"+
        `data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Ready"}]},{"type":"function_call","call_id":"call-exact","name":"query_entities","arguments":"{}"}]}}`+"\n\n"
    testing.expect(t,feed_bytewise(&s,data)==.None)
    response,err:=stream_finish(&s); testing.expect(t,err==.None && string(response.text[:])=="Ready" && len(response.calls)==1 && response.calls[0].id=="call-exact"); response_destroy(&response)
}
@(test)
test_stream_tracking_allocator_owns_failure_and_transfer :: proc(t:^testing.T) {
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    allocator:=mem.tracking_allocator(&tracker)
    s:Stream; stream_init(&s,.Chat_Completions,allocator=allocator)
    testing.expect(t,stream_feed(&s,transmute([]byte)string("data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"text\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"))==.None)
    response,error:=stream_finish(&s); testing.expect(t,error==.None); stream_destroy(&s); response_destroy(&response)
    testing.expect(t,len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker)
}

@(test)
test_configuration_diagnostics_never_format_credentials :: proc(t:^testing.T) {
    config:=config_default(); defer config_destroy(&config)
    config.api_key=strings.clone("diagnostic-test-secret")
    for format in ([4]string{"%v","%+v","%#v","%w"}) {
        text:=fmt.aprintf(format,config); testing.expect(t,!strings.contains(text,config.api_key)); delete(text)
    }
    serialized,err:=json.marshal(config); testing.expect(t,err==nil && !strings.contains(string(serialized),config.api_key)); delete(serialized)
}

@(test)
test_toml_numeric_forms_are_checked_without_wrapping :: proc(t:^testing.T) {
    for input in ([4]string{"max_tokens = +1_024","max_tokens = 0x400","max_tokens = 0o2000","max_tokens = 0b10000000000"}) {
        config,err:=config_parse(input); testing.expect(t,err==.None && config.max_tokens==1024); config_destroy(&config)
    }
    config,err:=config_parse("temperature = +1_0e-1"); testing.expect(t,err==.None && config.temperature==1); config_destroy(&config)
}
