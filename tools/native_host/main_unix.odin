#+build darwin, linux
#+feature dynamic-literals
//! Explicit existing-thread API fixture validates paired viewport input and preserves identity.
package main
import socket "../../odin/agent/socket"
import image "../../odin/image"
import "../wire"
import fixtures "../unix_fixtures"
import "core:os"
import "core:strings"
import "core:fmt"
import "core:time"
import "core:encoding/base64"
import "core:crypto/sha2"
import "core:encoding/hex"
import "core:path/filepath"
import "core:strconv"
import "core:mem/virtual"

THREAD :: "native-fixture-existing-thread"
main :: proc() {
    arena:virtual.Arena; wire.require(virtual.arena_init_growing(&arena)==nil,"Cannot allocate host fixture arena"); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    endpoint,receipt,stop_file:string; hold:=false; seconds:=3600
    for i:=1;i<len(os.args);i+=1 {
        arg:=os.args[i]
        if arg=="--help" { fmt.println("odin run tools/native_host -- --socket ABSOLUTE_PATH --receipt ABSOLUTE_PATH [--hold-second --stop-file FILE --seconds N]"); return }
        if arg=="--hold-second" { hold=true; continue }
        i+=1; wire.require(i<len(os.args),"Missing host option value")
        switch arg {
        case "--socket": endpoint=os.args[i]
        case "--receipt": receipt=os.args[i]
        case "--stop-file": stop_file=os.args[i]
        case "--seconds": value,ok:=strconv.parse_int(os.args[i]); wire.require(ok && value>0 && value<=43200,"Invalid host lifetime"); seconds=value
        case: wire.require(false,"Unknown host fixture option")
        }
    }
    wire.require(filepath.is_abs(endpoint) && filepath.is_abs(receipt) && os.dir(endpoint)==os.dir(receipt) && !os.exists(receipt),"Choose new absolute socket/receipt paths in the same private directory")
    listener,error:=socket.listen(endpoint); wire.require(error==.None,"Cannot bind private existing-thread fixture"); defer socket.listener_destroy(&listener)
    methods,questions:=make(wire.Array),make(wire.Array); active:string; connections:=0
    record:=wire.Object{"fixture"="local-existing-thread-api","thread_id"=THREAD,"methods"=methods,"questions"=questions,"connections"=i64(connections),"failures"=wire.Array{}}
    persist(receipt,record); fmt.println(wire.encode(wire.Object{"fixture_socket"=endpoint,"thread_id"=THREAD,"receipt"=receipt}))
    started:=time.tick_now()
    for time.tick_since(started)<time.Duration(seconds)*time.Second {
        if stop_file!="" && os.exists(stop_file) { break }
        channel,accept_error,ready:=socket.accept(&listener); wire.require(accept_error==.None,"Host accept failed"); if !ready { time.sleep(time.Millisecond); continue }
        connections+=1
        for {
            raw,ok:=fixtures.line(&channel); if !ok { break }; request:=wire.parse(raw); method:=wire.s(request,"method"); append(&methods,method)
            params:=wire.get(request,"params"); if method=="initialized" { record["methods"]=methods; persist(receipt,record); continue }
            _,integer_id:=wire.get(request,"id").(i64); wire.require(integer_id,"No reply to unsolicited host approval is permitted")
            result:wire.Object
            switch method {
            case "initialize": wire.require(wire.s(wire.get(params,"clientInfo"),"name")=="katla_odin_editor","Unexpected host client"); result={"userAgent"="explicit-native-local-fixture"}
            case "thread/loaded/list": result={"data"=wire.Array{THREAD},"nextCursor"=nil}
            case "thread/resume": wire.require(wire.s(params,"threadId")==THREAD,"Conversation identity changed"); result={"thread"=wire.Object{"id"=THREAD,"name"="Local native acceptance fixture"}}
            case "thread/read":
                wire.require(wire.s(params,"threadId")==THREAD && wire.b(params,"includeTurns"),"Read changed conversation")
                turns:=wire.Array{}; if active!="" { append(&turns,wire.Object{"id"=active,"status"="inProgress"}) }
                result={"thread"=wire.Object{"id"=THREAD,"status"=wire.Object{"type"="active" if active!="" else "idle"},"turns"=turns}}
            case "turn/start","turn/steer":
                wire.require(wire.s(params,"threadId")==THREAD,"Turn changed conversation")
                if active!="" { wire.require(method=="turn/steer" && wire.s(params,"expectedTurnId")==active,"Steer changed active turn") }
                else { wire.require(method=="turn/start","No active turn to steer"); active=fmt.aprintf("native-turn-%d",len(questions)+1) }
                inputs:=wire.a(params,"input"); wire.require(len(inputs)==2 && wire.s(inputs[0],"type")=="text" && wire.s(inputs[1],"type")=="image","Expected paired text/image input")
                text:=wire.s(inputs[0],"text"); wire.require(strings.contains(text,"Geometry candidates are not proof"),"Viewport context dropped visibility qualifier")
                position:=strings.last_index(text,"\n"); wire.require(position>=0,"Viewport metadata missing"); metadata:=wire.parse(text[position+1:]); wire.require(len(wire.object(metadata))>0,"Viewport metadata empty")
                url:=wire.s(inputs[1],"url"); prefix:="data:image/png;base64,"; wire.require(strings.has_prefix(url,prefix),"Viewport image URI mismatch")
                png,decode_error:=base64.decode(url[len(prefix):]); wire.require(decode_error==nil && len(png)<=32<<20,"Invalid image base64")
                decoded,image_error:=image.texture_image_decode(png); wire.require(image_error==.None && decoded.width>=64 && decoded.height>=64 && decoded.width<=8192 && decoded.height<=8192,"Viewport PNG invalid")
                number:=len(questions)+1; path:=strings.concatenate({os.dir(receipt),fmt.aprintf("/viewport-%d.png",number)}); wire.write(path,string(png)); wire.require(os.chmod(path,{.Read_User,.Write_User})==nil,"Cannot protect screenshot")
                hash:sha2.Context_256; sha2.init_256(&hash); sha2.update(&hash,png); bytes:[32]byte; sha2.final(&hash,bytes[:]); digest:=string(hex.encode(bytes[:]))
                append(&questions,wire.Object{"number"=i64(number),"method"=method,"turn"=active,"text"=text,"metadata"=metadata,"width"=i64(decoded.width),"height"=i64(decoded.height),"png_sha256"=digest,"png_path"=path}); image.texture_image_destroy(&decoded)
                _=fixtures.send(&channel,wire.Object{"id"=wire.get(request,"id"),"result"=wire.Object{"turn"=wire.Object{"id"=active}}})
                _=fixtures.send(&channel,wire.Object{"method"="item/agentMessage/delta","params"=wire.Object{"threadId"="foreign-thread","turnId"="foreign","itemId"="foreign","delta"="FOREIGN MUST NOT APPEAR"}})
                _=fixtures.send(&channel,wire.Object{"method"="item/agentMessage/delta","params"=wire.Object{"threadId"=THREAD,"turnId"=active,"itemId"=fmt.aprintf("native-item-%d",number),"delta"=fmt.aprintf("Local fixture received committed viewport %d. The existing conversation identity is preserved.",number)}})
                if !(hold && active=="native-turn-2") { completed(&channel,active,"completed"); active="" }
            case "turn/interrupt": wire.require(active!="" && wire.s(params,"threadId")==THREAD && wire.s(params,"turnId")==active,"Interrupt changed identity"); _=fixtures.send(&channel,wire.Object{"id"=wire.get(request,"id"),"result"=wire.Object{}}); completed(&channel,active,"interrupted"); active=""
            case: wire.require(false,strings.concatenate({"Unexpected host method: ",method}))
            }
            if method!="turn/start" && method!="turn/steer" && method!="turn/interrupt" { _=fixtures.send(&channel,wire.Object{"id"=wire.get(request,"id"),"result"=result}) }
            record["methods"]=methods; record["questions"]=questions; record["connections"]=i64(connections); record["active_turn_after_disconnect"]=active; persist(receipt,record)
        }; socket.close(&channel)
    }
}
persist :: proc(path:string,value:wire.Object) {
    temporary:=strings.concatenate({path,".tmp"}); wire.write(temporary,wire.encode(value)); wire.require(os.chmod(temporary,{.Read_User,.Write_User})==nil && os.rename(temporary,path)==nil,"Cannot publish private receipt")
}
completed :: proc(channel:^socket.Channel,turn,status:string) { _=fixtures.send(channel,wire.Object{"method"="turn/completed","params"=wire.Object{"threadId"=THREAD,"turn"=wire.Object{"id"=turn,"status"=status}}}) }
