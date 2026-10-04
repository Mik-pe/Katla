//! History compares object members independently of serialization order and keeps numeric lexemes exact.
package app

import "core:encoding/json"
import "core:mem"

@(private="package")
Scene_Action_Number :: distinct string
@(private="package")
Scene_Action_JSON :: union { json.Null,bool,string,Scene_Action_Number,[dynamic]Scene_Action_JSON,map[string]Scene_Action_JSON }

@(private="package")
scene_action_json_destroy :: proc(value:Scene_Action_JSON,allocator:mem.Allocator) {
    switch v in value {
    case string: delete(v,allocator)
    case [dynamic]Scene_Action_JSON: for child in v { scene_action_json_destroy(child,allocator) }; delete(v)
    case map[string]Scene_Action_JSON: for key,child in v { delete(key,allocator); scene_action_json_destroy(child,allocator) }; delete(v)
    case json.Null,bool,Scene_Action_Number:
    }
}
@(private="package")
scene_action_json_read :: proc(parser:^json.Parser)->(result:Scene_Action_JSON,valid:bool) {
    token:=parser.curr_token
    #partial switch token.kind {
    case .Null,.True,.False,.Integer,.Float:
        _,error:=json.advance_token(parser); if error!=nil { return {},false }
        #partial switch token.kind {
        case .Null: return json.Null{},true
        case .True: return true,true
        case .False: return false,true
        case: return Scene_Action_Number(token.text),true
        }
    case .String:
        value,error:=json.parse_value(parser); if error!=nil { return {},false }; return value.(string),true
    case .Open_Bracket:
        values:=make([dynamic]Scene_Action_JSON,parser.allocator)
        transferred:=false; defer { if !transferred { scene_action_json_destroy(Scene_Action_JSON(values),parser.allocator) } }
        _,error:=json.advance_token(parser); if error!=nil { return {},false }
        if parser.curr_token.kind!=.Close_Bracket { for {
            child,ok:=scene_action_json_read(parser); if !ok { return {},false }; append(&values,child)
            if parser.curr_token.kind==.Close_Bracket { break }
            if json.expect_token(parser,.Comma)!=nil || parser.curr_token.kind==.Close_Bracket { return {},false }
        } }
        if json.expect_token(parser,.Close_Bracket)!=nil { return {},false }
        transferred=true; return values,true
    case .Open_Brace:
        values:=make(map[string]Scene_Action_JSON,parser.allocator)
        transferred:=false; defer { if !transferred { scene_action_json_destroy(Scene_Action_JSON(values),parser.allocator) } }
        _,error:=json.advance_token(parser); if error!=nil { return {},false }
        if parser.curr_token.kind!=.Close_Brace { for {
            key,key_error:=json.parse_object_key(parser,parser.allocator); if key_error!=nil { return {},false }
            stored:=false; defer { if !stored { delete(key,parser.allocator) } }
            if key in values || json.expect_token(parser,.Colon)!=nil { return {},false }
            child,ok:=scene_action_json_read(parser); if !ok { return {},false }; values[key]=child; stored=true
            if parser.curr_token.kind==.Close_Brace { break }
            if json.expect_token(parser,.Comma)!=nil || parser.curr_token.kind==.Close_Brace { return {},false }
        } }
        if json.expect_token(parser,.Close_Brace)!=nil { return {},false }
        transferred=true; return values,true
    case: return {},false
    }
}
@(private="package")
scene_action_json_values_equal :: proc(a,b:Scene_Action_JSON)->bool {
    switch left in a {
    case json.Null: _,ok:=b.(json.Null); return ok
    case bool: right,ok:=b.(bool); return ok && left==right
    case string: right,ok:=b.(string); return ok && left==right
    case Scene_Action_Number: right,ok:=b.(Scene_Action_Number); return ok && left==right
    case [dynamic]Scene_Action_JSON:
        right,ok:=b.([dynamic]Scene_Action_JSON); if !ok || len(left)!=len(right) { return false }
        for child,i in left { if !scene_action_json_values_equal(child,right[i]) { return false } }; return true
    case map[string]Scene_Action_JSON:
        right,ok:=b.(map[string]Scene_Action_JSON); if !ok || len(left)!=len(right) { return false }
        for key,child in left { other,exists:=right[key]; if !exists || !scene_action_json_values_equal(child,other) { return false } }; return true
    }
    return false
}
@(private="package")
scene_action_json_equal :: proc(a,b:[]byte,allocator:mem.Allocator)->bool {
    left_parser:=json.make_parser(a,spec=.JSON,parse_integers=true,allocator=allocator)
    left,left_ok:=scene_action_json_read(&left_parser); defer scene_action_json_destroy(left,allocator)
    right_parser:=json.make_parser(b,spec=.JSON,parse_integers=true,allocator=allocator)
    right,right_ok:=scene_action_json_read(&right_parser); defer scene_action_json_destroy(right,allocator)
    return left_ok && right_ok && left_parser.curr_token.kind==.EOF && right_parser.curr_token.kind==.EOF && scene_action_json_values_equal(left,right)
}
