//! Content-addressed artifacts are admitted only after bounded process output validates.
package shader

import "core:os"
import "core:mem"
import "core:sync"
import "core:time"
import "core:path/filepath"
import "core:crypto/sha2"
import "core:encoding/json"

@(private="package")
MAX_COMPILER_BYTES :: 256*1024*1024
@(private="package")
process_start_mutex:sync.Mutex

@(private="package")
compiler_read_bounded :: proc(path:string,limit:int,allocator:mem.Allocator)->([]byte,bool) {
    file,error:=os.open(path); if error!=nil { return nil,false }; defer os.close(file)
    info,stat_error:=os.fstat(file,allocator); if stat_error!=nil { return nil,false }; defer os.file_info_delete(info,allocator)
    if info.type!=.Regular || info.size<=0 || info.size>i64(limit) { return nil,false }
    data:=make([]byte,int(info.size),allocator)
    offset:=0
    for offset<len(data) {
        count,read_error:=os.read(file,data[offset:]); offset+=count
        if count==0 || read_error!=nil { delete(data,allocator); return nil,false }
    }
    extra:[1]byte
    count,_:=os.read(file,extra[:])
    if count!=0 { delete(data,allocator); return nil,false }
    return data,true
}
@(private="package")
compiler_key :: proc(request,executable:[]byte,allocator:mem.Allocator)->string {
    digest:sha2.Context_256; sha2.init_256(&digest)
    sha2.update(&digest,transmute([]byte)string(COMPILER)); sha2.update(&digest,executable); sha2.update(&digest,request)
    return compiler_digest_finish(&digest,allocator)
}
@(private="package")
compiler_digest :: proc(data:[]byte,allocator:mem.Allocator)->string { digest:sha2.Context_256; sha2.init_256(&digest); sha2.update(&digest,data); return compiler_digest_finish(&digest,allocator) }
@(private="package")
compiler_digest_finish :: proc(digest:^sha2.Context_256,allocator:mem.Allocator)->string {
    output:[32]byte; sha2.final(digest,output[:])
    text:=make([]byte,64,allocator)
    hex :=  "0123456789abcdef"
    for value,i in output { text[i*2]=hex[value>>4]; text[i*2+1]=hex[value&15] }
    return string(text)
}
@(private="package")
compiler_reply_valid :: proc(data,request_data:[]byte,allocator:mem.Allocator)->bool {
    reply:Reply; request:Request
    error:=json.unmarshal(data,&reply,spec=.JSON,allocator=allocator)
    artifact:=Compiled{reply.compiler,reply.message,reply.entries,allocator}; defer compiled_destroy(&artifact)
    if error!=nil || reply.abi!=ABI || reply.compiler!=COMPILER { return false }
    request_error:=json.unmarshal(request_data,&request,spec=.JSON,allocator=allocator)
    defer { delete(request.source,allocator); for selected in request.selections { delete(selected.name,allocator) }; delete(request.selections,allocator); for key in request.constants { delete(key,allocator) }; delete(request.constants) }
    if request_error!=nil { return false }
    if reply.error!=.None { return len(reply.entries)==0 && len(reply.message)>0 }
    if len(reply.entries)!=len(request.selections) || len(reply.message)!=0 { return false }
    for entry,i in reply.entries { if entry.name!=request.selections[i].name || entry.stage!=request.selections[i].stage || !entry_valid(entry) { return false } }
    return true
}
@(private="package")
compiler_artifact :: proc(compiler:^Compiler,request:[]byte,allocator:mem.Allocator)->([]byte,Error) {
    executable,read_executable:=compiler_read_bounded(compiler.executable,MAX_COMPILER_BYTES,allocator)
    if !read_executable { return nil,.Load_Failed }; defer delete(executable,allocator)
    key:=compiler_key(request,executable,allocator); defer delete(key,allocator)
    cache,cache_error:=filepath.join({compiler.cache_directory,key},allocator=allocator); if cache_error!=nil { return nil,.Internal_Failure }; defer delete(cache,allocator)
    cached,read_cached:=compiler_read_bounded(cache,MAX_REPLY+130,allocator)
    if read_cached {
        valid:=len(cached)>130 && cached[64]=='\n' && cached[129]=='\n' && string(cached[:64])==key
        if valid {
            digest:=compiler_digest(cached[130:],allocator)
            valid=digest==string(cached[65:129]) && compiler_reply_valid(cached[130:],request,allocator); delete(digest,allocator)
        }
        if valid {
            reply:=make([]byte,len(cached)-130,allocator); copy(reply,cached[130:]); delete(cached,allocator)
            sync.mutex_lock(&compiler.mutex); compiler.cache_hits+=1; sync.mutex_unlock(&compiler.mutex)
            return reply,.None
        }
        delete(cached,allocator)
    }
    workspace,workspace_error:=os.make_directory_temp(compiler.cache_directory,"compile-*",allocator)
    if workspace_error!=nil { return nil,.Load_Failed }; defer { os.remove_all(workspace); delete(workspace,allocator) }
    input,input_error:=filepath.join({workspace,"request.json"},allocator=allocator); if input_error!=nil { return nil,.Internal_Failure }; defer delete(input,allocator)
    output,output_error:=filepath.join({workspace,"artifact.json"},allocator=allocator); if output_error!=nil { return nil,.Internal_Failure }; defer delete(output,allocator)
    if os.write_entire_file(input,request)!=nil { return nil,.Load_Failed }
    sync.mutex_lock(&process_start_mutex)
    process,start_error:=os.process_start({command={compiler.executable,"--request",input,"--output",output}})
    sync.mutex_unlock(&process_start_mutex)
    if start_error!=nil { return nil,.Load_Failed }
    sync.mutex_lock(&compiler.mutex); compiler.process_runs+=1; sync.mutex_unlock(&compiler.mutex)
    state,wait_error:=os.process_wait(process,30*time.Second)
    if wait_error!=nil {
        kill_error:=os.process_kill(process)
        _,reap_error:=os.process_wait(process,5*time.Second)
        if kill_error!=nil || reap_error!=nil { return nil,.Internal_Failure }
        return nil,.Internal_Failure
    }
    if !state.exited || state.exit_code!=0 { return nil,.Internal_Failure }
    artifact,read_artifact:=compiler_read_bounded(output,MAX_REPLY,allocator)
    if !read_artifact { return nil,.Invalid_Reply }
    if !compiler_reply_valid(artifact,request,allocator) { delete(artifact,allocator); return nil,.Invalid_Reply }
    after,read_after:=compiler_read_bounded(compiler.executable,MAX_COMPILER_BYTES,allocator)
    if !read_after { delete(artifact,allocator); return nil,.Load_Failed }; defer delete(after,allocator)
    after_key:=compiler_key(request,after,allocator); defer delete(after_key,allocator)
    if after_key!=key { delete(artifact,allocator); return nil,.Busy }
    digest:=compiler_digest(artifact,allocator); defer delete(digest,allocator)
    record:=make([]byte,len(artifact)+130,allocator); defer delete(record,allocator)
    copy(record[:64],transmute([]byte)key); record[64]='\n'
    copy(record[65:129],transmute([]byte)digest); record[129]='\n'; copy(record[130:],artifact)
    if os.write_entire_file(output,record)!=nil || os.rename(output,cache)!=nil { delete(artifact,allocator); return nil,.Load_Failed }
    return artifact,.None
}
