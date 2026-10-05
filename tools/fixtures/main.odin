#+feature dynamic-literals
//! Generate deterministic audio and glTF acceptance assets without a scripting runtime.
package main
import "../wire"
import "core:os"
import "core:fmt"
import "core:math"
import "core:encoding/base64"
import "core:strings"
import "core:mem/virtual"

Mesh :: struct { bytes:[dynamic]byte,views,accessors:wire.Array }
accessor :: proc(mesh:^Mesh,values:[][4]f32,components:int,shape:string)->i64 {
    for len(mesh.bytes)%4!=0 { append(&mesh.bytes,0) }; offset:=len(mesh.bytes)
    low,high:[4]f32; low={max(f32),max(f32),max(f32),max(f32)}; high={-max(f32),-max(f32),-max(f32),-max(f32)}
    for value in values { for i:=0;i<components;i+=1 { bits:=transmute(u32)value[i]; for j:=0;j<4;j+=1 { append(&mesh.bytes,u8(bits>>uint(8*j))) }; low[i]=min(low[i],value[i]); high[i]=max(high[i],value[i]) } }
    append(&mesh.views,wire.Object{"buffer"=i64(0),"byteOffset"=i64(offset),"byteLength"=i64(len(mesh.bytes)-offset)})
    row:=wire.Object{"bufferView"=i64(len(mesh.views)-1),"componentType"=i64(5126),"count"=i64(len(values)),"type"=shape}
    if shape=="VEC3" { row["min"]=wire.Array{f64(low[0]),f64(low[1]),f64(low[2])}; row["max"]=wire.Array{f64(high[0]),f64(high[1]),f64(high[2])} }; append(&mesh.accessors,row); return i64(len(mesh.accessors)-1)
}
indices :: proc(mesh:^Mesh,values:[]u16)->i64 {
    for len(mesh.bytes)%4!=0 { append(&mesh.bytes,0) }; offset:=len(mesh.bytes)
    for value in values { append(&mesh.bytes,u8(value),u8(value>>8)) }
    append(&mesh.views,wire.Object{"buffer"=i64(0),"byteOffset"=i64(offset),"byteLength"=i64(len(mesh.bytes)-offset)})
    append(&mesh.accessors,wire.Object{"bufferView"=i64(len(mesh.views)-1),"componentType"=i64(5123),"count"=i64(len(values)),"type"="SCALAR"}); return i64(len(mesh.accessors)-1)
}
// Embedded normal textures are pinned acceptance data shared with the checked-in models.
base :: proc(mesh:Mesh,png:string)->wire.Object {
    return {"asset"=wire.Object{"version"="2.0","generator"="Katla Odin fixture generator"},"buffers"=wire.Array{wire.Object{"byteLength"=i64(len(mesh.bytes)),"uri"=strings.concatenate({"data:application/octet-stream;base64,",base64.encode(mesh.bytes[:])})}},"bufferViews"=mesh.views,"accessors"=mesh.accessors,"images"=wire.Array{wire.Object{"uri"=png}},"textures"=wire.Array{wire.Object{"source"=i64(0)}},"nodes"=wire.Array{wire.Object{"mesh"=i64(0)}},"scenes"=wire.Array{wire.Object{"nodes"=wire.Array{i64(0)}}},"scene"=i64(0)}
}
models :: proc(output:string) {
    mesh:Mesh
    position:=accessor(&mesh,{{-1,-1,0,0},{1,-1,0,0},{1,1,0,0},{-1,1,0,0}},3,"VEC3")
    normal:=accessor(&mesh,{{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0}},3,"VEC3")
    tangent:=accessor(&mesh,{{1,0,0,1},{1,0,0,1},{1,0,0,1},{1,0,0,1}},4,"VEC4")
    uv:=accessor(&mesh,{{0,0,0,0},{1,0,0,0},{1,1,0,0},{0,1,0,0}},2,"VEC2"); index:=indices(&mesh,{0,1,2,0,2,3})
    pinned:=wire.parse(wire.read("resources/models/MirrorNormal.gltf")); png:=wire.s(wire.a(pinned,"images")[0],"uri")
    for name in ([]string{"MirrorNormal","UnlitBlend","BlendDepth"}) {
        current:=mesh
        back:=position; if name=="BlendDepth" { back=accessor(&current,{{-1,-1,-1,0},{1,-1,-1,0},{1,1,-1,0},{-1,1,-1,0}},3,"VEC3") }
        document:=base(current,png); primitives:=wire.Array{wire.Object{"attributes"=wire.Object{"POSITION"=position,"NORMAL"=normal,"TANGENT"=tangent,"TEXCOORD_0"=uv},"indices"=index,"material"=i64(0)}}
        materials:=wire.Array{wire.Object{"pbrMetallicRoughness"=wire.Object{"baseColorFactor"=wire.Array{f64(0.5),f64(0.5),f64(0.5),i64(1)},"metallicFactor"=i64(0),"roughnessFactor"=i64(1)},"normalTexture"=wire.Object{"index"=i64(0)}}}
        if name=="UnlitBlend" { materials=wire.Array{wire.Object{"pbrMetallicRoughness"=wire.Object{"baseColorFactor"=wire.Array{f64(0.5),f64(0.5),f64(0.5),f64(0.5)},"metallicFactor"=i64(1),"roughnessFactor"=i64(0)},"normalTexture"=wire.Object{"index"=i64(0)},"emissiveFactor"=wire.Array{i64(1),i64(0),i64(0)},"emissiveTexture"=wire.Object{"index"=i64(0)},"alphaMode"="BLEND","extensions"=wire.Object{"KHR_materials_unlit"=wire.Object{}}}} }
        if name=="BlendDepth" {
            append(&primitives,wire.Object{"attributes"=wire.Object{"POSITION"=back,"NORMAL"=normal,"TANGENT"=tangent,"TEXCOORD_0"=uv},"indices"=index,"material"=i64(1)})
            materials=wire.Array{}; for color in ([]wire.Array{{i64(1),i64(0),i64(0),f64(0.5)},{i64(0),i64(0),i64(1),f64(0.5)}}) { append(&materials,wire.Object{"pbrMetallicRoughness"=wire.Object{"baseColorFactor"=color},"alphaMode"="BLEND","extensions"=wire.Object{"KHR_materials_unlit"=wire.Object{}}}) }
        }
        if name!="MirrorNormal" { document["extensionsUsed"]=wire.Array{"KHR_materials_unlit"}; document["extensionsRequired"]=wire.Array{"KHR_materials_unlit"} }
        document["materials"]=materials; document["meshes"]=wire.Array{wire.Object{"primitives"=primitives}}; wire.write(strings.concatenate({output,"/",name,".gltf"}),wire.encode(document))
    }
    tangent_model(output)
}
tangent_model :: proc(output:string) {
    mesh:Mesh
    uv0:=accessor(&mesh,{{0,0,0,0},{1,0,0,0},{1,1,0,0},{0,1,0,0},{0,0,0,0},{1,0,0,0},{1,1,0,0},{0,1,0,0}},2,"VEC2")
    uv1:=accessor(&mesh,{{0,0,0,0},{0,1,0,0},{-1,1,0,0},{-1,0,0,0},{0,0,0,0},{0,1,0,0},{-1,1,0,0},{-1,0,0,0}},2,"VEC2")
    normal:=accessor(&mesh,{{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0},{0,0,1,0}},3,"VEC3")
    tangent:=accessor(&mesh,{{0,1,0,1},{0,1,0,1},{0,1,0,1},{0,1,0,1},{0,1,0,1},{0,1,0,1},{0,1,0,1},{0,1,0,1}},4,"VEC4")
    left:=indices(&mesh,{0,1,2,0,2,3}); right:=indices(&mesh,{4,5,6,4,6,7})
    position:=accessor(&mesh,{{-1.5,-1,0,0},{-0.1,-1,0,0},{-0.1,1,0,0},{-1.5,1,0,0},{0.1,-1,-0.5,0},{1.5,-1,-0.5,0},{1.5,1,-0.5,0},{0.1,1,-0.5,0}},3,"VEC3")
    pinned:=wire.parse(wire.read("resources/models/TangentUV.gltf")); document:=base(mesh,wire.s(wire.a(pinned,"images")[0],"uri"))
    for key in ([]string{"samplers","materials","extensionsUsed","extensionsRequired"}) { document[key]=wire.get(pinned,key) }; document["textures"]=wire.Array{wire.Object{"source"=i64(0),"sampler"=i64(0)}}
    first:=wire.Object{"POSITION"=position,"NORMAL"=normal,"TEXCOORD_0"=uv0,"TEXCOORD_1"=uv1}; second:=wire.Object{"POSITION"=position,"NORMAL"=normal,"TEXCOORD_0"=uv0,"TEXCOORD_1"=uv1,"TANGENT"=tangent}
    document["meshes"]=wire.Array{wire.Object{"primitives"=wire.Array{wire.Object{"attributes"=first,"indices"=left,"material"=i64(0)},wire.Object{"attributes"=second,"indices"=right,"material"=i64(1)}}}}
    wire.write(strings.concatenate({output,"/TangentUV.gltf"}),wire.encode(document))
}
audio :: proc(output:string) {
    wave:=make([]byte,44+4800*2); copy(wave[:],transmute([]byte)string("RIFF")); copy(wave[8:],transmute([]byte)string("WAVEfmt ")); copy(wave[36:],transmute([]byte)string("data"))
    for item in ([][2]u32{{4,u32(len(wave)-8)},{16,16},{20,0x00010001},{24,24000},{28,48000},{32,0x00100002},{40,9600}}) { for j:=0;j<4;j+=1 { wave[int(item[0])+j]=u8(item[1]>>uint(j*8)) } }
    for i:=0;i<4800;i+=1 { sample:=u16(i16(math.round(math.sin(2*math.PI*440*f64(i)/24000)*16383))); wave[44+i*2]=u8(sample); wave[45+i*2]=u8(sample>>8) }
    source:=strings.concatenate({output,"/tone.wav"}); wire.write(source,string(wave))
    for codec in ([]string{"ogg","mp3","flac"}) {
        command:=make([dynamic]string); append(&command,"ffmpeg","-hide_banner","-loglevel","error","-y","-i",source)
        switch codec {
        case "ogg": append(&command,"-ac","2","-c:a","vorbis","-strict","-2")
        case "mp3": append(&command,"-c:a","libmp3lame","-b:a","64k")
        case "flac": append(&command,"-c:a","flac")
        }
        append(&command,strings.concatenate({output,"/tone.",codec})); state,_,_,error:=os.process_exec({command=command[:]},context.allocator); wire.require(error==nil && state.exited && state.exit_code==0,"Audio fixture encoder failed")
    }
}
main :: proc() {
    arena:virtual.Arena; wire.require(virtual.arena_init_growing(&arena)==nil,"Cannot allocate fixture arena"); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    wire.require(len(os.args)==3,"odin run tools/fixtures -- audio|models OUTPUT_DIR"); output:=os.args[2]; wire.require(os.is_dir(output) || os.make_directory_all(output)==nil,"Cannot create fixture output")
    switch os.args[1] {
    case "audio": audio(output)
    case "models": models(output)
    case: wire.require(false,"Unknown fixture family")
    }; fmt.println("Generated Odin fixtures:",output)
}
