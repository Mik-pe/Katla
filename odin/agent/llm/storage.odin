//! Configuration storage bounds reads and restricts secret-bearing files before any write.
package llm

import "core:os"
import "core:io"
import "core:strings"
import "core:fmt"

/// Loads one explicit path; a missing or invalid file never enables a fallback provider.
config_load :: proc(path:string,allocator:=context.allocator,missing_disabled:=false)->(Config,Error) {
    file,err:=os.open(path)
    if err==.Not_Exist && missing_disabled { return config_default(allocator),.None }
    if err!=nil { return {},.Config }; defer os.close(file)
    data:=make([]byte,65537,allocator); defer delete(data,allocator)
    count:=0
    for count<len(data) {
        n,read_error:=os.read(file,data[count:]); count+=n
        if read_error==io.Error.EOF || n==0 && read_error==nil { break }
        if read_error!=nil { return {},.Config }
    }
    if count>65536 { return {},.Config }
    return config_parse(string(data[:count]),allocator)
}
/// Saves atomically through a 0600 Unix temporary file; Windows uses its directory ACL.
/// Errors retain the prior configuration.
config_save :: proc(c:Config,path:string)->Error {
    if config_validate(c)!=.None { return .Config }
    b:strings.Builder; strings.builder_init(&b,c.allocator); defer strings.builder_destroy(&b)
    provider:="disabled"
    switch c.provider {
    case .Disabled:
    case .OpenAI: provider="open_ai"
    case .OpenAI_Compatible: provider="open_ai_compatible"
    }
    for field in ([5][2]string{{"provider",provider},{"api","responses" if c.api==.Responses else "chat_completions"},{"api_key",c.api_key},{"base_url",c.base_url},{"model",c.model}}) {
        if field[0]=="base_url" && field[1]=="" { continue }
        strings.write_string(&b,field[0]); strings.write_string(&b," = "); write_quote(&b,field[1],c.allocator); strings.write_string(&b,"\n")
    }
    numeric:=fmt.aprintf("max_tokens = %d\nrate_limit_min_interval_ms = %d\nrate_limit_max_calls_per_minute = %d\ntimeout_ms = %d\n",c.max_tokens,c.interval_ms,c.max_calls,c.timeout_ms,allocator=c.allocator)
    strings.write_string(&b,numeric); delete(numeric,c.allocator)
    if c.has_temperature { strings.write_string(&b,"temperature = "); strings.write_f64(&b,c.temperature,'g'); strings.write_string(&b,"\n") }
    file,err:=os.create_temp_file(os.dir(path),".katla-llm-*"); if err!=nil { return .Config }
    temp_path:=strings.clone(os.name(file),c.allocator); defer delete(temp_path,c.allocator)
    open:=true; moved:=false
    defer { if open { os.close(file) }; if !moved { os.remove(temp_path) } }
    if os.fchmod(file,{.Read_User,.Write_User})!=nil { return .Config }
    bytes:=transmute([]byte)strings.to_string(b); written:=0
    for written<len(bytes) {
        n,write_error:=os.write(file,bytes[written:]); if write_error!=nil || n==0 { return .Config }; written+=n
    }
    if os.sync(file)!=nil { return .Config }
    close_error:=os.close(file); open=false; if close_error!=nil { return .Config }
    if os.rename(temp_path,path)!=nil { return .Config }; moved=true
    return .None
}

/// Returns the standard per-user Katla configuration path without reading credentials.
config_user_path :: proc(allocator:=context.allocator)->(string,Error) {
    directory,err:=os.user_config_dir(allocator,roaming=true)
    if err!=nil { return "",.Config }; defer delete(directory,allocator)
    path,path_error:=os.join_path({directory,"katla","llm.toml"},allocator)
    if path_error!=nil { return "",.Config }; return path,.None
}
/// Missing per-user settings keep the provider disabled; malformed settings remain an explicit error.
config_load_user :: proc(allocator:=context.allocator)->(Config,Error) {
    path,err:=config_user_path(allocator); if err!=.None { return {},err }; defer delete(path,allocator)
    return config_load(path,allocator,missing_disabled=true)
}
/// Creates the per-user directory before atomically saving this explicit provider choice.
config_save_user :: proc(c:Config)->Error {
    path,err:=config_user_path(c.allocator); if err!=.None { return err }; defer delete(path,c.allocator)
    if os.make_directory_all(os.dir(path),{.Read_User,.Write_User,.Execute_User})!=nil { return .Config }
    return config_save(c,path)
}
