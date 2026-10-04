//! Portable SDL key and Unicode offsets normalize input without native OS ownership.
package window

import ui "../../ui"

@(private="package")
portable_key :: proc(code:u32)->ui.Key {
    if code>=4 && code<=29 { return ui.Key(int(ui.Key.A)+int(code)-4) }
    if code>=30 && code<=38 { return ui.Key(int(ui.Key.Num1)+int(code)-30) }
    switch code {
    case 39:return .Num0
    case 40,88:return .Enter
    case 41:return .Escape
    case 42:return .Backspace
    case 43:return .Tab
    case 44:return .Space
    case 45:return .Minus
    case 46:return .Equal
    case 47:return .Left_Bracket
    case 48:return .Right_Bracket
    case 49,100:return .Backslash
    case 51:return .Semicolon
    case 52:return .Quote
    case 53:return .Backtick
    case 54:return .Comma
    case 55:return .Period
    case 56:return .Slash
    case 74:return .Home
    case 75:return .Page_Up
    case 76:return .Delete
    case 77:return .End
    case 78:return .Page_Down
    case 79:return .Right
    case 80:return .Left
    case 81:return .Down
    case 82:return .Up
    }
    return .None
}
@(private="package")
portable_modifiers :: proc(flags:u32)->ui.Modifiers {
    result:ui.Modifiers;if flags&1!=0 { result|={.Shift} };if flags&2!=0 { result|={.Control} };if flags&4!=0 { result|={.Alt} };if flags&8!=0 { result|={.Super} };return result
}
@(private="package")
portable_text_byte :: proc(text:string,characters:int)->int {
    if characters<=0 { return 0 };count:=0
    for _,offset in text { if count==characters { return offset };count+=1 }
    return len(text)
}
