//! Explicit browser filesystem actions use retained roots and never follow child links.
package resources
/// Creates one requested directory below the retained root; its parent must already exist.
create_directory :: proc(root:^Root,path:string)->Error { if root.file==nil || !valid_relative_path(path) { return .Invalid_Path }; return create_directory_native(root,path) }
/// Removes a regular file or, when recursive is true, a requested directory tree within the root.
remove_path :: proc(root:^Root,path:string,recursive:=false)->Error { if root.file==nil || !valid_relative_path(path) { return .Invalid_Path }; return remove_path_native(root,path,recursive) }
