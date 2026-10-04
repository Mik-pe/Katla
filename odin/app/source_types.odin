//! Nongeometry source identity preserves authored editor icons independently of optional components.
package app

/// Source markers carry no GPU allocation and do not imply the presence of optional components.
Scene_Builtin_Source_Kind :: enum { Light, ParticleEmitter, Trigger }
Scene_Builtin_Source :: struct { kind:Scene_Builtin_Source_Kind }
