# katla_ui

Declarative retained-mode UI system on top of immediate-mode rendering core.

## Rules

- Use the **declarative API** (`declarative/` module). Implement `Build` trait, drive with `ViewTree::frame()`, drain actions from `ViewTree::actions_mut()`.
- The immediate-mode context (`context/` module) is for building custom widgets only. Don't use it directly otherwise.
- `TextureId` is opaque — UI crate has no GPU knowledge. katla_app maps TextureId → TextureHandle.
- Clipping uses per-command clip rects, not a scissor state machine.

## Conventions

- Layout is handled by Taffy (Flexbox). Widget trees are laid out before drawing.
- State is per-node via `BuildContext::state()`. It survives frames and auto-cleans on node removal.
- Actions are typed and drained each frame. Don't accumulate them across frames.
- Read `memory-bank/systemPatterns.md` for the full widget catalog and rendering pipeline.
