//! Bounded stdio proxy forwards MCP bytes to the existing private editor world.
package main

import "core:os"
import "core:fmt"

main :: proc() {
    environment_endpoint:=os.get_env("KATLA_MCP_SOCKET",context.allocator); defer delete(environment_endpoint)
    endpoint:=environment_endpoint
    if len(os.args)==2 { endpoint=os.args[1] }
    if len(os.args)>2 || endpoint=="" { fmt.eprintln("usage: katla-mcp <private-editor.sock> or KATLA_MCP_SOCKET"); os.exit(2) }
    if !run_proxy(endpoint) { os.exit(1) }
}
