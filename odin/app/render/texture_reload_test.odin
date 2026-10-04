#+test
package render

import "core:testing"

@(test)
test_texture_freshness_observes_equal_size_byte_edits :: proc(t:^testing.T) {
    accepted:=[4]byte{1,2,3,4}; unchanged:=[4]byte{1,2,3,4}; edited:=[4]byte{1,2,0,4}
    testing.expect(t,texture_source_equal(accepted[:],unchanged[:]))
    testing.expect(t,!texture_source_equal(accepted[:],edited[:]))
    testing.expect(t,!texture_source_equal(accepted[:],accepted[:3]))
}
