//! Numeric transport keeps bounded integer fields exact without wrapping wider finite values.
package scene

import ron "../../encoding/ron"
import "core:encoding/json"
import "core:unicode/utf8"
import "core:strconv"
import "core:strings"

@(private="package")
bounded_scene_parse :: proc(data:[]byte,allocator:=context.allocator)->(json.Value,json.Error) {
    context.allocator=allocator
    text:=string(data); if len(data)>1<<20 || !utf8.valid_string(text) { return {},.Invalid_String }
    tokenizer:=json.make_tokenizer(text,.JSON,true); depth,last:=0,0
    builder:strings.Builder; changed:=false
    defer { if changed { strings.builder_destroy(&builder) } }
    for {
        token,err:=json.get_token(&tokenizer); if err!=nil && err!=.EOF { return {},err }
        #partial switch token.kind {
        case .Open_Brace,.Open_Bracket: depth+=1; if depth>64 { return {},.Unexpected_Token }
        case .Close_Brace,.Close_Bracket: depth-=1; if depth<0 { return {},.Unexpected_Token }
        case .Integer,.Float:
            number,valid:=strconv.parse_f64(token.text); if !valid || !(number>=-max(f64) && number<=max(f64)) { return {},.Invalid_Number }
            if token.kind==.Integer {
                digits:=token.text; limit:=u64(max(i64)); if len(digits)>0 && digits[0]=='-' { digits=digits[1:]; limit+=1 }
                integer,exact:=ron.decimal_u64(digits)
                if !exact || integer>limit {
                    if !changed { strings.builder_init(&builder,allocator); changed=true }
                    strings.write_string(&builder,text[last:token.offset]); strings.write_string(&builder,token.text); strings.write_string(&builder,".0")
                    last=token.offset+len(token.text)
                }
            }
        case .EOF: break
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return {},.Unexpected_Token }
    if changed { strings.write_string(&builder,text[last:]); text=strings.to_string(builder) }
    return json.parse(text,spec=.JSON,parse_integers=true,allocator=allocator)
}
