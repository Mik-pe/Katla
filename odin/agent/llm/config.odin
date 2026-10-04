//! Explicit provider configuration; credentials are resolved only at request time.
package llm

import "core:mem"
import "core:strings"
import "core:encoding/json"
import "core:os"
import "core:unicode/utf8"

/// Selects a provider without silently changing models or endpoints.
Provider :: enum { Disabled, OpenAI, OpenAI_Compatible }
/// Selects the provider wire contract explicitly.
API :: enum { Responses, Chat_Completions }
/// Owns configuration strings and stores credential references, never diagnostic text.
Config :: struct {
    provider:Provider,
    api:API,
    api_key:string `fmt:"-" json:"-"`,
    base_url,model:string,
    max_tokens:u32,
    temperature:f64,
    has_temperature:bool,
    interval_ms:u64,
    max_calls:u32,
    timeout_ms:u32,
    allocator:mem.Allocator `fmt:"-" json:"-"`,
}
/// Distinguishes local validation, transport and provider failures without exposing bodies or keys.
Error :: enum { None, Disabled, Config, Credentials, Network, Timeout, Cancelled, HTTP, Rate_Limited, Protocol, Limit, Truncated, Refused, Tool, Busy, Conversation_Limit }
/// Builds a disabled configuration which must be explicitly enabled before use.
config_default :: proc(allocator:=context.allocator)->Config {
    return Config{model=strings.clone("gpt-4o",allocator),max_tokens=4096,temperature=0.7,interval_ms=1000,max_calls=20,timeout_ms=120000,allocator=allocator}
}
/// Clones the complete provider choice so pending conversations do not borrow mutable settings.
config_clone :: proc(c:Config,allocator:=context.allocator)->Config {
    result:=c; result.allocator=allocator
    result.api_key=strings.clone(c.api_key,allocator); result.base_url=strings.clone(c.base_url,allocator); result.model=strings.clone(c.model,allocator)
    return result
}
/// Releases configuration strings using their captured allocator.
config_destroy :: proc(c:^Config) {
    for value in ([3]string{c.api_key,c.base_url,c.model}) { delete(value,c.allocator) }
    c^={}
}
@(private="package")
scalar_string :: proc(text:string,allocator:mem.Allocator)->(string,bool) {
    if len(text)<2 { return "",false }
    if text[0]=='\'' && text[len(text)-1]=='\'' {
        value:=text[1:len(text)-1]
        if strings.contains(value,"'") { return "",false }
        return strings.clone(value,allocator),true
    }
    if text[0]!='"' { return "",false }
    value,parsed:=parse_json(text,allocator)
    if !parsed { return "",false }; defer json.destroy_value(value,allocator)
    str,ok:=value.(string)
    if !ok { return "",false }
    return strings.clone(str,allocator),true
}
@(private="package")
uncomment :: proc(line:string)->string {
    quote:u8; escaped:=false
    for ch,i in transmute([]u8)line {
        if quote!=0 {
            if quote=='"' && escaped { escaped=false; continue }
            if quote=='"' && ch=='\\' { escaped=true; continue }
            if ch==quote { quote=0 }
        } else {
            if ch=='#' { return line[:i] }
            if ch=='"' || ch=='\'' { quote=ch }
        }
    }
    return line
}
/// Parses the flat llm.toml schema; unknown, duplicate or malformed fields fail atomically.
config_parse :: proc(data:string,allocator:=context.allocator)->(Config,Error) {
    if len(data)>65536 || !utf8.valid_string(data) || strings.contains(data,"\x00") { return {},.Config }
    c:=config_default(allocator); success:=false
    defer { if !success { config_destroy(&c) } }
    seen:=make(map[string]bool,allocator); defer delete(seen)
    api_explicit:=false
    lines:=strings.split(data,"\n",allocator); defer delete(lines,allocator)
    for raw in lines {
        line:=strings.trim_space(uncomment(raw)); if line=="" { continue }
        at:=strings.index_byte(line,'='); if at<1 { return {},.Config }
        key:=strings.trim_space(line[:at]); value:=strings.trim_space(line[at+1:])
        if seen[key] { return {},.Config }; seen[key]=true
        switch key {
        case "provider","api","api_key","base_url","model":
            str,valid:=scalar_string(value,allocator); if !valid { return {},.Config }
            switch key {
            case "provider":
                switch str {
                case "disabled": c.provider=.Disabled
                case "open_ai": c.provider=.OpenAI
                case "open_ai_compatible": c.provider=.OpenAI_Compatible
                case: delete(str,allocator); return {},.Config
                }
                delete(str,allocator)
            case "api":
                switch str {
                case "responses": c.api=.Responses
                case "chat_completions": c.api=.Chat_Completions
                case: delete(str,allocator); return {},.Config
                }
                api_explicit=true; delete(str,allocator)
            case "api_key": delete(c.api_key,allocator); c.api_key=str
            case "base_url": delete(c.base_url,allocator); c.base_url=str
            case "model": delete(c.model,allocator); c.model=str
            }
        case "max_tokens","rate_limit_min_interval_ms","rate_limit_max_calls_per_minute","timeout_ms":
            n,valid:=toml_uint(value); if !valid { return {},.Config }
            switch key {
            case "max_tokens": if n==0 || n>u64(max(u32)) { return {},.Config }; c.max_tokens=u32(n)
            case "rate_limit_min_interval_ms": if n>3600000 { return {},.Config }; c.interval_ms=n
            case "rate_limit_max_calls_per_minute": if n==0 || n>65536 { return {},.Config }; c.max_calls=u32(n)
            case "timeout_ms": if n==0 || n>3600000 { return {},.Config }; c.timeout_ms=u32(n)
            }
        case "temperature":
            n,valid:=toml_float(value); if !valid || !(n>=0 && n<=2) { return {},.Config }; c.temperature=n; c.has_temperature=true
        case: return {},.Config
        }
    }
    if !api_explicit { c.api=.Responses if c.provider==.OpenAI else .Chat_Completions }
    if config_validate(c)!=.None { return {},.Config }
    success=true; return c,.None
}
/// Checks the complete configuration before resolving credentials or opening a connection.
config_validate :: proc(c:Config)->Error {
    if c.max_tokens==0 || !(c.temperature>=0 && c.temperature<=2) || c.max_calls==0 || c.max_calls>65536 || c.timeout_ms==0 || c.interval_ms>3600000 { return .Config }
    if c.provider==.Disabled { return .None }
    if !utf8.valid_string(c.model) || !utf8.valid_string(c.base_url) || c.model=="" || len(c.model)>256 || c.api_key=="" || len(c.api_key)>8192 { return .Config }
    for ch in c.model { if ch<32 { return .Config } }
    if c.provider==.OpenAI && (c.base_url!="" || c.api!=.Responses) { return .Config }
    if c.provider==.OpenAI_Compatible && !endpoint_valid(c.base_url) { return .Config }
    return .None
}
@(private="package")
endpoint_valid :: proc(url:string)->bool {
    rest:string
    if strings.has_prefix(url,"https://") { rest=url[8:] }
    else if strings.has_prefix(url,"http://") { rest=url[7:] }
    else { return false }
    host:=rest; if i:=strings.index_byte(rest,'/'); i>=0 { host=rest[:i] }
    if host=="" { return false }
    for ch in url { if ch<=32 || ch=='@' || ch=='?' || ch=='#' || ch=='\\' { return false } }
    return true
}
/// Returns an owned key; callers must release it and must never log it.
config_resolve_key :: proc(c:Config)->(string,Error) {
    key:string
    if strings.has_prefix(c.api_key,"$") {
        name:=c.api_key[1:]; if name=="" { return "",.Credentials }
        for ch in name { if !(ch=='_' || ch>='A' && ch<='Z' || ch>='a' && ch<='z' || ch>='0' && ch<='9') { return "",.Credentials } }
        key=os.get_env(name,c.allocator)
    } else { key=strings.clone(c.api_key,c.allocator) }
    if key=="" { return "",.Credentials }
    for ch in key { if ch<=32 || ch>=127 { delete(key,c.allocator); return "",.Credentials } }
    return key,.None
}
