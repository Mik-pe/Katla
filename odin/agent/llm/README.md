# Odin provider transport and conversational tools

This package ports provider configuration, actual HTTP/SSE requests, conversational
history, tool rounds, admission and the asynchronous render-loop bridge. It never
receives a World pointer. The application keeps ticking its existing
`editor.Agent_Harness` while provider workers submit validated tools and consume
only their matching cancellation tickets through `agent_take_result_for`.
Different conversations and MCP may share a scene mailbox; callers must not use
FIFO reply consumption to steal another transport's tickets.

`Config` owns its strings and allocator. `config_parse` accepts the flat
`llm.toml` schema written by Katla: provider, api_key, base_url, model, max_tokens,
temperature, rate_limit_min_interval_ms, rate_limit_max_calls_per_minute. The new
optional `api` selects `responses` or `chat_completions`, and `timeout_ms` sets a
bounded deadline (default 120 seconds, maximum one hour). Unknown, duplicate,
malformed or out-of-range fields fail rather than silently choosing another
provider/model. Basic and literal single-line strings, numeric scalars and comments
are accepted; tables, arrays, multiline strings and other TOML constructs are
outside this flat settings schema and are rejected.

`open_ai` uses the OpenAI Responses endpoint and preserves the supplied model.
`open_ai_compatible` requires an explicit HTTP(S) base URL and defaults to Chat
Completions; its `api` can explicitly select Responses. URL credentials, query
strings and fragments are rejected. Sampling temperature is sent only when the
configuration explicitly supplies it, allowing models that reject this parameter.
These wire contracts follow the official [function-calling](https://developers.openai.com/api/docs/guides/function-calling)
and [streaming](https://developers.openai.com/api/docs/guides/streaming-responses)
documentation. There is no model/provider substitution or local response fallback.

Keys may use `$ENV_VAR`, resolved just before each actual request. Generic Odin
`fmt` and JSON serialization omit `Config.api_key`; do not log request bodies,
Authorization headers or resolved strings. `config_save` writes an empty temporary
file, applies Unix 0600 before any secret bytes, flushes/closes it, then atomically
renames it over the destination. Invalid input leaves the previous file intact.
Windows uses the inherited directory ACL; Unix permission evidence is not a Windows
ACL guarantee. `config_user_path`, `config_load_user` and `config_save_user` use
`os.user_config_dir` plus `katla/llm.toml`; a missing per-user file yields Disabled,
while unreadable/malformed settings yield Config. No existing user configuration
or key is required for the local transport acceptance suite.

Initialize one stationary `Runtime` before starting workers, supply a thread-safe
allocator, and join users before destruction. Native libcurl performs HTTP/TLS,
verifies peer/hostname, restricts protocols to HTTP(S), disables redirects and
sets connection and total request deadlines. It returns typed failures without
printing provider error bodies or credential-bearing diagnostics. Atomic cancellation
aborts transport; each rate-limit wait retries the shared atomic admission. Clock
sampling and admission have an outer mutex so scheduling between simultaneous
callers cannot turn a valid monotonic clock into a false backwards-clock failure.
Only admitted requests enter the rolling minute; there are no hidden retries.

SSE input tolerates arbitrary byte fragmentation, split UTF-8 and CRLF. It bounds
total input to 8 MiB, one event/line to 256 KiB, events to 16,384, text to 1 MiB,
and complete tool calls to 64. JSON has a 64-level depth limit, strict numeric
syntax, duplicate-key rejection, finite numbers and exact signed-integer bounds.
Empty object keys are rejected before the standard parser can discard them.
Chat Completions assembles indexed function-argument fragments and requires a
valid finish reason plus `[DONE]`; Responses checks the authoritative completed
output against streamed text. Failed/incomplete/refused, malformed, unsupported
or truncated results never enter tool execution. Function arguments must finish
as a JSON object, and IDs/names must be present and unique.

A `Conversation` clones the complete provider configuration when initialized, so
a pending worker never borrows mutable application settings. Recreate the runtime
after joining workers when rate-limit settings change; a mismatched limiter/config
fails explicitly. It owns bounded serialized history (512 messages/8 MiB), selected
tool definitions and all previously used call IDs. It rejects unadvertised tools
before mailbox admission, preserves exact string IDs in tool results and prevents
reused call IDs from replaying an operation. Entity identities remain decimal
strings even beyond JavaScript's precise range. Up to 16 provider/tool rounds run
per user turn; invalid admitted tool arguments become explicit correlated tool
feedback. Closed/exhausted mailboxes, cancellation, deadlines and limits fail the
turn. Earlier accepted scene operations remain in shared undo history; neither
network failure nor cancellation pretends to roll them back. A failed conversation
cannot silently resume an incomplete tool history.

`Job` starts one actual producer thread and exposes `job_poll_text` and
`job_poll`. Its bounded progress queue defaults to 64 chunks, with a 1 MiB byte
limit. A stalled consumer causes explicit Limit plus cancellation rather than
successful output with dropped text. Progress strings and terminal responses
transfer ownership once; unread data is freed by `job_destroy`, which cancels and
joins the worker. `conversation_reap` cancels queued work or frees its completed
reply. If a scene owner is still executing, `conversation_destroy` returns the
remaining ticket; the harness owns that late reply and must outlive execution.
Destroy the conversation, runtime/config and harness only after their users join.

```sh
odin test odin/agent/llm -out:target/odin-llm-tests -vet -strict-style
odin run tools/build -- validate http
odin run tools/build -- validate http --sanitize
```

The real `odin/examples/llm_authoring` consumer joins provider workers, executes
spawn/query against the independently owned application World, verifies terminal
and incremental text and undoes accepted operations. Seventeen subprocess journeys
exercise both HTTP/SSE contracts, simultaneous conversations sharing one mailbox,
schema restrictions, exact IDs/history, atomic admission, network and paused-owner
cancellation, cancellation after accepted mutation, bounded progress backpressure,
truncation, malformed arguments, 429, untrusted TLS rejection before any credential
reaches the test server, blocked redirects, oversized input and
timeouts. Every process checks that captured allocator tracking is empty on exit.
Unit tests also cover persistence, credential-safe diagnostics, schema/history
ownership, duplicate IDs, busy conversations, joined async terminal transfer and
parser limits. Normal, optimized and native AddressSanitizer checks pass; Linux
and Windows typechecks cover the runnable consumer.

This is local native transport and CPU authoring evidence. It does not establish
paid-provider credentials/model acceptance, desktop co-creator UI integration,
rendered material effects or native Linux/Windows HTTP execution. The native binary
links Odin's vendor libcurl; macOS uses system libcurl, Linux needs the vendor
binding's libcurl/mbedTLS/z link dependencies, and Windows needs its libcurl library.
The caller supplies schemas for the operations its actual scene owner supports;
remaining resource/application/gfx tools are extended by their own migrations.
