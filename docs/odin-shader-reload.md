# Odin application shader reload

The editor watches the canonical source directory supplied by `--shader-root`.
The packaged directory is `<build>/shaders`, copied from
`odin/app/render/shaders` and hashed by the build manifest. Runtime compilation
uses the existing explicit offline compiler executable and content-addressed
artifact cache; the application does not link a shader compiler runtime.

`Shader_Reload_Service` belongs to the application owner thread. Initialize it
at a stationary address and register the editor's shader modules as one
aggregate family. Call `shader_reload_poll` once per monotonically increasing
owner frame, before acquiring any GPU frame token. A repeated frame number
does no work. The source loader caches each file's owned bytes for that poll,
resolves transitive local includes through the existing shader source resolver,
and hashes the expanded source, selected entries, override constants, native
options and compiler contents. Missing files, invalid UTF-8 or invalid includes
preserve the accepted family; restoring the source admits a new candidate.

Compiler file size, timestamps, inode and device are checked each poll; its
content digest is refreshed when those attributes change. Ordinary replacement
or rebuilding therefore invalidates the family. Deliberately rewriting the
same file while restoring all those attributes is outside this watcher contract.
The offline artifact cache independently reads and hashes the actual executable
before each compile and checks it again before accepting process output.

Changed modules use the existing bounded asynchronous `shader.Service` worker.
Unchanged requests can reuse its artifact cache without starting another process.
Unchanged source, options and override values do not republish pipelines.
Cancellation discards superseded results; a compiler process already accepted
by the worker can finish, and destruction joins that worker before freeing its
compiler or source configuration. The compiler and allocator must outlive the
service, and the allocator must support concurrent calls.

The publisher has three mandatory operations:

- `prepare` receives borrowed compiled artifacts and creates the complete
  candidate family. It may return `Busy` while a consumer has an authored frame,
  or return a partially created candidate with an error for cleanup. Any shader
  interface, compiler or native pipeline failure leaves every accepted owner
  unchanged.
- `publish` swaps the complete family before GPU frame acquisition. It allocates
  nothing and cannot fail. It updates live graph handles and stationary shader
  descriptor references used by future scene/model rebuilds, then returns the
  previous family for destruction.
- `destroy` releases unpublished or previous public handles and owned descriptor
  snapshots. Native submission records retain resources already accepted by the
  GPU; removing a public pipeline handle cannot invalidate pending commands.

Artifacts are borrowed only during `prepare`. Descriptors needed by later
scene/model rebuilding must own deep compiled snapshots, including reflection,
binaries, MSL and every string. They cannot point into service results, which
are released after publication or rejection. Forward and reverse depth variants
belong to the same candidate; shared handles must be destroyed only once.
The UI helper replaces draw/decode/encode pipelines while retaining atlas,
sampler and texture ownership. The picking helper replaces both depth senses
and all masked/cull variants without changing committed picking snapshots.

The app tests in `shader_reload_test.odin` use the actual offline executable:
transitive edits, overrides, no-change polling, bad WGSL, missing/invalid UTF-8,
changed/missing compiler identity, native preparation rejection and recovery.
Pass `-define:SHADER_COMPILER=<executable>` when running render tests. CPU tests
retain address and leak checks with `ASAN_OPTIONS=detect_leaks=1`.

`odin/app_render_shader_reload_native` accepts the compiler executable, Vulkan
loader and canonical shader directory as three positional arguments. On macOS
it runs both native Metal and Vulkan. Each backend verifies 4,608 exact rendered
pixels through real image-to-buffer readback, coherent publication of two scene
pipelines plus UI and picking pipeline families, retention of old pending
submissions, failed WGSL and actual missing-entry native pipeline rejection,
source repair, and cache reuse. Its native GPU process uses the explicit
external CF/Objective-C/driver leak boundary `ASAN_OPTIONS=detect_leaks=0`; Odin
allocation tracking still must be empty. Enable
`MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` and use an explicit
`VK_ICD_FILENAMES`. Vulkan synchronization validation must report zero errors.
This focused harness proves native pipeline lifetime and the service boundary;
production acceptance additionally exercises the aggregate publisher across
all four editor consumers and future scene rebuilding.
