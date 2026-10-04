#+build windows
package socket

import "core:mem"

Error :: enum { None, Invalid_Path, Permission, Exists, Unavailable, Closed, IO }
Channel :: struct {}
Listener :: struct { channel:Channel, path:string, inode,device:u64, allocator:mem.Allocator }
private_endpoint :: proc(_:string)->Error { return .Unavailable }
connect :: proc(_:string)->(Channel,Error) { return {},.Unavailable }
listen :: proc(_:string,_:=context.allocator)->(Listener,Error) { return {},.Unavailable }
accept :: proc(_:^Listener)->(Channel,Error,bool) { return {},.Unavailable,false }
close :: proc(_:^Channel) {}
buffer_prepare :: proc(_:^Channel,_:bool)->(i32,i32,Error) { return 0,0,.Unavailable }
shutdown_write :: proc(_:^Channel)->Error { return .Unavailable }
listener_destroy :: proc(_:^Listener) {}
wait :: proc(_:^Channel,_:bool,_:i32)->(bool,Error) { return false,.Unavailable }
read :: proc(_:^Channel,_:[]byte)->(int,Error) { return 0,.Unavailable }
write :: proc(_:^Channel,_:[]byte)->(int,Error) { return 0,.Unavailable }
