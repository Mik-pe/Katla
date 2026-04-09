# TODO

## AI Panel: Tool Calling

The LLM tools are defined and sent to the API, but tool calls from the LLM are
not parsed or executed. The streaming pipeline drops tool call data on the floor.

### 1. Carry tool call data through the streaming pipeline
- **What**: `StreamChunk` currently only has `content_delta` and `finish_reason`. The OpenAI streaming API sends tool call deltas (function name + argument fragments) as separate fields on each chunk. These are ignored in `convert_stream_chunk()`.
- **Files**: `katla_agent/src/llm/mod.rs`, `katla_agent/src/llm/openai.rs`
- **Details**:
  - Add `tool_call_deltas: Vec<ToolCallDelta>` to `StreamChunk` where `ToolCallDelta { index: usize, id: Option<String>, name: Option<String>, arguments_delta: Option<String> }`
  - In `convert_stream_chunk()`, extract `choice.delta.tool_calls` from the OpenAI stream response and populate the deltas
  - In `CoCreatorAgent::poll_stream()`, accumulate tool call deltas across chunks into complete `ToolCall` structs

### 2. Accumulate complete tool calls in CoCreatorAgent
- **What**: When streaming finishes with `FinishReason::ToolCall`, the accumulated tool calls need to be assembled and stored.
- **Files**: `katla_agent/src/co_creator/mod.rs`
- **Details**:
  - Add `pending_tool_calls: Vec<ToolCall>` and `tool_call_accumulators: HashMap<usize, ToolCallAccumulator>` to `CoCreatorAgent`
  - When `StreamEvent::ToolCall` fires, the accumulated tool calls are available
  - Store them on the agent for later execution
  - Include tool calls in the assistant message recorded in history (the `ChatMessage.tool_calls` field already exists but is always `None`)

### 3. Execute tool calls against the ECS world
- **What**: Parse completed tool call arguments and translate them into scene operations or direct ECS mutations.
- **Files**: `katla_app/src/application/editor/agent.rs`, `katla_agent/src/co_creator/mod.rs`
- **Details**:
  - Map tool call names to `SceneOp` variants or `LocalAction` variants:
    - `spawn_entity` -> `SceneOp::SpawnEntity`
    - `destroy_entity` -> `SceneOp::DestroyEntity`
    - `set_field` -> `SceneOp::SetField`
    - `query_entities` -> `SceneOp::QueryEntities`
    - `get_scene_hierarchy` -> `SceneOp::GetSceneHierarchy`
    - `duplicate_entity` -> `SceneOp::DuplicateEntity`
  - Execute via the existing `SceneTool` system or directly through `Application` methods
  - Return results as tool result messages

### 4. Send tool results back to the LLM
- **What**: After executing a tool call, send the result back to the LLM so it can continue the conversation. This is the multi-turn tool call loop.
- **Files**: `katla_agent/src/co_creator/mod.rs`, `katla_agent/src/runtime.rs`, `katla_app/src/application/editor/agent.rs`
- **Details**:
  - Add `ChatMessage` with `role: Tool` and content = execution result to history
  - Re-submit the full conversation history (including tool results) as a new streaming request
  - The LLM will either call more tools or produce a final text response
  - Handle the multi-turn loop: submit -> tool call -> execute -> submit -> ... -> final response

### 5. Display tool call activity in the AI panel
- **What**: Show what the LLM is doing when it calls tools (e.g. "Spawning cube at (0, 1, 0)...").
- **Files**: `katla_app/src/ui/editor_ui/co_creator.rs`, `katla_app/src/application/editor/agent.rs`
- **Details**:
  - Replace the current `(Tool calling not yet wired)` system message with actual tool call info
  - Add a `DisplayMessage` variant or use system messages to show tool execution status
  - Show tool results (e.g. entity IDs, query results) as formatted messages

## AI Panel: UX Polish

- [x] ~~### 6. Auto-scroll to latest message~~ — Implemented via ScrollArea with `stick_to_bottom(true)`.
- [x] ~~### 7. Message area scroll with ScrollArea widget~~ — Replaced `begin_column`/`end_column` with `scroll_area()`, tracking `ScrollAreaState` in `CoCreatorState`.
- [x] ~~### 8. Multiline input with Shift+Enter~~ — Added `multiline` mode to `TextInput`: Shift+Enter inserts newline, Enter submits. Dynamic input height up to 5 lines.
