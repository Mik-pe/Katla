//! Capture exporters serialize owned data without driver handles or further GPU activity.
package gfx
import "core:encoding/json"
import "core:fmt"
import "core:strings"

/// Encodes the complete compiler/native recording as structured JSON.
capture_json :: proc(c:^Capture_Snapshot,allocator:=context.allocator)->([]byte,bool) {
    findings:=capture_compare(c,allocator); defer delete(findings)
    data,error:=json.marshal(struct {capture:Capture_Snapshot,comparison:[]Capture_Divergence}{c^,findings[:]},allocator=allocator,opt=json.Marshal_Options{use_enum_names=true,indentation=2}); return data,error==nil
}
@(private="package")
capture_dot_quote :: proc(b:^strings.Builder,value:string) {
    strings.write_byte(b,'"')
    for byte in transmute([]byte)value {
        switch byte {
        case '"','\\': strings.write_byte(b,'\\'); strings.write_byte(b,byte)
        case '\n': strings.write_string(b,"\\n")
        case '\r': strings.write_string(b,"\\r")
        case '\t': strings.write_string(b,"\\t")
        case: if byte>=32 { strings.write_byte(b,byte) }
        }
    }; strings.write_byte(b,'"')
}
/// Draws every declaration, its actual live order and exact compiled dependencies.
capture_dot :: proc(c:^Capture_Snapshot,allocator:=context.allocator)->string {
    b:strings.Builder; strings.builder_init(&b,allocator); defer strings.builder_destroy(&b)
    strings.write_string(&b,"digraph render_graph {\n  rankdir=LR;\n")
    for p in c.passes { fmt.sbprintf(&b,"  p%d [label=",p.index); capture_dot_quote(&b,p.name); fmt.sbprintf(&b,",style=%s];\n",p.live ? "solid" : "dashed") }
    for resource in c.buffers { fmt.sbprintf(&b,"  b%d [shape=ellipse,label=\"buffer %d bytes %d\"];\n",resource.index,resource.index,resource.desc.size) }
    for resource in c.images { fmt.sbprintf(&b,"  i%d [shape=ellipse,label=\"image %d %dx%d %v\"];\n",resource.index,resource.index,resource.desc.width,resource.desc.height,resource.desc.format) }
    for pass in c.passes {
        for access in pass.buffers { if access_reads(access.mode) { fmt.sbprintf(&b,"  b%d -> p%d;\n",access.resource_index,pass.index) }; if access_writes(access.mode) { fmt.sbprintf(&b,"  p%d -> b%d;\n",pass.index,access.resource_index) } }
        for access in pass.images { if access_reads(access.mode) { fmt.sbprintf(&b,"  i%d -> p%d;\n",access.resource_index,pass.index) }; if access_writes(access.mode) { fmt.sbprintf(&b,"  p%d -> i%d;\n",pass.index,access.resource_index) } }
    }
    for h in c.dependencies { fmt.sbprintf(&b,"  p%d -> p%d [label=\"%v %d\"];\n",h.before,h.after,h.resource_kind,h.resource_index) }
    previous_encoder:=-1
    for e,i in c.events {
        if e.kind==.Allocation {
            fmt.sbprintf(&b,"  a%d [shape=box,label=\"physical %d bytes %d\"];\n",e.object,e.object,e.size)
            if e.resource_kind==.Buffer { fmt.sbprintf(&b,"  b%d -> a%d [style=dotted];\n",e.resource_index,e.object) }
            if e.resource_kind==.Image { fmt.sbprintf(&b,"  i%d -> a%d [style=dotted];\n",e.resource_index,e.object) }
            if e.heap!=0 { fmt.sbprintf(&b,"  h%d [shape=box,label=\"heap %d\"];\n  a%d -> h%d [label=\"offset %d\"];\n",e.heap,e.heap,e.object,e.heap,e.offset) }
        }
        if e.kind==.Alias && e.alias_previous_kind==.Buffer && e.resource_kind==.Buffer { fmt.sbprintf(&b,"  b%d -> b%d [label=\"alias p%d to p%d\",color=blue];\n",e.alias_previous_index,e.resource_index,e.previous_pass_index,e.pass_index) }
        if e.kind==.Alias && e.alias_previous_kind==.Image && e.resource_kind==.Image { fmt.sbprintf(&b,"  i%d -> i%d [label=\"alias p%d to p%d\",color=blue];\n",e.alias_previous_index,e.resource_index,e.previous_pass_index,e.pass_index) }
        if e.kind==.Encoder_Begin {
            fmt.sbprintf(&b,"  e%d [shape=hexagon,label=\"native encoder %d\"];\n",i,e.encoder)
            if e.pass_index>=0 { fmt.sbprintf(&b,"  p%d -> e%d [style=dotted];\n",e.pass_index,i) }
            if previous_encoder>=0 { fmt.sbprintf(&b,"  e%d -> e%d [color=gray];\n",previous_encoder,i) }; previous_encoder=i
        }
    }
    strings.write_string(&b,"}\n"); return strings.clone(strings.to_string(b),allocator)
}
/// Summarizes an accepted frame with the observed native timeline and feedback.
capture_text :: proc(c:^Capture_Snapshot,allocator:=context.allocator)->string {
    b:strings.Builder; strings.builder_init(&b,allocator); defer strings.builder_destroy(&b)
    fmt.sbprintf(&b,"%v submission %d frame slot %d generation %d revision %d: %v\n",c.backend,c.submission,c.slot,c.generation,c.revision,c.feedback)
    for p in c.passes { fmt.sbprintf(&b,"pass %d order %d %v %s live=%v\n",p.index,p.order,p.kind,p.name,p.live) }
    for e,i in c.events { fmt.sbprintf(&b,"native %d %v pass=%d object=%d resource=%v:%d bytes=%d+%d emitted=%v %s %s\n",i,e.kind,e.pass_index,e.object,e.resource_kind,e.resource_index,e.offset,e.size,e.emitted,e.label,e.reason) }
    findings:=capture_compare(c,allocator); defer delete(findings)
    for finding in findings { fmt.sbprintf(&b,"divergence %v pass=%d resource=%d expected=%d observed=%d dependency=%d\n",finding.kind,finding.pass_index,finding.resource_index,finding.expected_index,finding.event_index,finding.dependency_index) }
    return strings.clone(strings.to_string(b),allocator)
}
