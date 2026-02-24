TODO:
---

  List from RHI_DESIGN document
  ┌──────────┬────────────────────────────┬───────────────────────────────────┐
  │ Priority │           Change           │              Impact               │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Descriptor Set Cache       │ Eliminates per-frame allocations  │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Unified Descriptor Builder │ Reduces code duplication          │ DONE
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Pipeline Cache             │ Faster lookups, better hot reload │ DONE
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Opaque Handles             │ Cleaner API, better encapsulation │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Per-Frame Command Pools    │ Better memory management          │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Low      │ Sampler Cache              │ Fewer sampler objects             │
  └──────────┴────────────────────────────┴───────────────────────────────────┘


Random issues:

- Particles (in katla_app) use materials wrong - we should have a unified material API
  - in order to get this working we should have custom descroptor layouts beyond PBR textures.
  - This would be good for UI code and fullscreen passes as well  

- Rendergraph often fail when we set up more/other passes. The transitions between imageviews(?) usually give validation errors when we tweak stuff.
  - I have a hunch that we're doing this setup too hard for ourselves.
  - We should make a strategy for robuster setup
  - We should allow multiple rendergraphs to be active so that we can get multi viewport&camera support, where we can show different scenes in different ui panels.
