#+feature dynamic-literals
//! Real Unix connection acceptance and shared-owner MCP journeys.
package katla_build
import fixtures "../unix_fixtures"
import "../wire"
import "core:os"
import "core:time"
import "core:strings"


private_directory :: proc(name:string)->string { path:=join(output,name); mkdir(path); require(os.chmod(path,{.Read_User,.Write_User,.Execute_User})==nil,"Cannot protect private fixture directory"); return path }
validation_host :: proc() {
    binary:=validation_build("odin/examples/host_connection","host-connection")
    for scenario in ([]string{"journey","eof","malformed","missing_thread","wrong_resume","cancel_wait"}) {
        endpoint:=join(private_directory("host"),"control.sock"); fixture:fixtures.Fixture; fixtures.start(&fixture,endpoint,"host",scenario)
        child:=wire.start({binary,endpoint,"journey" if scenario=="journey" else "cancel_wait" if scenario=="cancel_wait" else "disconnect"})
        _=wire.line(&child,25*time.Second); wire.finish(&child); if scenario=="journey" { require(strings.join(fixture.methods[:],",")=="initialize,initialized,thread/loaded/list,thread/loaded/list,thread/resume,thread/read,turn/start,thread/read,turn/steer,turn/interrupt","Existing host method sequence changed") }; fixtures.stop(&fixture)
    }
}
validation_proxy :: proc() {
    binary:=validation_build("odin/mcp_proxy","proxy"); endpoint:=join(private_directory("proxy-private"),"editor.sock")
    fixture:fixtures.Fixture; fixtures.start(&fixture,endpoint,"echo","")
    payload:=strings.repeat("line\x00\r\n{\"jsonrpc\":\"2.0\"}\n",140000); input:=join(output,"proxy-input"); write(input,payload)
    file,error:=os.open(input); require(error==nil,"Cannot open proxy fixture input")
    state,stdout,stderr,exec_error:=os.process_exec({command={binary,endpoint},stdin=file},context.allocator); os.close(file)
    require(exec_error==nil && state.exited && state.exit_code==0 && string(stdout)==payload && len(stderr)==0,"Proxy did not preserve >3 MiB byte identity"); delete(stdout); delete(stderr); fixtures.stop(&fixture)
    fixtures.start(&fixture,endpoint,"reply",""); child:=wire.start({binary,endpoint}); require(wire.s(wire.reply(&child),"id")=="closed","Peer EOF lost reply"); wire.finish(&child); fixtures.stop(&fixture)
    fixtures.start(&fixture,endpoint,"broken",""); child=wire.start({binary,endpoint}); os.close(child.output); child.output=nil; wire.finish(&child,1); fixtures.stop(&fixture)
    write(endpoint,"existing file"); rejected:=wire.start({binary,endpoint}); wire.finish(&rejected,1); require(string(read(endpoint))=="existing file","Proxy damaged pre-existing path"); remove(endpoint)
    if validation.slow {
        fixtures.start(&fixture,endpoint,"slow",""); child=wire.start({binary,endpoint}); started:=time.tick_now()
        slow_state,wait_error:=os.process_wait(child.process,18*time.Second); require(wait_error==nil && slow_state.exited && slow_state.exit_code==1 && time.tick_since(started)>14*time.Second,"Stalled output deadline failed")
        child.open=false; wire.close_input(&child); os.close(child.output); fixtures.stop(&fixture)
    }
}
validation_socket :: proc() {
    directory:=private_directory("shared-world"); endpoint:=join(directory,"editor.sock"); project:=join(directory,"project"); remove(project); mkdir(join(project,"resources"))
    owner:=validation_build("odin/examples/mcp_socket_authoring","mcp-world"); proxy:=validation_build("odin/mcp_proxy","mcp-proxy")
    process,error:=os.process_start({command={owner,endpoint,project},stderr=os.stderr}); require(error==nil,"Cannot start shared owner")
    owner_alive:=true; defer { if owner_alive { _=os.process_kill(process); _,_=os.process_wait(process,os.TIMEOUT_INFINITE) } }
    started:=time.tick_now(); for !os.exists(endpoint) { require(time.tick_since(started)<5*time.Second,"Shared owner socket deadline"); time.sleep(time.Millisecond) }
    one:=wire.client({proxy,endpoint}); defer wire.abort(&one.child)
    two:=wire.client({proxy,endpoint}); defer wire.abort(&two.child)
    entity:=wire.text(wire.a(wire.content(&one,"spawn_entity",wire.Object{"name"="ÅNGSTRÖM shared sphere","shape"="sphere","position"=wire.Array{i64(1),i64(2),i64(3)}}),"entity_ids")[0])
    duplicate:=wire.text(wire.a(wire.content(&one,"duplicate_entity",wire.Object{"entity_id"=entity,"position_offset"=wire.Array{i64(2),i64(0),i64(0)}}),"entity_ids")[0]); require(duplicate!=entity,"Duplicate reused identity")
    require(len(wire.a(wire.content(&two,"query_entities",wire.Object{}),"entity_ids"))==2,"Second process does not share world")
    _=wire.content(&one,"create_resource",wire.Object{"path"="resources/shared.txt","content"="first content"}); _=wire.content(&two,"write_resource",wire.Object{"path"="resources/shared.txt","content"="replacement content"})
    require(string(read(join(project,"resources/shared.txt")))=="replacement content","Shared resource mutation missing")
    wire.finish(&one.child); require(len(wire.a(wire.content(&two,"query_entities",wire.Object{}),"entity_ids"))==2,"Disconnect destroyed shared world"); wire.finish(&two.child)
    state,wait_error:=os.process_wait(process,5*time.Second); require(wait_error==nil && state.exited && state.exit_code==0 && !os.exists(endpoint),"Owner cleanup failed")
    owner_alive=false
}
