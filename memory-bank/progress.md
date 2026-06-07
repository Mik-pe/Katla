# Progress

What's been done and what's next. **Update this file when completing or starting work. Remove completed items to keep this lean.**

## Completed Recently

- Default scene physics entities + active on load: ground static box, 10 dynamic spheres (top 2 grid rows), dynamic cube, dynamic cyan sphere, dynamic magenta cylinder. PhysicsActive(true) at builder init for both headless and windowed.
- UI polish round 10: hierarchy text color (#8E8E93), row padding (29px height), vstack spacing
- UI polish round 11 (overnight R2): DraggablePanel and Modal — 10px rounded corners, zero borders, full-screen popup_shadow backdrop, removed crude drag handle lines from title bar. RCP selectable_selected brightened to #3A3A3C, hovered to #484848.
- UI polish round 12 (overnight R3 final): panel bg #252527→#2C2C2E (matches spec, better canvas/panel contrast), inspector padding 8→12px, inspector section headers 11→12px (FontSize::Medium), hierarchy panel padding 4→8px spacing 4→6px. Vision score: 7/10 — all spec compliance issues resolved. Remaining gaps are structural (empty states, panel dividers, asset browser sizing).
- UI polish round 13 (Droid duty): Modal backdrop alpha 0.5→0.7 for clear visibility. Theme grid now shows ✓ check mark on selected theme. Inspector sections (Transform, Type, Audio Source, Audio Listener) now collapsible with chevron indicators via section() widget. selectable_selected alpha 0.18→0.30.
- SceneSnapshot physics bug fixed: spawn_from_descriptor now restores RigidBody, ColliderShape, PhysicsMaterial, TriggerVolume, CollisionFilter — physics bodies survive play/stop cycles.
- Default scene path fix: added default_scene_path() resolving via CARGO_MANIFEST_DIR, eliminating test/runtime path drift. Both now target workspace-root assets/scenes/default.katla.
- Code health pass: cargo fmt/clippy/test all clean.
- Pushed 49 commits to origin/main.

## In Progress

(Nothing in progress — add entries when work begins.)

## Upcoming / Blocked

- Preferences theme grid: consider orange accent tint in addition to check mark for even more prominence.
- Asset browser sizing still needs work (structural gap from vision score 7/10).
- Empty states for panels (hierarchy, inspector with no selection).
- Panel dividers between docked panels.

<!-- Things planned but not started. Remove when started (move to In Progress). -->
