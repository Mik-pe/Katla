//! Query limits retain exact unsigned parsing while spatial numbers use finite floating point.
package agent

import "core:encoding/json"
import "core:unicode/utf8"
import "core:strconv"

@(private="package")
query_arguments_validate :: proc(arguments:[]byte,allocator:=context.allocator)->(int,Call_Error) {
    context.allocator=allocator
    if len(arguments)>1<<20 || !utf8.valid_string(string(arguments)) { return 0,.Invalid_JSON }
    tokenizer:=json.make_tokenizer(string(arguments),.JSON,true)
    depth:=0; limit:=64; key_expected,limit_key,value_expected:bool
    for {
        token,err:=json.get_token(&tokenizer); if err!=nil && err!=.EOF { return 0,.Invalid_JSON }
        if depth==1 && value_expected && token.kind!=.Colon {
            if limit_key && token.kind!=.Null {
                if token.kind!=.Integer { return 0,.Invalid_Arguments }
                value,valid:=parse_entity_id(token.text); if !valid { return 0,.Invalid_Arguments }
                limit=int(clamp(u64(value),1,256))
            }
            value_expected=false
        }
        #partial switch token.kind {
        case .Open_Brace,.Open_Bracket:
            depth+=1; if depth>64 { return 0,.Invalid_JSON }; if depth==1 { key_expected=true }
        case .Close_Brace,.Close_Bracket:
            depth-=1; if depth<0 { return 0,.Invalid_JSON }
        case .String:
            if depth==1 && key_expected {
                key,key_error:=json.parse(token.text,spec=.JSON,allocator=allocator)
                if key_error!=nil { return 0,.Invalid_JSON }
                text,is_string:=key.(string); limit_key=is_string && text=="limit"; json.destroy_value(key); key_expected=false
            }
        case .Colon: if depth==1 { value_expected=true }
        case .Comma: if depth==1 { key_expected=true }
        case .Integer,.Float:
            value,valid:=strconv.parse_f64(token.text); if !valid || !(value>=-max(f64) && value<=max(f64)) { return 0,.Invalid_JSON }
        case .EOF: break
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return 0,.Invalid_JSON }
    return limit,.None
}
