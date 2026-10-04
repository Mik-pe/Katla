#+build windows
package host

Config_Unix_Only :: true
Transport :: struct {}
config_validate :: proc(_:Config)->Error { return .Unsupported }
transport_open :: proc(_:^Transport,_:Config)->Error { return .Unsupported }
transport_close :: proc(_:^Transport) {}
transport_wait :: proc(_:^Transport,_:bool,_:i32)->(bool,Error) { return false,.Unsupported }
transport_read :: proc(_:^Transport,_:[]byte)->(int,Error) { return 0,.Unsupported }
transport_write :: proc(_:^Transport,_:[]byte)->(int,Error) { return 0,.Unsupported }
transport_worker_signals :: proc() {}
