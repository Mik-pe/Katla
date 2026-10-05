# Visible image thumbnails

The asset browser requests thumbnails only for mounted image rows intersecting
the retained UI clip. The editor bridges those requests to the stationary
[thumbnail cache](../odin/app/render/thumbnail_native.odin) after retiring its
combined GPU submission and before acquiring another frame. Browser state stores
opaque `Texture_Id` values, dimensions and loading/failure status; it owns no
GPU handle.

A cache key combines a retained root generation and confined relative path.
The browser supplies a freshness epoch on refresh and periodic source polls.
Periodic polls wait for visible pending jobs; an explicit refresh still
supersedes them. This prevents a slow decode from being perpetually invalidated
by its own polling interval.
The cache queues at most four background jobs. Each job clones the actual
directory capability, reads bounded encoded bytes and decodes/downsizes through
the canonical image decoder. Closing the original root does not invalidate an
active job. Jobs own their results; no worker touches the renderer or UI registry.

The application thread drains completed jobs. Superseded or no-longer-visible
results are discarded. SHA-256 comparison keeps an unchanged native image across
freshness polls. Actual GPU uploads and UI registration precede publication of
the accepted revision. A read, decode or native upload failure retains the
previous accepted image; failed epochs are retried only after a freshness change.
Native `Busy` keeps the completed decoded job for a later frame.

The default cache holds at most 128 images with a maximum edge of 128 pixels.
Eviction selects an older, nonvisible entry without an active job. UI composition
imports only textures in its immutable prepared frame, so cache lifetime does
not append persistent graph resources. Replaced or removed public handles remain
owned by already accepted native execution. Teardown joins jobs, unregisters
images and releases allocations before the borrowed UI owner is destroyed.

The [native fixture](../odin/examples/thumbnails_native/main_darwin.odin) checks
four-job admission, retained root lifetime, unchanged-byte deduplication, stale
worker rejection, root identity, bounded eviction, decode/native failure keeping
exact live UI pixels, replacement colors, and retained capture after complete
cache removal. It runs real UI triangle composition and readback on both native
adapters, with address checks and zero tracked allocation residue.

```sh
odin run tools/build -- validate ui \
  --build-manifest /absolute/path/to/asan/build.json --sanitize \
  --backend both --vulkan-loader /absolute/path/to/libvulkan.dylib \
  --vulkan-icd /absolute/path/to/MoltenVK_icd.json
```

The manifest selects matching font, image and offline shader dependencies.
CPU leak detection stays enabled; native GPU acceptance explicitly excludes
external framework exit leak checking while retaining address safety checks.
Model preview rendering is separate from these image thumbnails.
