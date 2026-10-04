//! Confined URI resolution and encoded image ownership for glTF dependencies.
package app

import resources "../resources"
import cgltf "../deps/cgltf"
import "core:strings"
import "core:slice"
import "core:encoding/base64"

@(private="package")
gltf_hex :: proc(character:byte)->(byte,bool) {
    if character>='0' && character<='9' { return character-'0',true }
    if character>='a' && character<='f' { return character-'a'+10,true }
    if character>='A' && character<='F' { return character-'A'+10,true }
    return 0,false
}
@(private="package")
gltf_dependency_path :: proc(model_path,uri:string)->(string,Gltf_Error) {
    if uri=="" || uri[0]=='/' || strings.contains_any(uri,"\\:?#") { return "",.Invalid_Path }
    decoded:=make([]byte,len(uri)); defer delete(decoded)
    written:=0
    for cursor:=0;cursor<len(uri);cursor+=1 {
        character:=uri[cursor]
        if character=='%' {
            if len(uri)-cursor<3 { return "",.Invalid_Path }
            a,av:=gltf_hex(uri[cursor+1]); b,bv:=gltf_hex(uri[cursor+2]); if !av || !bv { return "",.Invalid_Path }
            character=a*16+b; cursor+=2
        }
        if character==0 || character=='\\' || character==':' { return "",.Invalid_Path }
        decoded[written]=character; written+=1
    }
    if written==0 || decoded[0]=='/' { return "",.Invalid_Path }
    prefix:=""; slash:=strings.last_index(model_path,"/"); if slash>=0 { prefix=model_path[:slash+1] }
    joined:=strings.concatenate({prefix,string(decoded[:written])}); defer delete(joined)
    parts:=make([dynamic]string); defer delete(parts)
    remaining:=joined
    for part in strings.split_iterator(&remaining,"/") {
        if part=="" { return "",.Invalid_Path }
        if part=="." { continue }
        if part==".." { if len(parts)==0 { return "",.Invalid_Path }; resize(&parts,len(parts)-1); continue }
        append(&parts,part)
    }
    path:=strings.join(parts[:],"/")
    if !resources.valid_relative_path(path) { delete(path); return "",.Invalid_Path }
    return path,.None
}
@(private="package")
gltf_uri_bytes :: proc(root:^resources.Root,model_path,uri:string,budget:^int)->([]byte,string,Gltf_Error) {
    if strings.has_prefix(uri,"data:") {
        comma:=strings.index(uri,","); if comma<0 { return nil,"",.Invalid_Data }
        header:=uri[5:comma]
        if !strings.has_suffix(header,";base64") { return nil,"",.Unsupported }
        if base64.decoded_len(uri[comma+1:])>budget^ { return nil,"",.Limit }
        bytes,error:=base64.decode(uri[comma+1:]); if error!=nil { return nil,"",.Invalid_Data }
        budget^-=len(bytes); return bytes,header[:len(header)-7],.None
    }
    path,path_error:=gltf_dependency_path(model_path,uri); if path_error!=.None { return nil,"",path_error }; defer delete(path)
    bytes,read_error:=resources.read_bytes(root,path,min(resources.MAX_BYTES,budget^))
    if read_error!=.None { return nil,"",.Limit if read_error==.Limit else .IO }
    owned:=slice.clone(bytes); delete(bytes,root.allocator); budget^-=len(owned)
    return owned,"",.None
}
@(private="package")
gltf_load_buffers :: proc(root:^resources.Root,path:string,data:^cgltf.data,buffers:[][]byte,budget:^int)->Gltf_Error {
    for &buffer,i in data.buffers {
        if buffer.size>uint(budget^) { return .Limit }
        if buffer.uri==nil {
            if i!=0 || data.file_type!=.glb || buffer.size>uint(len(data.bin)) { return .Invalid_Data }
            buffer.data=raw_data(data.bin); continue
        }
        bytes,_,error:=gltf_uri_bytes(root,path,string(buffer.uri),budget)
        if error!=.None { return error }
        buffers[i]=bytes
        if uint(len(bytes))<buffer.size { return .Invalid_Data }
        buffer.data=raw_data(bytes); buffer.data_free_method=.none
    }
    return .None
}
@(private="package")
gltf_span_valid :: proc(size,offset,stride,count,element_size:uint)->bool {
    return count>0 && stride>=element_size && offset<=size && element_size<=size-offset && count-1<=(size-offset-element_size)/stride
}
@(private="package")
gltf_validate_storage :: proc(data:^cgltf.data)->Gltf_Error {
    for view in data.buffer_views {
        if view.buffer==nil || view.offset>view.buffer.size || view.size>view.buffer.size-view.offset { return .Invalid_Accessor }
        if view.has_meshopt_compression { return .Unsupported }
    }
    for accessor in data.accessors {
        if accessor.count==0 || accessor.count>MAX_MESH_INDICES || accessor.type==.invalid || accessor.component_type==.invalid { return .Invalid_Accessor }
        element_size:=cgltf.calc_size(accessor.type,accessor.component_type)
        if element_size==0 || accessor.stride<element_size { return .Invalid_Accessor }
        if accessor.buffer_view!=nil && !gltf_span_valid(accessor.buffer_view.size,accessor.offset,accessor.stride,accessor.count,element_size) { return .Invalid_Accessor }
        if accessor.is_sparse {
            sparse:=accessor.sparse
            if sparse.count==0 || sparse.count>accessor.count || sparse.indices_buffer_view==nil || sparse.values_buffer_view==nil || sparse.indices_component_type not_in (bit_set[cgltf.component_type]{.r_8u,.r_16u,.r_32u}) { return .Invalid_Accessor }
            index_size:=cgltf.component_size(sparse.indices_component_type)
            if !gltf_span_valid(sparse.indices_buffer_view.size,sparse.indices_byte_offset,index_size,sparse.count,index_size) || !gltf_span_valid(sparse.values_buffer_view.size,sparse.values_byte_offset,element_size,sparse.count,element_size) { return .Invalid_Accessor }
        }
    }
    for node in data.nodes {
        cursor:=node.parent; depth:=0
        for cursor!=nil { if depth>=1024 { return .Limit }; depth+=1; cursor=cursor.parent }
    }
    return .None
}
@(private="package")
gltf_extract_images :: proc(root:^resources.Root,path:string,data:^cgltf.data,model:^Gltf_Model,budget:^int)->Gltf_Error {
    model.images=make([]Gltf_Image,len(data.images))
    for &image,i in data.images {
        target:=&model.images[i]; target.name=gltf_name(image.name)
        mime:=""; if image.mime_type!=nil { mime=string(image.mime_type) }
        if image.buffer_view!=nil {
            view:=image.buffer_view
            if view.size>uint(budget^) { return .Limit }
            bytes:=cgltf.buffer_view_data(view); if bytes==nil { return .Invalid_Data }
            target.encoded=slice.clone(bytes[:int(view.size)]); budget^-=int(view.size)
        } else if image.uri!=nil {
            encoded,uri_mime,error:=gltf_uri_bytes(root,path,string(image.uri),budget)
            if error!=.None { return error }; target.encoded=encoded
            if mime=="" { mime=uri_mime }
        } else { return .Invalid_Data }
        bytes:=target.encoded
        actual:=""
        if len(bytes)>=8 && bytes[0]==137 && string(bytes[1:4])=="PNG" && bytes[4]==13 && bytes[5]==10 && bytes[6]==26 && bytes[7]==10 { actual="image/png" }
        else if len(bytes)>=3 && bytes[0]==255 && bytes[1]==216 && bytes[2]==255 { actual="image/jpeg" }
        if actual=="" { return .Unsupported }
        if mime!="" && mime!=actual { return .Invalid_Data }
        target.mime=strings.clone(actual)
    }
    return .None
}
