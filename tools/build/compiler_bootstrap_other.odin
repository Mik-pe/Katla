#+build !darwin
//! Explicit host boundary for the macOS LLVM runtime bootstrap.
package katla_build

bootstrap_odin :: proc(llvm_config:string) {
    fail("Native LLVM 22 Odin bootstrap currently requires macOS")
}
