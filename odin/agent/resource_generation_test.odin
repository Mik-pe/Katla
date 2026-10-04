#+test
package agent
import "core:testing"
import "core:mem"

@(test)
test_generation_admission_owns_utf8_request_and_rejects_unknown_shapes :: proc(t:^testing.T) {
    encoded:=make([]byte,len(`{"path":"assets/particles/å.json","resource_type":"particle_system","description":"RAIN"}`)); copy(encoded,transmute([]byte)string(`{"path":"assets/particles/å.json","resource_type":"particle_system","description":"RAIN"}`))
    allocator:=context.allocator
    decoded,error:=resource_generation_decode(encoded,allocator); delete(encoded,allocator)
    testing.expect_value(t,error,Call_Error.None); testing.expect(t,decoded.request.path=="assets/particles/å.json" && decoded.request.description=="RAIN" && decoded.request.kind==.Particle_System)
    context.allocator=mem.nil_allocator(); resource_generation_destroy(&decoded); context.allocator=allocator
    for invalid in ([5]string{`{"path":"a","resource_type":"unknown","description":"fire"}`,`{"path":"a","resource_type":"scene","description":null}`,`{"path":"a","resource_type":"scene"}`,`{"path":"a","resource_type":"scene","description":"x","extra":0}`,`{"path":"a","path":"b","resource_type":"scene","description":"x"}`}) {
        call,err:=decode_call({name="generate_resource",arguments=transmute([]byte)invalid}); defer decoded_call_destroy(&call); testing.expect(t,err!=.None)
    }
}
