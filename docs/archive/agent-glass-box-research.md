# AI Co-Creator — Content Generation with Glass Box Transparency — Research Report

> Investigation completed April 2026. This document captures findings from researching 6 topic areas
> for building an AI co-creator in the Katla engine: an AI that helps with world building, parameter
> tuning, and game logic iteration inside the editor. Every AI action is visible, inspectable, and
> undoable. See TODO.md "AI Co-Creator" section for the actionable items derived from this research.

---

## Table of Contents

1. [Agent Harness Architecture](#1-agent-harness-architecture)
2. [Built-in LLM Assistant for the Editor](#2-built-in-llm-assistant-for-the-editor)
3. [Agent Observability and Glass Box UI](#3-agent-observability-and-glass-box-ui)
4. [Multithreading Approach for the Engine](#4-multithreading-approach-for-the-engine)
5. [Game Loop Modes (Edit/Play/Simulate)](#5-game-loop-modes-editplaysimulate)
6. [Component Inspection / Reflection API](#6-component-inspection--reflection-api)

---

## 1. Agent Harness Architecture

### Summary

Building a first-class agent system for Katla requires a harness that provides observation (reading ECS world state), action (spawning entities, modifying components, controlling simulation), perception (receiving results), and iteration (looping based on outcomes). The architecture draws from Unity ML-Agents' Agent class pattern (CollectObservations → Decide → OnActionReceived loop), MCP (Model Context Protocol) tool definitions for structured action APIs, and the command pattern for undo/redo integration.

The harness should be a new workspace crate (`katla_agent`) that defines agent traits, tool definitions, an execution context, and an action log. Agents run on a background thread (via tokio or crossbeam channels) and submit actions to the main thread for execution. Each action is wrapped as a `Command` object for undo/redo. The LLM assistant and other AI features consume this harness as their execution backend.

### Key Findings

**Observation/Action API patterns:**
- Unity ML-Agents: Agent class with `CollectObservations()` (serialize state to observation buffer), `OnActionReceived()` (execute decided actions), `Heuristic()` (human fallback). Academy singleton manages episode lifecycle.
- MCP (Model Context Protocol): JSON Schema-based tool definitions with `name`, `description`, `inputSchema`. Tools are discovered via `tools/list` and invoked via `tools/call`. Supports structured content responses. Official Rust SDK (`rmcp`) exists.
- OpenAI/Anthropic function calling: Both use JSON Schema for parameter definitions. OpenAI's `functions` parameter; Anthropic's `tool_use` content blocks. Katla should define tools using a schema that maps to both.

**Agent-ECS interaction:**
- Agents need: `query_entities(filter)`, `get_component::<T>(entity)`, `spawn_entity(components)`, `destroy_entity(entity)`, `set_component(entity, data)`.
- All mutations go through a `Command` pattern for undo/redo.
- Observation serializes relevant ECS state (selected entity + components, nearby entities, hierarchy) to structured JSON.

**Async execution:**
- Run agents on a background thread with a tokio runtime (or crossbeam for lighter weight).
- Communication: `mpsc` channel for action submission (agent → main thread), `oneshot` for results (main thread → agent).
- Main thread processes pending actions each frame before rendering.
- Agent yields between actions so the render loop never blocks.

**Undo/redo:**
- Each agent action = a `Command` with `execute()` and `undo()`.
- All actions from one agent "turn" are grouped into an `UndoGroup` (composite command).
- Single Ctrl+Z undoes the entire agent turn.

**Sandboxing:**
- Agent actions go through a validation layer (reject invalid entity IDs, out-of-range values, destructive operations on protected entities).
- Rate limiting: cap actions per frame to prevent agent from overwhelming the engine.

### Relevant Libraries & Projects

| Project | Relevance |
|---------|-----------|
| Unity ML-Agents | Agent class API pattern, episode lifecycle |
| MCP (Model Context Protocol) | Tool definition schema, `rmcp` Rust SDK |
| `rmcp` crate (1.3+) | Official Rust MCP SDK for building servers/clients |
| `rig` crate | Rust LLM framework with agent builder and tool calling |
| OpenAI function calling | JSON Schema tool definition format |
| `undo_2` / `redo` crates | Rust command pattern implementations |
| BehaviorTree-cpp | Behavior tree patterns for structured agent logic |

### Gotchas

- Don't let agents directly mutate ECS world — route all mutations through the main thread via commands.
- Agent actions must be rate-limited; an unbounded agent can starve the render loop.
- Entity IDs change between world clones (edit/play mode); agents must work with stable entity references or be re-bound on mode transitions.
- Don't store full ECS world per checkpoint — store component diffs.

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.

---

## 2. Built-in LLM Assistant for the Editor

### Summary

An in-editor LLM assistant is feasible using existing Rust crates, with a backend-agnostic `LlmProvider` trait abstracting over API-based providers (OpenAI, Mistral) and local inference (llama.cpp, mistral.rs). The recommended starting point is API-based (`async-openai` crate) for best quality-to-effort ratio, with local inference as a sub-feature for offline use. The UI is a chat panel built with the existing `katla_ui` `DraggablePanel` widget. Engine state (selected entities, component data, hierarchy) serializes via serde to structured JSON context. The entire feature is gated behind `#[cfg(feature = "llm-assistant")]`.

For external tool integration, the engine can expose an MCP server using the official `rmcp` Rust SDK, allowing external AI agents (like Claude Code, Cursor) to query and modify the scene directly via stdio transport.

### Key Findings

**Rust LLM ecosystem:**
- `async-openai` — Most mature Rust OpenAI API client. Streaming, function calling, embeddings. Works with any OpenAI-compatible endpoint.
- `llama-cpp-2` — Rust bindings to llama.cpp. GGUF models, GPU acceleration (CUDA/Metal/Vulkan). Requires C++ toolchain.
- `mistralrs` (0.7+) — Pure Rust inference engine. GGUF, Safetensors, ISQ quantization. Embeddable library + OpenAI-compatible server mode.
- `candle-transformers` — HuggingFace Rust ML framework. Good for small models, less production-ready for large LLMs.
- `ort` — ONNX Runtime Rust bindings. Best for specialized small models, not large autoregressive LLMs.

**Context serialization:**
- Use serde to convert ECS components to JSON for LLM context.
- Serialize only relevant state: selected entity + components, parent/children, nearby entities.
- System prompt explains Katla's ECS architecture and available tools.
- Start simple with keyword-based doc retrieval; upgrade to RAG later.

**UI design:**
- DraggablePanel-based chat panel in katla_ui.
- Scrolling message list, text input, send button, streaming token display.
- Function call results shown as action badges with expandable details.

**MCP integration:**
- `rmcp` crate (official Rust MCP SDK) for building an MCP server.
- Expose scene tools (query, mutate, inspect) via stdio transport.
- External AI agents can connect and drive the engine.

**Feature gating:**
- All code behind `#[cfg(feature = "llm-assistant")]`.
- Optional dependencies in Cargo.toml.
- Consider separate `katla_ai` crate.

### Relevant Crates

| Crate | Purpose |
|-------|---------|
| `async-openai` | OpenAI-compatible API client (chat, streaming, function calling) |
| `llama-cpp-2` | Local GGUF model inference via llama.cpp |
| `mistralrs` | Pure Rust LLM inference engine |
| `rmcp` (1.3+) | Official Rust MCP SDK |
| `serde` + `serde_json` | ECS state serialization (already in workspace) |
| `tokio` | Async runtime for API calls (needs careful integration with winit) |

### Gotchas

- No tokio in Katla currently — run a dedicated tokio runtime on a background thread, communicate via channels.
- Don't dump entire scene into LLM context — serialize only what's relevant (context window limits).
- `llama-cpp-2` requires C++ compiler and cmake; gate behind own feature flag.
- Never block the render loop for LLM calls.
- API keys must not be logged or committed.

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.

---

## 3. Agent Observability and Glass Box UI

### Summary

Making agent actions transparent requires: (1) an `AgentAction` data model with timestamps, parent references, and structured metadata; (2) an `ActionLog` with checkpoint storage for rollback; (3) a timeline widget showing actions as colored bars; (4) viewport entity highlighting via outline rendering; and (5) a diff view for scene changes. The observability tools landscape (LangSmith, Langfuse, Phoenix Arize, AgentOps) provides proven patterns for trace data as nested spans (trace → run → step → tool call), expandable tree views, and run comparison.

Game engine profilers offer complementary patterns: Unity's Profiler Timeline (hierarchical horizontal bars), Unreal's Rewind Debugger (timeline scrubbing + state replay), and Chrome DevTools Performance tab (combined overview + detail). For Katla, the observability UI is built in `katla_ui` using existing widgets (DraggablePanel, ListView, ScrollArea) plus new timeline and diff widgets.

### Key Findings

**Agent observability tools:**
- All major tools use a span model: start time, end time, parent, metadata, status.
- LangSmith: hierarchical trace tree, run comparison, token usage tracking.
- Langfuse: open-source, graph view for multi-agent, session tracing.
- Phoenix Arize: open-source, real-time trace streaming, OpenTelemetry spans.
- Common pattern: expandable tree views, color-coded status, filtering by type/status.

**Timeline UI:**
- Combined approach: timeline overview at top + detail list/tree below.
- Horizontal bars colored by action type (LLM call, tool use, entity mutation).
- Click to select, scroll to zoom, drag to scrub.
- Reference: Chrome DevTools Performance tab, Unity Profiler Timeline.

**Viewport highlights:**
- Post-process stencil outline shader for highlighted entities.
- Color coding by action type: green=add, red=remove, yellow=modify, blue=inspect.
- 2D overlay labels using existing DrawList system.

**Pause/resume/rollback:**
- Checkpoint-based rollback: snapshot ECS World at each agent step.
- LangGraph's time-travel/fork pattern for state management.
- Store component diffs, not full world snapshots.
- Controls: Play/Pause/Step/Rollback/Fork buttons.

**Diff views:**
- Git-style diff: additions (green), removals (red), modifications (yellow with old→new).
- Entity-level indicators in scene hierarchy.
- Aggregate summary per agent session.

### Relevant Libraries & Projects

| Project | Relevance |
|---------|-----------|
| LangSmith | Trace tree view, run comparison |
| Langfuse (open-source) | Open-source trace UI, graph view |
| Phoenix Arize (open-source) | OpenTelemetry spans, real-time streaming |
| imgui_timeline_editor | Dear ImGui timeline widget reference |
| egui-keyframe | Rust egui keyframe animation widget |
| Unity Profiler Timeline | Horizontal timeline UI pattern |
| Unreal Rewind Debugger | Timeline scrubbing + state replay |
| Chrome DevTools Performance | Combined timeline + detail reference |

### Gotchas

- Don't store full world snapshots per checkpoint — use component diffs.
- Don't block render loop for trace collection — use lock-free ring buffer.
- Build in katla_ui, not a separate web dashboard.
- Timeline widget is non-trivial to build in immediate mode — budget significant time.
- Start with wireframe overlay for highlighting, upgrade to stencil outlines later.

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.

---

## 4. Multithreading Approach for the Engine

### Summary

Katla is currently single-threaded. The recommended approach is rayon + crossbeam (no tokio needed for engine internals — tokio only for the LLM assistant). The highest-impact change is ECS parallel system dispatch: analyze component access patterns to identify systems that can run concurrently, then use rayon's scoped threads to dispatch them. For rendering, Vulkan secondary command buffers enable parallel command buffer recording across multiple threads, with each thread owning its own command pool. Parallel asset loading uses a crossbeam channel to feed resources from worker threads to the main thread.

Structural changes: per-thread command pools in katla_gfx, an ECS access analysis pass to determine system parallelism, and a parallel dispatch scheduler in katla_app. The render graph can support parallel pass execution where passes have no resource dependencies.

### Key Findings

**Vulkan command buffer parallelism:**
- Secondary command buffers recorded in parallel, executed via `vkCmdExecuteCommands` on the primary buffer.
- Each thread needs its own command pool (Vulkan requirement: command pools are not thread-safe).
- NVIDIA best practices: batch small draws into fewer command buffers, use conditional rendering.

**ECS parallel dispatch:**
- Bevy's approach: analyze component access (read vs write) and resource access per system. Systems with non-conflicting access run in parallel via rayon scope.
- `hecs-schedule`: parallel ECS scheduler based on access pattern analysis.
- Key insight: component type access conflicts determine parallelism. Two systems that only read the same component type can run in parallel; one writer blocks all.

**Rust libraries:**
- **rayon**: Work-stealing thread pool. Best for parallel system dispatch and parallel asset processing. `par_iter()`, `scope()` for scoped parallelism.
- **crossbeam**: Scoped threads, channels. Best for background worker threads (asset loading, agent execution). `crossbeam::scope`, `crossbeam::channel::bounded`.
- **tokio**: Only needed for async I/O (LLM API calls). NOT recommended for game engine internals — overhead too high for frame-level work.

**Structural changes:**
- katla_gfx: per-thread command pool management, thread-safe resource upload queue.
- katla_ecs: system access metadata (which components each system reads/writes), parallel dispatch scheduler.
- katla_app: parallel frame orchestration, background asset loading thread.

### Relevant Crates

| Crate | Purpose |
|-------|---------|
| `rayon` | Work-stealing thread pool for parallel ECS dispatch and asset processing |
| `crossbeam` | Scoped threads and channels for background workers |
| `bevy_ecs` | Reference for access-based parallel system dispatch |
| `hecs-schedule` | Alternative parallel ECS scheduler |

### Gotchas

- Don't use tokio for game engine internals — rayon + crossbeam are the right tools.
- Command pools are not thread-safe in Vulkan — one pool per thread.
- ECS archetype-based storage enables efficient parallel iteration (contiguous memory).
- Start with parallel system dispatch (highest impact), then add parallel command buffer recording.
- Access analysis must be conservative — false negatives (serializing safe parallelism) are acceptable; false positives (parallelizing conflicting access) cause data races.

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.

---

## 5. Game Loop Modes (Edit/Play/Simulate)

### Summary

All major engines implement edit/play mode separation. Unity uses domain reload + scene reload. Godot re-instantiates from PackedScene resources. Unreal duplicates the world (`UWorld::DuplicateWorldForPIE`). For Katla's custom ECS, the recommended approach is: (1) a mode state machine (`Edit → Play → Edit`, with `Simulate` as a variant); (2) ECS world cloning on mode transitions (not serialization round-trips — direct archetype buffer cloning is 10-100x faster); (3) system set dispatch to run editor-only vs. runtime-only systems; and (4) entity ID remapping for component references that survive cloning.

### Key Findings

**Unity:** Domain reload (tear down scripting domain, recreate) + scene reload (re-serialize objects). Configurable independently since 2019.3. Heavy but thorough. Static variable leakage is a major gotcha.

**Godot:** Re-instantiates scene from saved PackedScene. Simpler but can't persist runtime changes back to editor. Process modes (INHERIT/PAUSABLE/WHEN_PAUSED/ALWAYS/DISABLED) control which nodes run.

**Unreal:** World duplication via `DuplicateWorldForPIE`. Original editor world preserved. Simulate mode runs physics/gameplay at editor camera without possessing player. Eject/Possess toggle during play. "Keep Simulation Changes" feature for selective propagation back to editor.

**ECS world cloning:**
- Iterate archetypes, clone component buffers (contiguous memory — fast).
- Build entity ID remapping table (old → new).
- Skip editor-only components (selection, gizmo) when cloning for play.
- Skip runtime-only components (velocity, physics) when restoring editor.
- GPU resources (textures, meshes) shared via Arc, not cloned.

**System set dispatch:**
- Define system sets: `Editor`, `Runtime`, `Always`.
- Mode enum as run condition.
- Bevy pattern: `run_if(in_state(AppState::Playing))`.

**Simulate mode:**
- Runs runtime systems but keeps editor camera active.
- No player possession.
- Viewport remains interactive (select, inspect while physics runs).

### Gotchas

- Don't clone GPU resources — share via reference counting.
- Entity ID remapping is critical for cross-entity references (parent-child, joints).
- Avoid serialization round-trips — direct world cloning is much faster.
- Static/mutable global state causes subtle bugs across mode transitions.
- Aim for <100ms transitions (Unity takes seconds for large projects).

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.

---

## 6. Component Inspection / Reflection API

### Summary

The most practical approach for Katla is extending the existing `#[derive(Component)]` proc macro to optionally generate an `Inspect` trait implementation behind `#[cfg(feature = "editor")]`. This avoids the complexity of full reflection (bevy_reflect) while providing enough metadata for generic editor property editing. A simple `Inspect` trait with `fields()` → `Vec<FieldInfo>` and `field_mut()` for mutation, paired with a `PropertyEditor` dispatch that renders the appropriate katla_ui widget per field type, replaces the hardcoded inspector match arms.

Key references: bevy_reflect for architectural patterns, bevy_inspector_egui for layered UI design, the `facet` crate for an alternative associated-const approach, and `inventory`/`linkme` for self-registering component metadata.

### Key Findings

**bevy_reflect architecture:**
- `Reflect` trait + subtraits (`Struct`, `Enum`, `List`).
- `TypeRegistry` mapping TypeId → TypeRegistration with arbitrary TypeData.
- Dynamic types (DynamicStruct, DynamicEnum) for patching/deserialization.
- Requires explicit registration via `app.register_type::<T>()`.

**Simpler `Inspect` trait approach (recommended for Katla):**
- `fn fields(&self) -> Vec<FieldInfo>` — field metadata (name, type, constraints).
- `fn field_mut(&mut self, name: &str) -> Option<FieldMut>` — mutable field access.
- Generated by extending `#[derive(Component)]` proc macro.
- Field attributes: `#[inspect(skip)]`, `#[inspect(range = 0.0..=1.0)]`, `#[inspect(color)]`.
- Much simpler than full reflection; no type registry, no dynamic types.

**bevy_inspector_egui layered design:**
- Priority: custom InspectorEguiImpl → InspectorOptions derive → generic reflection fallback.
- Recursive rendering for nested structs.
- Multi-value editing across entities.

**Feature gating:**
- All `Inspect` trait + derive output behind `#[cfg(feature = "editor")]`.
- Zero-cost when disabled (no trait impls, no metadata compiled).
- Consider separate `katla_editor` crate to avoid additive feature issues.

**Missing katla_ui widgets needed:**
- DragValue (numeric drag-to-edit) — most critical for inspector.
- TextEdit (single-line text input) — exists as TextInput but may need refinement.
- Dropdown/ComboBox (enum selector).
- ColorPicker.

**Rust reflection ecosystem 2025-2026:**
- `facet` (fasterthanlime): associated const Shape data, no registry, serde replacement.
- `inventory` / `linkme`: distributed registration via linker sections.
- Official Rust goal: compile-time reflection via `const fn` — years from stabilization.

### Relevant Crates

| Crate | Purpose |
|-------|---------|
| `bevy_reflect` | Full reflection system — reference architecture |
| `bevy_inspector_egui` | Inspector UI layering pattern |
| `facet` | Alternative: associated const Shape, no registry |
| `inventory` / `linkme` | Self-registering components without explicit registration |
| `syn` + `quote` | Already in Katla's dependency tree (katla_derive) |

### Gotchas

- Feature flags are additive — if any crate enables `editor`, all see it.
- TypeId is not stable across compilations — use type path strings for serialization.
- Avoid full reflection complexity if only inspection is needed.
- Offset-based field access requires unsafe — consider generating typed closures instead.
- Privacy: derived field access should respect `pub` visibility.
- Color spaces: display/edit in sRGB, convert to/from linear for rendering.

### Actionable Follow-up Items

See TODO.md "Agent-in-the-Loop Glass Box" section for concrete tasks.
