//! Host tools, source verification and atomic artifact receipts for the Odin build command.
package katla_build
import "core:os"
import "core:path/filepath"
import "core:fmt"
import "core:time"
import "core:strings"
import "core:crypto/sha2"
import "core:encoding/hex"
import "core:encoding/json"

root:string
output:string
cc,cxx,ar,odin,cargo:string
sanitize:bool
build_smoke:bool
report:string

fail :: proc(message:string)->! {
    for process in pending { _=os.process_kill(process); _,_=os.process_wait(process,os.TIMEOUT_INFINITE) }
    fmt.eprintfln("Katla: %s",message); os.exit(1)
}
require :: proc(ok:bool,message:string) { if !ok { fail(message) } }
join :: proc(parts:..string)->string { value,error:=filepath.join(parts); require(error==nil,"Invalid path"); return value }
absolute :: proc(path:string)->string {
    if filepath.is_abs(path) { value,error:=filepath.clean(path); require(error==nil,"Cannot normalize path"); return value }
    directory,error:=os.get_working_directory(context.allocator); require(error==nil,"Cannot resolve working directory")
    return join(directory,path)
}
mkdir :: proc(path:string) { if os.is_dir(path) { return }; error:=os.make_directory_all(path); require(error==nil,fmt.aprintf("Cannot create %s: %v",path,error)) }
read :: proc(path:string)->[]byte { value,error:=os.read_entire_file(path,context.allocator); require(error==nil,fmt.aprintf("Cannot read %s",path)); return value }
write :: proc(path,data:string) { mkdir(filepath.dir(path)); require(os.write_entire_file(path,data)==nil,fmt.aprintf("Cannot write %s",path)) }
copy_file :: proc(target,source:string) { mkdir(filepath.dir(target)); require(os.copy_file(target,source)==nil,fmt.aprintf("Cannot copy %s",source)) }
remove :: proc(path:string) { if os.exists(path) { error:=os.remove_all(path) if os.is_dir(path) else os.remove(path); require(error==nil,fmt.aprintf("Cannot remove %s: %v",path,error)) } }
executable_suffix :: proc()->string { when ODIN_OS==.Windows { return ".exe" } else { return "" } }
library_suffix :: proc()->string { when ODIN_OS==.Darwin { return ".dylib" } else when ODIN_OS==.Windows { return ".dll" } else { return ".so" } }
host :: proc()->string { when ODIN_OS==.Darwin { return "Darwin" } else when ODIN_OS==.Windows { return "Windows" } else when ODIN_OS==.Linux { return "Linux" } else { fail("Unsupported host") } }
architecture :: proc()->string { when ODIN_ARCH==.arm64 { return "arm64" } else when ODIN_ARCH==.amd64 { return "amd64" } else { fail("Unsupported architecture") } }
default_output :: proc()->string { return join(root,"target/katla-odin",cat(strings.to_lower(host()),"-",architecture()),"asan" if sanitize else "normal") }
env :: proc(key,fallback:string)->string { value:=os.get_env(key,context.allocator); return fallback if value=="" else value }
set_env :: proc(key,value:string) { require(os.set_env(key,value)==nil,"Cannot set child environment") }
find_tool :: proc(name:string)->string {
    if filepath.is_abs(name) || strings.contains_any(name,"/\\") { return name if filepath.is_abs(name) else join(root,name) }
    separator:=":"; when ODIN_OS==.Windows { separator=";" }
    for directory in strings.split(env("PATH",""),separator) {
        candidate:=join(directory,name); if os.is_file(candidate) { return candidate }
        when ODIN_OS==.Windows { candidate=cat(candidate,executable_suffix()); if os.is_file(candidate) { return candidate } }
    }
    fail(fmt.aprintf("Required tool unavailable: %s",name))
}
run_at :: proc(command:[]string,directory:string,timeout:=600*time.Second) {
    fmt.println("+",strings.join(command," "))
    process,error:=os.process_start({command=command,working_dir=directory,stdout=os.stdout,stderr=os.stderr,stdin=os.stdin})
    require(error==nil,fmt.aprintf("Cannot execute %s: %v",command[0],error))
    state,wait_error:=os.process_wait(process,timeout)
    if wait_error!=nil { _=os.process_kill(process); _,_=os.process_wait(process,os.TIMEOUT_INFINITE); fail(cat("Command deadline exceeded: ",command[0])) }
    require(wait_error==nil && state.exited && state.exit_code==0,fmt.aprintf("Command failed: %s (exit %d, wait %v)",command[0],state.exit_code,wait_error))
}
run :: proc(command:[]string) { run_at(command,root) }
capture :: proc(command:[]string)->string {
    state,out,err,error:=os.process_exec({command=command,working_dir=root},context.allocator)
    require(error==nil && state.exited && state.exit_code==0,fmt.aprintf("Command failed: %s\n%s",command[0],string(err)))
    return strings.trim_space(string(out))
}
digest :: proc(data:[]byte)->string { hash:sha2.Context_256; sha2.init_256(&hash); sha2.update(&hash,data); result:[32]byte; sha2.final(&hash,result[:]); return string(hex.encode(result[:])) }
verify :: proc(path,expected:string) { data:=read(path); defer delete(data); require(digest(data)==expected,fmt.aprintf("Pinned source checksum mismatch: %s",path)) }
download :: proc(url,path,checksum:string) {
    mkdir(filepath.dir(path))
    if !os.is_file(path) {
        temporary:=cat(path,".download"); remove(temporary)
        run({"curl","--fail","--location","--retry","3","--connect-timeout","30","--output",temporary,url})
        if checksum!="" { verify(temporary,checksum) }
        require(os.rename(temporary,path)==nil,"Cannot publish download")
    }
    if checksum!="" { verify(path,checksum) }
}
source :: proc(name,url,revision:string)->string {
    folder:=join(root,"target/native-source",name)
    if !os.is_dir(join(folder,".git")) {
        remove(folder); mkdir(folder)
        run({"git","init",folder}); run({"git","-C",folder,"config","core.autocrlf","false"}); run({"git","-C",folder,"config","core.eol","lf"}); run({"git","-C",folder,"fetch","--depth","1",url,revision}); run({"git","-C",folder,"checkout","--detach","FETCH_HEAD"})
    }
    require(capture({"git","-C",folder,"rev-parse","HEAD"})==revision,fmt.aprintf("Source revision mismatch: %s",name))
    require(capture({"git","-C",folder,"status","--porcelain"})=="",fmt.aprintf("Source checkout must be clean: %s",name))
    return folder
}
files :: proc(folder:string,suffix:string,recursive:=true)->[]string {
    result:[dynamic]string; walker:=os.walker_create(folder); defer os.walker_destroy(&walker)
    for info in os.walker_walk(&walker) {
        if info.type==.Directory { if !recursive && info.fullpath!=folder { os.walker_skip_dir(&walker) }; continue }
        require(info.type!=.Symlink,fmt.aprintf("Source/artifact cannot be a symlink: %s",info.fullpath))
        if info.type==.Regular && strings.has_suffix(info.name,suffix) { append(&result,strings.clone(info.fullpath)) }
    }
    _,error:=os.walker_error(&walker); require(error==nil,"Cannot enumerate sources")
    return result[:]
}
write_json :: proc(path:string,value:any) {
    data,error:=json.marshal(value,{pretty=true}); require(error==nil,"Cannot encode manifest")
    require(os.write_entire_file(path,data)==nil,"Cannot publish manifest")
}
native_flags :: proc()->[dynamic]string {
    flags:=make([dynamic]string); append(&flags,"-O1" if sanitize else "-O2","-g")
    when ODIN_OS!=.Windows { append(&flags,"-fPIC") }
    if sanitize { append(&flags,"-fsanitize=address","-fno-omit-frame-pointer") }
    return flags
}
pending: [dynamic]os.Process
finish_compiles :: proc() {
    succeeded:=true
    for process in pending {
        state,error:=os.process_wait(process,os.TIMEOUT_INFINITE)
        succeeded=succeeded && error==nil && state.exited && state.exit_code==0
    }
    clear(&pending); require(succeeded,"Native compilation failed")
}
compile :: proc(driver,source_file,object:string,flags:[]string) {
    if len(pending)==4 { finish_compiles() }
    mkdir(filepath.dir(object)); command:=make([dynamic]string); append(&command,driver); append(&command,..flags); append(&command,"-c",source_file,"-o",object)
    fmt.println("+",strings.join(command[:]," "))
    process,error:=os.process_start({command=command[:],working_dir=root,stdout=os.stdout,stderr=os.stderr})
    require(error==nil,"Cannot start native compiler"); append(&pending,process)
}

archive :: proc(path:string,objects:[]string) {
    finish_compiles()
    remove(path); command:=make([dynamic]string); append(&command,ar,"rcs",path); append(&command,..objects); run(command[:])
}
shared :: proc(driver,path:string,objects,flags:[]string) {
    finish_compiles()
    mkdir(filepath.dir(path)); command:=make([dynamic]string); append(&command,driver)
    append(&command,"-dynamiclib" if ODIN_OS==.Darwin else "-shared"); append(&command,..objects); append(&command,..flags); append(&command,"-o",path)
    when ODIN_OS==.Windows { append(&command,"-fuse-ld=lld") }
    run(command[:])
}
replace_once :: proc(text,before,after:string)->string { require(strings.count(text,before)==1,"Pinned source adaptation contract changed"); return replace(text,before,after) }

cut :: proc(text,separator:string)->(string,string,bool) {
    offset:=strings.index(text,separator); if offset<0 { return text,"",false }; return text[:offset],text[offset+len(separator):],true
}

cat :: proc(parts:..string)->string { return strings.concatenate(parts) }
replace :: proc(text,before,after:string)->string { value,_:=strings.replace_all(text,before,after); return value }

fields :: strings.fields
