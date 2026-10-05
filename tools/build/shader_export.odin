//! Export selected WGSL artifacts through the existing bounded compiler protocol.
package katla_build
import shader "../../odin/gfx/shader"
import "core:os"
import "core:encoding/json"
import "core:fmt"

export_shader :: proc(compiler,source,entry,stage,basename:string) {
    selected:shader.Stage
    switch stage {
    case "Vertex": selected=.Vertex
    case "Fragment": selected=.Fragment
    case "Compute": selected=.Compute
    case: fail("Shader stage must be Vertex, Fragment or Compute")
    }
    instance:shader.Compiler; require(shader.compiler_init(&instance,compiler,join(output,"shader-export-cache"))==.None,"Cannot open shader compiler"); defer { require(shader.compiler_destroy(&instance)==.None,"Compiler is busy") }; result,compile_error:=shader.compile(&instance,string(read(source)),{{name=entry,stage=selected}})
    defer shader.compiled_destroy(&result)
    require(compile_error==.None,fmt.aprintf("Shader compiler: %v: %s",compile_error,result.message))
    require(len(result.entries)==1 && result.entries[0].name==entry && result.entries[0].stage==selected,"Compiler selection mismatch")
    mkdir(os.dir(basename)); artifact:=struct { abi:int,compiler:string,error:shader.Error,message:string,entries:[]shader.Entry }{1,result.compiler,.None,result.message,result.entries}; data,error:=json.marshal(artifact,{spec=.JSON,use_enum_names=true,sort_maps_by_key=true}); require(error==nil,"Cannot encode shader artifact")
    write(cat(basename,".json"),string(data)); write(cat(basename,".metal"),result.entries[0].metal_source)
    words:=result.entries[0].spirv; bytes:=make([]byte,len(words)*4)
    for word,i in words { bytes[i*4]=u8(word); bytes[i*4+1]=u8(word>>8); bytes[i*4+2]=u8(word>>16); bytes[i*4+3]=u8(word>>24) }
    require(os.write_entire_file(cat(basename,".spv"),bytes)==nil,"Cannot export SPIR-V")
}
