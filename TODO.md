TODO:
---

  List from RHI_DESIGN document
  ┌──────────┬────────────────────────────┬───────────────────────────────────┐
  │ Priority │           Change           │              Impact               │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Descriptor Set Cache       │ Eliminates per-frame allocations  │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Unified Descriptor Builder │ Reduces code duplication          │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Pipeline Cache             │ Faster lookups, better hot reload │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Opaque Handles             │ Cleaner API, better encapsulation │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Per-Frame Command Pools    │ Better memory management          │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Low      │ Sampler Cache              │ Fewer sampler objects             │
  └──────────┴────────────────────────────┴───────────────────────────────────┘


To implement:
- Rendergraph
  - Draw commands
- Materials
- TextureManager
  - Opaque handles

Random issues:

- Particles (in katla_app) use materials wrong - we should have a unified material API
  - in order to get this working we should have custom descroptor layouts beyond PBR textures.
  - This would be good for UI code and fullscreen passes as well
