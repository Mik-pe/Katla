#+build windows
//! This fixture exercises Unix transport; Windows cannot claim Unix runtime acceptance.
package main
import "../wire"
main :: proc() { wire.require(false,"Native existing-host Unix fixture requires Darwin or Linux") }
