# Direct Luau dependency

`python3 scripts/build_odin_luau.py` retrieves Luau 0.709 at
`b968ef742741bb2b703afc3b3c53f06608c87481`, verifies the source revision,
and compiles its Common, Ast, Compiler and VM targets. Source comes from the
upstream repository, independently of Cargo or a preinstalled Lua runtime.
`--sanitize` builds C++ with the same LLVM AddressSanitizer runtime as Odin.
`--output <file>` selects an isolated dylib, .so or Windows DLL; mode-specific
objects remain under that file's parent. Windows uses Clang C++17, lld and
an installed Windows SDK; native Windows execution requires a Windows host.

The ABI2 C facade owns the allocator, compiler, protected calls and thread guard.
Every allocating or metamethod-capable stack operation catches Lua failures
before returning to Odin. Callback failures remain pending until Odin returns
and finishes its defers, then the native trampoline raises the Lua error. Host
operations consume the pending error before restoring their stack. A cached
native error string remains available when the VM cannot allocate.
The Odin dependency package loads the complete ABI before constructing a VM.
Odin `script` owns math/entity userdata, immutable snapshots, instances,
lifecycle, subscriptions, variables, reload, diagnostics and deferred commands.
Application scene, physics, input, audio and renderer authority stays in `app`.

A VM permits 128 MiB of native allocations. Each outer protected call permits
10 million VM interrupt safe points and five seconds of wall time; nested
host calls share that budget. Ten consecutive failed ticks disable an instance.
Native userdata metatables are protected and immutable. Instance environments
inherit sandboxed read-only globals; debug, filesystem, package, require and
process/environment functions are absent. No exception or Lua error crosses an
active Odin callback: callbacks return an error result before the C trampoline
raises it. All access, reset and destruction require the creating thread.

Run real language/runtime tests with:

```sh
python3 scripts/build_odin_luau.py --sanitize
odin test odin/script -all-packages -vet -strict-style -sanitize:address \
  -define:LUAU_LIBRARY=target/libkatla_luau_asan.dylib
```

These execute typed Luau, native userdata, exact u64 entities, retained-proxy
expiry, event payloads and ordering, hook rollback, ten-error disabling,
atomic environment replacement, scalar inspection/editing/reload, logs,
execution-budget interruption and recovery, real 128 MiB exhaustion during
Odin callbacks, and throwing or looping host environment lookups. Native and Odin allocators must
both return to zero on teardown. Application tests additionally select real
Box3D and run collision events through Luau, animation/particle commands and
Play/Pause/Resume/Stop with fresh restored entity generations.

The canonical editor initializes the direct application owner using
`script_native_init`. The superseded Rust scene bridge is not part of that
initialization path. Generic command packets alone do not establish application
feature completion. The app owns actual
inspector variables, forced reload, logs, native ray/force/velocity/audio queries
and commands, with immutable snapshots and next-tick feedback.

Bare resource names normalize below `scripts/` with a `.luau` extension. Installed
Resource and Project roots retain relative identity. Explicit File descriptors
from selected scenes, prefabs or files receive exact parent-and-basename
capabilities through scene preparation; failed publication revokes new scopes.
Reads and reloads reuse these retained capabilities and reject symlink children,
parent escapes, invalid UTF-8 and sources above 1 MiB. Editor byte validation
uses the real sandbox and does not replace an existing instance on failure.

Runtime destruction prepares renderer membership and native physics removal
before retiring ECS generations. Constraints referencing removed participants
retire together. Surviving authored trigger references remain stale during Play
and produce per-action diagnostics; Stop restores the authored snapshot with
fresh mapped entity references. Editing deletion instead updates durable
references through the shared authored undo transaction.
