//! Opaque bindings to the pinned TOML parser; all input is supplied as bounded bytes.
package toml_native
import "core:c"
@(private)
LIB :: #config(TOML_LIBRARY,"../../../target/odin-toml/libtoml.a")
foreign import lib { LIB }
Type :: enum c.int { Other=-1,Missing,String,Number,Boolean,Table }
foreign lib {
    @(link_name="katla_toml_parse") parse :: proc(data:rawptr,length:c.int)->rawptr ---
    @(link_name="katla_toml_destroy") destroy :: proc(tree:rawptr) ---
    @(link_name="katla_toml_find") find :: proc(tree:rawptr,path:cstring,text:^cstring,length:^c.int,number:^f64)->Type ---
}
