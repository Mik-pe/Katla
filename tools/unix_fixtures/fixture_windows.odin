#+build windows
//! Unix transport acceptance is unavailable on Windows; never report it as passed.
package unix_fixtures
import "../wire"
Fixture :: struct { methods:[dynamic]string }
start :: proc(fixture:^Fixture,path,mode,scenario:string) { wire.require(false,"Unix host/proxy fixture requires Darwin or Linux") }
stop :: proc(fixture:^Fixture) { wire.require(false,"Unix host/proxy fixture requires Darwin or Linux") }
