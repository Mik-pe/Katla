//! Preferences and docking storage retain an explicit configuration root and publish complete files.
package preferences
import resources "../../resources"
import "core:mem"
import "core:os"
import "core:strings"
import "core:encoding/json"

Store :: struct { root:resources.Root,allocator:mem.Allocator }
/// Resolves the same per-user config directory as the Rust editor.
config_directory :: proc(allocator:=context.allocator)->string {
    when ODIN_OS==.Windows {
        base:=os.get_env("APPDATA",allocator); defer delete(base,allocator)
        if base=="" { return "" }; return strings.concatenate({base,"/katla"},allocator)
    } else {
        base:=os.get_env("HOME",allocator); defer delete(base,allocator)
        if base=="" { return "" }
        when ODIN_OS==.Darwin { return strings.concatenate({base,"/Library/Application Support/katla"},allocator) }
        else {
            config:=os.get_env("XDG_CONFIG_HOME",allocator); defer delete(config,allocator)
            if config!="" { return strings.concatenate({config,"/katla"},allocator) }
            return strings.concatenate({base,"/.config/katla"},allocator)
        }
    }
}
/// Opens a real retained config directory; explicit directories support portable projects and tests.
store_init :: proc(store:^Store,directory:="",allocator:=context.allocator)->Error {
    path:=directory; allocated:=false
    if path=="" { path=config_directory(allocator); allocated=true }; defer { if allocated { delete(path,allocator) } }
    if path=="" { return .Invalid_Path }
    if error:=os.make_directory_all(path); error!=nil && error!=os.Error(.Exist) { return .IO }
    root,error:=resources.root_open(path,allocator); if error!=.None { return .IO }
    store^={root=root,allocator=allocator}; return .None
}
/// Releases the configuration handle once the shell and audio consumers have stopped using it.
store_destroy :: proc(store:^Store) { resources.root_destroy(&store.root); store^={} }
/// Returns defaults with an explicit diagnostic when preferences cannot be loaded or decoded.
load :: proc(store:^Store)->(Value,Error) {
    bytes,error:=resources.read_text(&store.root,"preferences.toml",1024*1024)
    if error!=.None { return defaults(store.allocator),.IO }; defer delete(bytes,store.allocator)
    value,decode_error:=decode(bytes,store.allocator)
    if decode_error!=.None { return defaults(store.allocator),decode_error }
    return value,.None
}
/// Validates live preferences and atomically publishes TOML only after complete encoding.
save :: proc(store:^Store,value:^Value)->(bool,Error) {
    validate(value)
    bytes,error:=encode(value,store.allocator); if error!=.None { return false,error }; defer delete(bytes,store.allocator)
    published,write_error:=resources.write_atomic(&store.root,"preferences.toml",bytes)
    if write_error!=.None { return published,.IO }; return published,.None
}
/// Loads bounded JSON for the shell's canonical transactional dock_restore validator.
dock_load :: proc(store:^Store)->([]byte,Error) {
    context.allocator=store.allocator
    bytes,error:=resources.read_text(&store.root,"dock-layout.json",1024*1024)
    if error!=.None { return nil,.IO }
    tree,parse_error:=json.parse(bytes,spec=.JSON,allocator=store.allocator)
    if parse_error!=nil { delete(bytes,store.allocator); return nil,.Invalid_Value }
    json.destroy_value(tree); return bytes,.None
}
/// Persists a docking snapshot after the shell validates panel IDs, duplicates and split structure.
dock_save :: proc(store:^Store,bytes:[]byte,state:rawptr,validator:proc(rawptr,[]byte)->bool)->(bool,Error) {
    if len(bytes)>1024*1024 || validator==nil || !validator(state,bytes) { return false,.Invalid_Value }
    published,error:=resources.write_atomic(&store.root,"dock-layout.json",bytes)
    if error!=.None { return published,.IO }; return published,.None
}
