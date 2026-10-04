//! Bounded RON syntax preserves current asset tuples, objects and tagged variant identity.
package ron

import "core:encoding/json"
import "core:mem"
import "core:strings"

/// Reports a syntax/limit failure at its exact source byte.
Error_Kind :: enum { None, Syntax, Limit }
Error :: struct { kind:Error_Kind,offset:int }
@(private="package")
Parser :: struct { text:string,index,nodes:int,allocator:mem.Allocator,error:Error }
/// Parses a current asset document into owned values; named variants use __variant/__payload.
parse :: proc(text:string,allocator:=context.allocator)->(json.Value,Error) {
    if len(text)>64*1024*1024 { return nil,{.Limit,0} }
    context.allocator=allocator
    parser:=Parser{text=text,allocator=allocator}; value:=read_value(&parser,0)
    skip_space(&parser)
    if parser.error.kind==.None && parser.index!=len(text) { fail(&parser,.Syntax) }
    if parser.error.kind!=.None { json.destroy_value(value); return nil,parser.error }
    return value,{}
}
@(private="package")
fail :: proc(parser:^Parser,kind:Error_Kind) { if parser.error.kind==.None { parser.error={kind,parser.index} } }
@(private="package")
skip_space :: proc(parser:^Parser) {
    for parser.index<len(parser.text) {
        char:=parser.text[parser.index]
        if char==' ' || char=='\n' || char=='\t' || char=='\r' { parser.index+=1; continue }
        if strings.has_prefix(parser.text[parser.index:],"//") { for parser.index<len(parser.text) && parser.text[parser.index]!='\n' { parser.index+=1 }; continue }
        if strings.has_prefix(parser.text[parser.index:],"/*") {
            parser.index+=2; depth:=1
            for parser.index<len(parser.text) && depth>0 {
                if strings.has_prefix(parser.text[parser.index:],"/*") { depth+=1; parser.index+=2 }
                else if strings.has_prefix(parser.text[parser.index:],"*/") { depth-=1; parser.index+=2 }
                else { parser.index+=1 }
            }
            if depth!=0 { fail(parser,.Syntax) }; continue
        }
        if strings.has_prefix(parser.text[parser.index:],"#![") {
            closing:=strings.index(parser.text[parser.index:],"]")
            if closing<0 { fail(parser,.Syntax); return }
            header:=parser.text[parser.index:parser.index+closing+1]
            if header!="#![enable(unwrap_newtypes)]" && header!="#![enable(implicit_some)]" && header!="#![enable(unwrap_variant_newtypes)]" { fail(parser,.Syntax); return }
            parser.index+=closing+1; continue
        }
        break
    }
}
@(private="package")
read_string :: proc(parser:^Parser)->json.Value {
    start:=parser.index; parser.index+=1; escaped:=false
    for parser.index<len(parser.text) {
        char:=parser.text[parser.index]; parser.index+=1
        if char=='"' && !escaped {
            data:=transmute([]byte)parser.text[start:parser.index]
            value,err:=json.parse(data,spec=.JSON,allocator=parser.allocator)
            if err!=nil { fail(parser,.Syntax) }; return value
        }
        if char=='\\' && !escaped { escaped=true } else { escaped=false }
    }
    fail(parser,.Syntax); return nil
}
@(private="package")
read_identifier :: proc(parser:^Parser)->string {
    start:=parser.index
    for parser.index<len(parser.text) {
        char:=parser.text[parser.index]
        if !(char>='a' && char<='z') && !(char>='A' && char<='Z') && !(char>='0' && char<='9') && char!='_' { break }
        parser.index+=1
    }
    return parser.text[start:parser.index]
}
@(private="package")
read_value :: proc(parser:^Parser,depth:int)->json.Value {
    if depth>128 || parser.nodes>=8_000_000 { fail(parser,.Limit); return nil }
    parser.nodes+=1; skip_space(parser)
    if parser.error.kind!=.None || parser.index>=len(parser.text) { fail(parser,.Syntax); return nil }
    char:=parser.text[parser.index]
    if char=='"' { return read_string(parser) }
    if char=='(' || char=='[' || char=='{' { return read_container(parser,depth+1) }
    if char=='-' || char>='0' && char<='9' {
        start:=parser.index
        for parser.index<len(parser.text) {
            byte:=parser.text[parser.index]
            if !(byte>='0' && byte<='9') && byte!='-' && byte!='+' && byte!='.' && byte!='e' && byte!='E' { break }; parser.index+=1
        }
        data:=transmute([]byte)parser.text[start:parser.index]
        value,err:=json.parse(data,spec=.JSON,parse_integers=true,allocator=parser.allocator)
        if err!=nil { fail(parser,.Syntax) }; return value
    }
    identifier:=read_identifier(parser)
    if identifier=="" { fail(parser,.Syntax); return nil }
    if identifier=="true" { return true }; if identifier=="false" { return false }; if identifier=="None" { return json.Null{} }
    skip_space(parser)
    if identifier=="Some" {
        if parser.index>=len(parser.text) || parser.text[parser.index]!='(' { fail(parser,.Syntax); return nil }
        parser.index+=1; value:=read_value(parser,depth+1); skip_space(parser)
        if parser.index<len(parser.text) && parser.text[parser.index]==',' { parser.index+=1; skip_space(parser) }
        if parser.index>=len(parser.text) || parser.text[parser.index]!=')' { json.destroy_value(value); fail(parser,.Syntax); return nil }
        parser.index+=1; return value
    }
    variant:=make(json.Object,parser.allocator)
    variant[strings.clone("__variant",parser.allocator)]=strings.clone(identifier,parser.allocator)
    if parser.index<len(parser.text) && parser.text[parser.index]=='(' { variant[strings.clone("__payload",parser.allocator)]=read_container(parser,depth+1) }
    return variant
}
@(private="package")
read_container :: proc(parser:^Parser,depth:int)->json.Value {
    opening:=parser.text[parser.index]; parser.index+=1; skip_space(parser)
    closing:u8=')'; if opening=='[' { closing=']' }; if opening=='{' { closing='}' }
    object_mode:=opening=='{'
    if opening=='(' && parser.index<len(parser.text) {
        saved:=parser.index; empty:=parser.text[saved]==')'
        if parser.text[parser.index]=='"' { temporary:=read_string(parser); json.destroy_value(temporary) } else { read_identifier(parser) }
        skip_space(parser); object_mode=empty || (parser.index<len(parser.text) && parser.text[parser.index]==':'); parser.index=saved
    }
    object:json.Object; array:json.Array
    if object_mode { object=make(json.Object,parser.allocator) } else { array=make(json.Array,0,parser.allocator) }
    success:=false
    defer { if !success { if object_mode { json.destroy_value(json.Value(object)) } else { json.destroy_value(json.Value(array)) } } }
    for parser.error.kind==.None {
        skip_space(parser)
        if parser.index>=len(parser.text) { fail(parser,.Syntax); break }
        if parser.text[parser.index]==closing { parser.index+=1; success=true; if object_mode { return object } else { return array } }
        key:string
        if object_mode {
            if parser.text[parser.index]=='"' { value:=read_string(parser); text,ok:=value.(string); if !ok { fail(parser,.Syntax); break }; key=text }
            else { identifier:=read_identifier(parser); if identifier=="" { fail(parser,.Syntax); break }; key=strings.clone(identifier,parser.allocator) }
            skip_space(parser)
            if parser.index>=len(parser.text) || parser.text[parser.index]!=':' { delete(key,parser.allocator); fail(parser,.Syntax); break }; parser.index+=1
            if _,duplicate:=object[key]; duplicate { delete(key,parser.allocator); fail(parser,.Syntax); break }
        }
        value:=read_value(parser,depth)
        if object_mode { object[key]=value } else { append(&array,value) }
        skip_space(parser)
        if parser.error.kind!=.None { break }
        if parser.index<len(parser.text) && parser.text[parser.index]==',' { parser.index+=1; continue }
        if parser.index<len(parser.text) && parser.text[parser.index]==closing { continue }
        fail(parser,.Syntax)
    }
    return nil
}
