//! Validate raw unsigned tokens before floating material parsing can round them.
package agent

import "core:encoding/json"
import "core:strconv"
import "core:unicode/utf8"

@(private="package")
material_arguments_validate :: proc(data:[]byte,allocator:=context.allocator)->(u64,Call_Error) {
    context.allocator=allocator
    if len(data)>1<<20 || !utf8.valid_string(string(data)) { return 0,.Invalid_JSON }
    tokenizer:=json.make_tokenizer(string(data),.JSON,true)
    depth:=0; key:=""; previous_string,integer_value:bool; image_index:u64
    for {
        token,error:=json.get_token(&tokenizer); if error!=nil && error!=.EOF { return 0,.Invalid_JSON }
        if integer_value {
            integer_value=false
            if token.kind!=.Null {
                if token.kind!=.Integer { return 0,.Invalid_Arguments }
                number,valid:=parse_entity_id(token.text); if !valid { return 0,.Invalid_Arguments }
                if key=="image_index" { image_index=u64(number) }
            }
        }
        #partial switch token.kind {
        case .String:
            value,parse_error:=json.parse(token.text,spec=.JSON,allocator=allocator); if parse_error!=nil { return 0,.Invalid_JSON }
            text,is_text:=value.(string); key=""; if is_text && (text=="tex_coord" || text=="anisotropy" || text=="image_index") { key=text }
            // Only fixed constant spellings survive destruction of the temporary parsed string.
            if key=="tex_coord" { key="tex_coord" } else if key=="anisotropy" { key="anisotropy" } else if key=="image_index" { key="image_index" }
            json.destroy_value(value); previous_string=true
        case .Colon: integer_value=previous_string && key!=""; previous_string=false
        case .Open_Brace,.Open_Bracket: depth+=1; if depth>64 { return 0,.Invalid_JSON }; previous_string=false
        case .Close_Brace,.Close_Bracket: depth-=1; if depth<0 { return 0,.Invalid_JSON }; previous_string=false
        case .Integer,.Float:
            number,valid:=strconv.parse_f64(token.text); if !valid || !(number>=-max(f64) && number<=max(f64)) { return 0,.Invalid_JSON }; previous_string=false
        case .EOF: break
        case: previous_string=false
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return 0,.Invalid_JSON }; return image_index,.None
}
