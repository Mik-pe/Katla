#+test
package app

import "core:testing"

@(test)
test_windows_asset_identity_normalizes_separators_and_drive_without_casefolding_names :: proc(t:^testing.T) {
    normalized:=asset_windows_identity(`c:\Katla\resources\scripts\Öak.luau`); defer delete(normalized)
    testing.expect_value(t,normalized,"C:/Katla/resources/scripts/Öak.luau")
    unix:=asset_windows_identity("/Katla/CaseSensitive/File"); defer delete(unix); testing.expect_value(t,unix,"/Katla/CaseSensitive/File")
    unc:=asset_windows_identity(`\\Host\Share\CaseSensitive\File`); defer delete(unc); testing.expect_value(t,unc,"//Host/Share/CaseSensitive/File")
}
