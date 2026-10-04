#+test
package window

import ui "../../ui"
import "core:testing"

@(test)
test_portable_unicode_scalar_indices_become_utf8_byte_offsets :: proc(t:^testing.T) {
    testing.expect_value(t,portable_text_byte("é😊",2),3)
    testing.expect_value(t,portable_text_byte("é😊",3),7)
    testing.expect_value(t,portable_text_byte("Åäö",2),4)
    testing.expect_value(t,portable_text_byte("Åäö",20),6)
    testing.expect_value(t,portable_text_byte("",0),0)
    testing.expect_value(t,portable_text_byte("字",-1),0)
}
@(test)
test_portable_physical_keys_and_modifiers_normalize :: proc(t:^testing.T) {
    testing.expect_value(t,portable_key(26),ui.Key.W)
    testing.expect_value(t,portable_key(4),ui.Key.A)
    testing.expect_value(t,portable_key(29),ui.Key.Z)
    testing.expect_value(t,portable_key(39),ui.Key.Num0)
    testing.expect_value(t,portable_key(30),ui.Key.Num1)
    testing.expect_value(t,portable_key(88),ui.Key.Enter)
    testing.expect_value(t,portable_key(80),ui.Key.Left)
    testing.expect_value(t,portable_key(511),ui.Key.None)
    testing.expect_value(t,portable_modifiers(15),ui.Modifiers{.Shift,.Control,.Alt,.Super})
}
