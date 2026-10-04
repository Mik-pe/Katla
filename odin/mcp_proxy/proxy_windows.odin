#+build windows
package main

import "core:fmt"
run_proxy :: proc(_:string)->bool { fmt.eprintln("Private Unix editor sockets are unavailable on this platform."); return false }
