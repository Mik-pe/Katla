# Existing Codex conversation connection

`Bridge` connects to one explicit owner-private Unix control socket and verifies
that its configured thread is already loaded. It sends `initialize`,
`initialized`, paginated `thread/loaded/list` and `thread/resume`, then verifies
returned identity. A question first reads that same thread. Idle threads receive
`turn/start`; an existing active turn receives `turn/steer` with its exact ID.
No thread is created or forked, no daemon is launched, no authentication file is
read, and no model, working directory, approval or sandbox override is supplied.

The API is based on the current Rust editor contract, the installed Codex CLI
0.144.4 generated JSON schemas and the primary App Server documentation:
<https://developers.openai.com/codex/app-server/>. The installed command
`codex app-server proxy --sock ...` confirmed byte-compatible JSONL routing.
A real private-socket fixture also exposed its current EOF behavior: closing the
server stream left proxy stdout open while stdin remained open. Odin therefore
uses a direct nonblocking Unix stream to that same chosen endpoint. Its EOF is
observable without launching or terminating a host process.

`bridge_submit` clones the question and a committed PNG/metadata snapshot. Host
workers never borrow a World, application controller, GPU resource or frame.
Progress retains thread filtering and exact turn/item IDs. Unsolicited host
requests produce `Attention`; this client does not answer host approval, tool or
authentication requests. `bridge_cancel` can explicitly interrupt only the last
accepted turn. Disconnect/destroy cancels unsent requests and closes its owned
stream, leaving the host and active external conversation alive.

Bounds are explicit: four queued commands, 32 MiB queued payload, 1 MiB question
and metadata each, 24 MiB encoded PNG, 4 MiB incoming line, 64 JSON nesting
levels, 256 progress events / 4 MiB progress and a 15-second API/write deadline.
Overflow disconnects explicitly and retains a terminal event. New questions
remain disabled after disconnect until the application reconnects explicitly.
Unix endpoints require the current effective owner and exact mode 0600; symlinks
and relative paths fail. Windows reports unsupported transport.

`config_environment`, `config_load` and `config_save` only handle explicit Katla
socket/thread preferences. A missing selection remains disconnected. Connection
is a separate UI action; storing an offline endpoint does not contact it.

Run `python3 scripts/validate_odin_host.py --sanitize` for real Unix API fixtures:
loaded pagination, resume identity, idle start, active steer, turn/item filtering,
host attention, explicit interrupt, EOF, malformed integer overflow, missing
thread, wrong resume identity and cancellation of a stalled API read. These are
local transport tests; they do not establish paid-model behavior or attachment
to an actual user's desktop conversation.
