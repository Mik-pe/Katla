//! Run explicit deterministic HTTP/SSE editor fixtures until a bounded deadline or stop file.
package main
import "../providers"
import "../wire"
import "core:os"
import "core:time"
import "core:fmt"
import "core:strconv"

main :: proc() {
    config,receipt,stop_file:string; scenario:="material"; port:=0; seconds:=3600
    for i:=1;i<len(os.args);i+=1 {
        arg:=os.args[i]; if arg=="--help" { fmt.println("odin run tools/provider -- --config FILE [--scenario material|editor_scene|cancel|http429|truncated] [--port N --seconds N --stop-file FILE --receipt FILE]"); return }
        i+=1; wire.require(i<len(os.args),"Missing provider option value"); value:=os.args[i]
        switch arg {
        case "--config": config=value
        case "--receipt": receipt=value
        case "--stop-file": stop_file=value
        case "--scenario": wire.require(value=="material" || value=="editor_scene" || value=="cancel" || value=="http429" || value=="truncated","Unknown provider scenario"); scenario=value
        case "--port": number,ok:=strconv.parse_int(value); wire.require(ok && number>=0 && number<=65535,"Invalid port"); port=number
        case "--seconds": number,ok:=strconv.parse_int(value); wire.require(ok && number>0 && number<=43200,"Invalid provider lifetime"); seconds=number
        case: wire.require(false,"Unknown provider option")
        }
    }
    wire.require(config!="","Choose --config"); server:providers.Server; providers.start(&server,port)
    defer providers.stop(&server,receipt)
    wire.write(config,fmt.aprintf("provider=\"open_ai_compatible\"\napi=\"responses\"\napi_key=\"local-transport-test\"\nbase_url=\"http://127.0.0.1:%d/%s/v1\"\nmodel=\"explicit-test-model\"\nrate_limit_min_interval_ms=0\ntimeout_ms=10000\n",server.port,scenario)); wire.require(os.chmod(config,{.Read_User,.Write_User})==nil,"Cannot protect config")
    fmt.println("Local fixture ready:",server.port,"scenario:",scenario,"config:",config)
    started:=time.tick_now(); for time.tick_since(started)<time.Duration(seconds)*time.Second { if stop_file!="" && os.exists(stop_file) { break }; time.sleep(10*time.Millisecond) }
}
