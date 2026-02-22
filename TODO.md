TODO:
---

  ┌──────────┬────────────────────────────┬───────────────────────────────────┐
  │ Priority │           Change           │              Impact               │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Descriptor Set Cache       │ Eliminates per-frame allocations  │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Unified Descriptor Builder │ Reduces code duplication          │ DONE
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ High     │ Pipeline Cache             │ Faster lookups, better hot reload │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Opaque Handles             │ Cleaner API, better encapsulation │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Medium   │ Per-Frame Command Pools    │ Better memory management          │
  ├──────────┼────────────────────────────┼───────────────────────────────────┤
  │ Low      │ Sampler Cache              │ Fewer sampler objects             │
  └──────────┴────────────────────────────┴───────────────────────────────────┘


Random issues:

- Particles (in katla_app) use materials wrong - we should have a unified material API
  
