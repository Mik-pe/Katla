//! Luau syntax spans use UTF8 byte offsets shared by native shaping and the retained text editor.
package editor_app
import ui "../../ui"
import "core:mem"

@(private="package")
code_letter :: proc(value:byte)->bool { return value>='a' && value<='z' || value>='A' && value<='Z' || value=='_' || value>=128 }
@(private="package")
code_digit :: proc(value:byte)->bool { return value>='0' && value<='9' }
@(private="package")
code_keyword :: proc(value:string)->bool {
    for word in ([]string{"and","break","continue","do","else","elseif","end","false","for","function","if","in","local","nil","not","or","repeat","return","then","true","until","while","type","export"}) { if value==word { return true } }; return false
}
@(private="package")
code_long_end :: proc(value:string,start:int)->(int,bool) {
    if start>=len(value) || value[start]!='[' { return start,false }
    open:=start+1; for open<len(value) && value[open]=='=' { open+=1 }; if open>=len(value) || value[open]!='[' { return start,false }
    count:=open-start-1; position:=open+1
    for position<len(value) {
        if value[position]==']' { next:=position+1; for next<len(value) && value[next]=='=' { next+=1 }; if next-position-1==count && next<len(value) && value[next]==']' { return next+1,true } }
        position+=1
    }; return len(value),true
}
/// Returns owned ordered nonoverlapping spans; unterminated strings/comments remain colored through EOF.
code_syntax :: proc(value:string,allocator:mem.Allocator)->[]ui.Text_Run {
    runs:=make([dynamic]ui.Text_Run,allocator); defer delete(runs)
    position:=0
    for position<len(value) {
        start:=position; color:ui.Color; colored:=false
        if value[position]=='-' && position+1<len(value) && value[position+1]=='-' {
            position+=2; stop,long:=code_long_end(value,position)
            if long { position=stop } else { for position<len(value) && value[position]!='\n' { position+=1 } }
            color={.5,.62,.48,1}; colored=true
        } else if value[position]=='\'' || value[position]=='"' {
            quote:=value[position]; position+=1
            for position<len(value) { current:=value[position]; position+=1; if current=='\\' && position<len(value) { position+=1 } else if current==quote || current=='\n' { break } }
            color={.88,.72,.44,1}; colored=true
        } else if stop,long:=code_long_end(value,position); long { position=stop; color={.88,.72,.44,1}; colored=true }
        else if code_letter(value[position]) {
            position+=1; for position<len(value) && (code_letter(value[position]) || code_digit(value[position])) { position+=1 }
            if code_keyword(value[start:position]) { color={.74,.6,.94,1}; colored=true }
        } else if code_digit(value[position]) {
            position+=1; for position<len(value) && (code_digit(value[position]) || value[position]=='.' || value[position]=='x' || value[position]=='X' || value[position]>='a' && value[position]<='f' || value[position]>='A' && value[position]<='F' || value[position]=='_') { position+=1 }
            color={.47,.79,.85,1}; colored=true
        } else { position+=1 }
        if colored { append(&runs,ui.Text_Run{start,position,color}) }
    }
    result:=make([]ui.Text_Run,len(runs),allocator); copy(result,runs[:]); return result
}
