package katla_icons

import "core:testing"
import "core:unicode/utf8"

@(test)
test_icon_catalogue :: proc(t: ^testing.T) {
    catalogue := ALL_ICONS
    for icon,i in catalogue {
        testing.expect(t,icon.codepoint >= 0xf000 && icon.codepoint <= 0xf8ff)
        for j in i+1..<len(catalogue) {
            other := catalogue[j]
            testing.expect(t,icon.name != other.name && icon.codepoint != other.codepoint)
        }
        bytes,count := utf8.encode_rune(icon.codepoint)
        decoded,width := utf8.decode_rune(bytes[:count])
        testing.expect(t,count == 3 && decoded == icon.codepoint && width == count)
    }
}
@(test)
test_common_icons :: proc(t: ^testing.T) {
    testing.expect(t,len(COMMON_ICONS)>0)
    for code in COMMON_ICONS {
        found := false
        for icon in ALL_ICONS { if code == icon.codepoint { found = true; break } }
        testing.expect(t,found)
    }
    testing.expect(t,CUBE == 0xf1b2 && PLAY == 0xf04b && SAVE == 0xf0c7)
}
