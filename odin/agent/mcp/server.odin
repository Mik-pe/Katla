//! MCP requests own protocol state; the synchronized mailbox alone crosses to the scene owner.
package mcp

import agent ".."
import editor "../../editor"
import "core:encoding/json"
import "core:mem"
import "core:fmt"
import "core:time"
import "core:strings"

@(private="package")
Pending :: struct { id,tool:string, ticket:u64, admitted:time.Duration, abandoned:bool }
/// Stationary, single-caller protocol state; no World or application pointer crosses here.
Server :: struct { mailbox:^editor.Agent_Harness, pending:[dynamic]Pending, timeout:time.Duration, closed:bool, allocator:mem.Allocator }
/// Initializes a ticket-selective adapter over a scene mailbox shared with other producers.
server_init :: proc(s:^Server,mailbox:^editor.Agent_Harness,allocator:=context.allocator,timeout:=15*time.Second) {
    assert(timeout>0 && mailbox!=nil)
    s^=Server{mailbox=mailbox,pending=make([dynamic]Pending,allocator),timeout=timeout,allocator=allocator}
}
/// Releases this connection and its queued or unread replies without closing other producers.
/// Join the scene owner before destroying its mailbox; already executing mutations are not undone.
server_destroy :: proc(s:^Server) {
    for pending in s.pending { editor.agent_abandon(s.mailbox,pending.ticket); delete(pending.id,s.allocator); delete(pending.tool,s.allocator) }
    delete(s.pending); s^={}
}
/// Closes admission while preserving accepted work for normal result draining.
server_finish :: proc(s:^Server) { s.closed=true }
@(private="package")
remove_pending :: proc(s:^Server,index:int) { delete(s.pending[index].id,s.allocator); delete(s.pending[index].tool,s.allocator); ordered_remove(&s.pending,index) }
@(private="package")
tool_error :: proc(id,message:string,allocator:mem.Allocator)->string {
    text,err:=json.marshal(message,allocator=allocator); assert(err==nil); defer delete(text,allocator)
    failure,encode_error:=json.marshal(struct{success:bool,message:string}{false,message},allocator=allocator); assert(encode_error==nil); defer delete(failure,allocator)
    result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s,"isError":true,"structuredContent":%s,"content":[{{"type":"text","text":%s}}]}}`,SERVER_META,string(failure),string(text),allocator=allocator)
    defer delete(result,allocator)
    return rpc_result(id,result,allocator)
}
@(private="package")
cancel_request :: proc(s:^Server,params:json.Object) {
    id,valid:=request_id(params["requestId"],s.allocator)
    if !valid { return }; defer delete(id,s.allocator)
    for &pending,i in s.pending {
        if pending.id!=id { continue }
        editor.agent_abandon(s.mailbox,pending.ticket); remove_pending(s,i)
        return
    }
}
/// Handles one complete frame; returns an owned immediate response or an empty string.
/// Pass monotonic elapsed time; asynchronous tool responses arrive through server_poll.
server_receive :: proc(s:^Server,line:string,now:time.Duration)->string {
    context.allocator=s.allocator
    tree,valid:=parse_message(line,s.allocator)
    if !valid { return rpc_error("",-32700,"Invalid JSON",allocator=s.allocator) }
    defer json.destroy_value(tree)
    object,object_ok:=tree.(json.Object)
    if !object_ok { return rpc_error("",-32600,"Expected one request object",allocator=s.allocator) }
    _,has_id:=object["id"]
    id:=""
    if has_id {
        id,valid=request_id(object["id"],s.allocator)
        if !valid { return rpc_error("",-32600,"Request ID must be a string or signed 64-bit integer",allocator=s.allocator) }
    }
    defer delete(id,s.allocator)
    version,version_ok:=object["jsonrpc"].(string)
    method,method_ok:=object["method"].(string)
    if !version_ok || version!="2.0" || !method_ok || method=="" || !allowed_fields(object,{"jsonrpc","id","method","params"}) {
        return rpc_error(id,-32600,"Invalid JSON-RPC request",allocator=s.allocator)
    }
    params,params_ok:=object["params"].(json.Object)
    if !has_id {
        if method=="notifications/cancelled" && params_ok { cancel_request(s,params) }
        return ""
    }
    for pending in s.pending {
        if pending.id==id { return rpc_error("",-32600,"Request ID is already in flight",allocator=s.allocator) }
    }
    if now<0 { return rpc_error(id,-32603,"Invalid monotonic clock",allocator=s.allocator) }
    if !params_ok { return rpc_error(id,-32602,"Request params must include protocol metadata",allocator=s.allocator) }
    meta,meta_ok:=params["_meta"].(json.Object)
    protocol,protocol_ok:=meta["io.modelcontextprotocol/protocolVersion"].(string)
    _,capabilities_ok:=meta["io.modelcontextprotocol/clientCapabilities"].(json.Object)
    if !meta_ok || !protocol_ok || !capabilities_ok {
        return rpc_error(id,-32602,"Protocol version and client capabilities are required on every request",allocator=s.allocator)
    }
    if info,present:=meta["io.modelcontextprotocol/clientInfo"]; present {
        client_info,info_ok:=info.(json.Object)
        _,info_name_ok:=client_info["name"].(string); _,info_version_ok:=client_info["version"].(string)
        if !info_ok || !info_name_ok || !info_version_ok { return rpc_error(id,-32602,"Client info requires name and version strings",allocator=s.allocator) }
    }
    if protocol!=PROTOCOL_VERSION {
        data,err:=json.marshal(struct { supported:[]string, requested:string }{{PROTOCOL_VERSION},protocol}); assert(err==nil); defer delete(data)
        return rpc_error(id,-32022,"Unsupported protocol version",string(data),s.allocator)
    }
    if s.closed { return rpc_error(id,1003,"Scene connection is closed",allocator=s.allocator) }
    switch method {
    case "server/discover":
        if !allowed_fields(params,{"_meta"}) { return rpc_error(id,-32602,"Invalid discovery params",allocator=s.allocator) }
        result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s,"supportedVersions":["%s"],"capabilities":{{"tools":{{"listChanged":false}}}},"instructions":"Scene authoring, materials, assets and gameplay on the existing application owner."}}`,SERVER_META,PROTOCOL_VERSION)
        defer delete(result)
        return rpc_result(id,result,s.allocator)
    case "ping":
        if !allowed_fields(params,{"_meta"}) { return rpc_error(id,-32602,"Invalid ping params",allocator=s.allocator) }
        result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s}}`,SERVER_META); defer delete(result)
        return rpc_result(id,result,s.allocator)
    case "tools/list":
        if !allowed_fields(params,{"_meta","cursor"}) { return rpc_error(id,-32602,"Invalid tool list params",allocator=s.allocator) }
        if _,present:=params["cursor"]; present { return rpc_error(id,-32602,"No pagination cursor is available",allocator=s.allocator) }
        result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s,"tools":%s}}`,SERVER_META,strings.trim_space(TOOLS_JSON)); defer delete(result)
        return rpc_result(id,result,s.allocator)
    case "tools/call":
        if !allowed_fields(params,{"_meta","name","arguments"}) { return rpc_error(id,-32602,"Invalid tool call params",allocator=s.allocator) }
        name,name_ok:=params["name"].(string)
        if !name_ok || name=="" { return rpc_error(id,-32602,"Tool name is required",allocator=s.allocator) }
        arguments,has_arguments:=params["arguments"]
        if !has_arguments { arguments=json.Object{} }
        if _,ok:=arguments.(json.Object); !ok { return tool_error(id,"Tool arguments must be an object",s.allocator) }
        bytes,err:=json.marshal(arguments); assert(err==nil); defer delete(bytes)
        ticket,call_error:=agent.submit_call(s.mailbox,{id,name,bytes})
        switch call_error {
        case .None:
            append(&s.pending,Pending{id=id,tool=strings.clone(name,s.allocator),ticket=ticket,admitted=now}); id=""
            return ""
        case .Unknown_Tool: return rpc_error(id,-32602,"Unknown tool",allocator=s.allocator)
        case .Invalid_Arguments: return tool_error(id,"Invalid tool arguments",s.allocator)
        case .Invalid_JSON: return rpc_error(id,-32603,"Could not decode tool arguments",allocator=s.allocator)
        case .Mailbox_Full: return rpc_error(id,1002,"Scene mailbox is full",allocator=s.allocator)
        case .Mailbox_Closed: return rpc_error(id,1003,"Scene connection is closed",allocator=s.allocator)
        case .Identifier_Exhausted: return rpc_error(id,1005,"Scene request IDs are exhausted",allocator=s.allocator)
        }
    }
    return rpc_error(id,-32601,"Method not found",allocator=s.allocator)
}
/// Drains a ready result or expires one stopped-owner request; the returned JSON is owned.
/// Cancelled/expired accepted mutations are never retried or silently rolled back.
server_poll :: proc(s:^Server,now:time.Duration)->string {
    context.allocator=s.allocator
    for index:=0;index<len(s.pending); {
        pending:=s.pending[index]
        response,ready:=editor.agent_take_result_for(s.mailbox,pending.ticket)
        if !ready { index+=1; continue }
        defer editor.agent_response_destroy(&response)
        ordered_remove(&s.pending,index)
        defer { delete(pending.id,s.allocator); delete(pending.tool,s.allocator) }
        if pending.abandoned { continue }
        if response.result.error!=.None {
            message:=""
            if pending.tool=="editor_view" { message=view_error_message(response.result.data,s.allocator) }
            if message=="" { message=fmt.aprintf("Scene tool failed: %v",response.result.error) }; defer delete(message)
            return tool_error(pending.id,message,s.allocator)
        }
        if pending.tool=="editor_view" { return view_result(pending.id,response.result.data,s.allocator) }
        ids:=make([]string,len(response.result.entities)); defer { for id in ids { delete(id) }; delete(ids) }
        for entity,i in response.result.entities { ids[i]=fmt.aprintf("%d",u64(entity)) }
        entities,err:=json.marshal(ids); assert(err==nil); defer delete(entities)
        data:="null"
        if len(response.result.data)>0 { data=string(response.result.data) }
        payload:=fmt.aprintf(`{{"entity_ids":%s,"data":%s}}`,string(entities),data); defer delete(payload)
        text,marshal_error:=json.marshal(payload); assert(marshal_error==nil); defer delete(text)
        result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s,"isError":false,"structuredContent":%s,"content":[{{"type":"text","text":%s}}]}}`,SERVER_META,payload,string(text)); defer delete(result)
        return rpc_result(pending.id,result,s.allocator)
    }
    for &pending,i in s.pending {
        if pending.abandoned || now<pending.admitted || now-pending.admitted<s.timeout { continue }
        output:=rpc_error(pending.id,1004,"Scene owner did not complete the request before its deadline",allocator=s.allocator)
        editor.agent_abandon(s.mailbox,pending.ticket); remove_pending(s,i)
        return output
    }
    return ""
}
