//! Numeric configuration parsing rejects overflow before arithmetic can normalize it.
package llm

import "core:strconv"

@(private="package")
checked_decimal :: proc(text:string)->(u64,bool) {
    if len(text)==0 { return 0,false }
    value:u64
    for ch in text {
        if ch<'0' || ch>'9' { return 0,false }
        digit:=u64(ch-'0')
        if value>(max(u64)-digit)/10 { return 0,false }
        value=value*10+digit
    }
    return value,true
}
@(private="package")
toml_uint :: proc(input:string)->(u64,bool) {
    text:=input
    if text=="-0" { return 0,true }
    signed:=false
    if len(text)>0 && text[0]=='+' { text=text[1:]; signed=true }
    if len(text)==0 { return 0,false }
    base:=u64(10)
    if len(text)>=2 && text[0]=='0' {
        switch text[1] {
        case 'x': base=16
        case 'o': base=8
        case 'b': base=2
        }
        if base!=10 { if signed { return 0,false }; text=text[2:] }
    }
    value:u64; digits:=0; separated:=true; leading_zero:=len(text)>0 && text[0]=='0'
    for ch in text {
        if ch=='_' { if separated { return 0,false }; separated=true; continue }
        digit:u64
        if ch>='0' && ch<='9' { digit=u64(ch-'0') }
        else if ch>='a' && ch<='f' { digit=u64(ch-'a')+10 }
        else if ch>='A' && ch<='F' { digit=u64(ch-'A')+10 }
        else { return 0,false }
        if digit>=base || value>(u64(max(i64))-digit)/base { return 0,false }
        value=value*base+digit; digits+=1; separated=false
        if base==10 && leading_zero && digits>1 { return 0,false }
    }
    return value,digits>0 && !separated
}
@(private="package")
toml_float :: proc(input:string)->(f64,bool) {
    text:=input
    if len(text)>0 && text[0]=='+' { text=text[1:] }
    clean:=make([]byte,len(text)); defer delete(clean); count:=0
    for ch,i in transmute([]byte)text {
        if ch=='_' {
            if i==0 || i+1==len(text) || text[i-1]<'0' || text[i-1]>'9' || text[i+1]<'0' || text[i+1]>'9' { return 0,false }
            continue
        }
        clean[count]=ch; count+=1
    }
    value:=string(clean[:count]); if !json_number_valid(value) { return 0,false }
    return strconv.parse_f64(value)
}
