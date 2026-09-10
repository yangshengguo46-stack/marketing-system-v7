# AI IP native marketing work (development slice)

This fork adds the `marketing_work` model tool through the native extension registry, not a client-side agent or a new JSON-RPC method. Use ordinary `thread/start` and `turn/start` with a marketing assignment. The native Lead can:

1. Call `{"action":"open","workId":"gift-first-work","brief":"The actual mission and constraints","materials":["materials/interview.md"]}`. Material paths must already exist relative to the selected execution workspace.
2. Read the returned `workFile` and original material files using native tools. Research audience situations and proof; record actual findings with `{"action":"record","workId":"gift-first-work","stage":"research","body":"Findings, sources and unresolved questions"}`.
3. Continue through `direction` and `draft`, retaining the reasons for the direction and delivering full words, audiovisual choices and practical production notes. Assemble the existing ContentPackage for the final response; this tool does not validate marketing quality.

`open` without a brief reopens saved work. `startAt:"draft"` supports revising existing material without fabricating research. Recording an upstream stage retains downstream bodies and marks them `needsReview`; explicit review can reuse them. Saved `results` indexes are research, direction and draft. Tool previews are incomplete navigation aids, limited to a 900-byte serialized response; full bodies remain in the file. Briefs and stage bodies accept up to 8000 UTF-8 bytes each; keep large source material in separate files.

Files are development artifacts at `output/marketing-work/<workId>.json`, not the finished encrypted customer-project store. Use a new workId for a different brief/material set. Only one writer should edit a workId; cross-process coordination and crash-atomic writes are not provided. Completed saves can be reopened from another thread sharing the workspace. Source-file changes are not automatically detected: reread materials and record revised findings. Native filesystem permissions apply. `environmentId` selects among multiple available environments. The upstream extension API currently omits foreign-platform working directories (for example Linux host with Windows executor); that existing limitation remains, without changing core.

# User verification cancellation (experimental)

Local UI clients can cancel a native user-verification RPC by sending
`userVerification/cancel` with `{requestId}` and the `experimentalApi` opt-in.
The result is an empty acknowledgment (`{}`). This API does not enable desktop
verification capability advertisement.

`requestId` is the original status, enroll, delete, or verify RPC's string or
integer ID on the same connection, not the server elicitation ID. Use fresh IDs
for each operation and a distinct ID for the cancel RPC. Unknown, finished,
unrelated, and other-connection requests are no-ops.

The acknowledgment confirms the cancellation signal without waiting for the OS
prompt to close. The original RPC completes independently, with
`cancelled/interrupted` when cancellation prevents completion. Cancellation
cannot roll back completed effects. It remains effective while a proof waits for
outbound queue capacity, but cannot retract a response already enqueued.

Canceling or resolving an elicitation does not itself stop a separate
`userVerification/verify` RPC. Clients must cancel that RPC separately and discard
late proofs after the approval is canceled or resolved. Only one native worker
runs per app-server; if an OS call remains active after cancellation or timeout,
subsequent local operations return `failed/providerError` until that worker exits.

# Hosted Codex Apps MCP protocol

The host-owned HTTP `codex_apps` server uses Legacy by default in app-server and
standalone Codex. To discover the 2026-07-28 protocol, set
`codex_apps_mcp_2026_07_28 = true` under `[features]`, or send a true runtime
override via `experimentalFeature/enablement/set`. Discovery falls back to Legacy
when the server does not support it. Explicit config takes precedence.
The dedicated setting does not apply to third-party HTTP or local `codex_app`
stdio servers. The existing `mcp_2026_07_28` flag still governs eligible other
servers, regardless of whether their names or URLs resemble hosted Apps.
App-server does not persist this selection.

# Thread removal

`thread/archive` and `thread/delete` reject attempts to remove a live internal
worker with JSON-RPC error `-32600`. The worker's owner controls its shutdown.
For example, a Guardian reviewer remains available to its parent conversation
after a client tries to archive or delete it.

After the owner releases the worker, its saved conversation can be archived or
deleted normally. Ordinary client-controlled threads keep their existing behavior.

# Amazon Bedrock authentication

If `model_providers.amazon-bedrock.aws.credential_export` is configured, Bedrock setup and
Bedrock login return an error without changing configuration or saved credentials. Remove the
exporter configuration before selecting another credential source. `aws.credential_export` and
`aws.profile` cannot be configured together.
