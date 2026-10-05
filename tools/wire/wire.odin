#+feature dynamic-literals
//! Bounded JSONL process transport for Katla's development and acceptance tools.
package wire
import "core:os"
import "core:time"
import "core:fmt"
import "core:strings"
import "core:encoding/json"

Value :: json.Value
Object :: json.Object
Array :: json.Array
require :: proc(ok:bool,message:string) { if !ok { fmt.eprintln("Katla tool:",message); os.exit(1) } }
parse :: proc(text:string)->Value { value:Value; require(json.unmarshal(transmute([]byte)text,&value,spec=.JSON)==nil,"Invalid JSON"); return value }
encode :: proc(value:any)->string { data,error:=json.marshal(value,{spec=.JSON,use_enum_names=true,sort_maps_by_key=true}); require(error==nil,"Cannot encode JSON"); return string(data) }
object :: proc(value:Value)->Object { result,ok:=value.(Object); require(ok,"Expected JSON object"); return result }
array :: proc(value:Value)->Array { result,ok:=value.(Array); require(ok,"Expected JSON array"); return result }
text :: proc(value:Value)->string { result,ok:=value.(string); require(ok,"Expected JSON string"); return result }
number :: proc(value:Value)->f64 { if integer,ok:=value.(i64); ok { return f64(integer) }; result,ok:=value.(f64); require(ok,"Expected JSON number"); return result }
get :: proc(value:Value,key:string)->Value { return object(value)[key] }
s :: proc(value:Value,key:string)->string { return text(get(value,key)) }
b :: proc(value:Value,key:string)->bool { result,_:=get(value,key).(bool); return result }
a :: proc(value:Value,key:string)->Array { return array(get(value,key)) }
read :: proc(path:string)->string { bytes,error:=os.read_entire_file(path,context.allocator); require(error==nil,"Cannot read file"); return string(bytes) }
write :: proc(path,text:string) { require(os.write_entire_file(path,text)==nil,"Cannot write file") }

Child :: struct { process:os.Process, input,output:^os.File, buffer:[dynamic]byte, open:bool }
start :: proc(command:[]string)->Child {
    input_r,input_w,input_error:=os.pipe(); require(input_error==nil,"Cannot create input pipe")
    output_r,output_w,output_error:=os.pipe(); require(output_error==nil,"Cannot create output pipe")
    defer os.close(input_r); defer os.close(output_w)
    process,error:=os.process_start({command=command,stdin=input_r,stdout=output_w,stderr=os.stderr})
    require(error==nil,"Cannot start child"); return Child{process=process,input=input_w,output=output_r,open=true}
}
send_raw :: proc(child:^Child,data:[]byte) { n,error:=os.write(child.input,data); require(error==nil && n==len(data),"Child input failed") }
send :: proc(child:^Child,value:any) { data:=strings.concatenate({encode(value),"\n"}); send_raw(child,transmute([]byte)data) }
line :: proc(child:^Child,timeout:=20*time.Second)->string {
    started:=time.tick_now()
    for {
        for c,i in child.buffer { if c=='\n' { result:=strings.clone(string(child.buffer[:i])); copy(child.buffer[:],child.buffer[i+1:]); resize(&child.buffer,len(child.buffer)-i-1); return result } }
        require(time.tick_since(started)<timeout,"JSONL reply deadline exceeded")
        ready,error:=os.pipe_has_data(child.output); require(error==nil,"Child output unavailable")
        if !ready { time.sleep(time.Millisecond); continue }
        bytes:[65536]byte; n,read_error:=os.read(child.output,bytes[:]); require(read_error==nil && n>0,"Child closed before reply")
        require(len(child.buffer)+n<=32<<20,"JSONL reply exceeds 32 MiB"); append(&child.buffer,..bytes[:n])
    }
}
reply :: proc(child:^Child)->Value { return parse(line(child)) }
close_input :: proc(child:^Child) { if child.input!=nil { os.close(child.input); child.input=nil } }
finish :: proc(child:^Child,expected:=0) {
    close_input(child); state,error:=os.process_wait(child.process,5*time.Second)
    if error!=nil { _=os.process_kill(child.process); _,_=os.process_wait(child.process,os.TIMEOUT_INFINITE) }
    child.open=false; require(error==nil && state.exited && (int(state.exit_code)==expected || (expected<0 && state.exit_code!=0)),fmt.aprintf("Child exit status mismatch: actual %v, expected %d, wait %v",state,expected,error)); if child.output!=nil { os.close(child.output) }; child.output=nil; delete(child.buffer)
}
abort :: proc(child:^Child) { if child.open { close_input(child); _=os.process_kill(child.process); _,_=os.process_wait(child.process,os.TIMEOUT_INFINITE); if child.output!=nil { os.close(child.output) }; delete(child.buffer); child^={} } }
Client :: struct { child:Child,sequence:u64 }
client :: proc(command:[]string)->Client { return Client{child=start(command)} }
rpc :: proc(client:^Client,method:string,parameters:Object)->Value {
    parameters:=parameters
    client.sequence+=1; id:=fmt.aprintf("%d",9007199254740992+client.sequence)
    parameters["_meta"]=Object{"io.modelcontextprotocol/protocolVersion"="2026-07-28","io.modelcontextprotocol/clientCapabilities"=Object{}}
    send(&client.child,Object{"jsonrpc"="2.0","id"=id,"method"=method,"params"=parameters})
    for { message:=reply(&client.child); if get(message,"method")!=nil || is_null(get(message,"id")) || s(message,"id")!=id { continue }; require(get(message,"error")==nil,"RPC rejected"); return get(message,"result") }
}
tool :: proc(client:^Client,name:string,args:Object,allow_error:=false)->Value {
    result:=rpc(client,"tools/call",Object{"name"=name,"arguments"=args}); require(allow_error || !b(result,"isError"),strings.concatenate({"Tool failed: ",name," ",encode(result)})); return result
}
content :: proc(client:^Client,name:string,args:Object)->Value { return get(tool(client,name,args),"structuredContent") }
data :: proc(client:^Client,name:string,args:Object)->Value { return get(content(client,name,args),"data") }

is_null :: proc(value:Value)->bool { _,null:=value.(json.Null); return value==nil || null }
