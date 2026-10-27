# Optional conversational AI diagnostics

Date: 2026-10-27
Status: Approved by the user on 2026-10-27. Detailed implementation planning follows.

## Intent and agreed requirements

Extend this OBDium fork with an optional conversational assistant that helps investigate vehicle problems using the user's description and recorded scanner data. Ordinary diagnostics must remain usable without AI, credentials, or network access.

The user explicitly chose:

- OpenAI, Anthropic Claude, and DeepSeek support, with their own API key and a selected model in Settings.
- A conversational assistant that obtains missing context from the user.
- Local scanner recordings at a defined interval, retrievable by the AI on demand.
- Recordings that persist across app sessions and can be deleted.
- Explicit manual Start and Stop recording controls.
- The conversational approach instead of a questionnaire followed by a single report.

The user approved this design and its proposed defaults on 2026-10-27. The sections below retain the distinction between original requirements and the implementation recommendations included in that approval.

## Approaches considered

1. **Conversational assistant with read-only data tools — selected by the user.** Supports follow-up questions and targeted retrieval of recordings; requires provider-specific tool loops and conversation management.
2. **Guided questionnaire and report.** Easier request lifecycle, but less flexible for investigating a fault. Not selected.

For implementation, retain the Rust/Tauri backend and plain JavaScript/HTML/CSS interface. Use one local recording store and conversation service with three provider adapters. A remote application server would add deployment and custody of user credentials without serving the stated needs. Putting provider logic in the webview would make credential handling and enforcing recording access harder. Neither is proposed for v1.

## Current code and constraints

- `frontend/scripts/settings.js` stores existing preferences in `localStorage`. Keep API keys out of this mechanism.
- `backend/src/main.rs` initializes Tauri, the app-data directory, and scanner polling.
- `backend/src/stats.rs` polls different groups at different rates. Some groups rotate through sensors, so the timer period is not the refresh interval of every sensor. `update_card` rounds values to whole numbers before emitting them.
- `backend/src/scalar.rs` carries values and units, including `NoData`; zero must not be interpreted as missing. Some unit conversion already happens before display.
- `backend/src/bridge/events.rs` handles connection, DTC, vehicle information, settings, and custom PID events. Scanner access uses a shared mutex.
- The existing `Record OBD Responses` feature produces replay data. The new diagnostic recordings are a separate feature and database; do not change the meaning of the existing replay file format.
- SQLite is already a dependency. Use a separate writable database under the application data directory, never the bundled VIN or DTC databases.
- `frontend/index.html` loads Chart.js remotely and the Tauri CSP is currently disabled. Secure credential entry requires bundling executable frontend dependencies and a suitable CSP; rendering AI text must not introduce HTML/script execution.
- Demo/replay and freeze-frame readings must be labeled by their actual acquisition mode, not presented as current vehicle observations. The existing freeze-frame event spelling mismatch (`use-freeze-frame` versus `use-freeze-fram`) must be addressed if the integration depends on that setting.

## Proposed v1 behavior

### Settings

- AI is disabled by default. Enabling it does not automatically send vehicle data.
- Choose a provider; save, replace, or remove its API key; choose and remember a model per provider.
- Store keys in the operating system credential store through Rust. If that store is unavailable, offer an explicitly session-only key; never silently save plaintext. Return only key-presence/masked status to the UI after entry.
- Fetch available models on an explicit Refresh/Test action, with pagination and a cached list for offline display. Listing a model does not prove that it supports the required tool calls.
- Allow an exact model ID entry for models not shown in the cached list. A test must verify text generation and a harmless synthetic read-only tool round trip before marking the model compatible. Explain that this test may incur a small provider charge and sends no vehicle data.
- Bind each conversation to its provider/model. Changing settings changes the default for new conversations, not the recipient of an existing chat. Switching an existing diagnosis to another provider requires a new chat and explicit selection of its recordings.
- Recording interval is independent of AI settings: proposed default 1 second, selectable 1, 2, 5, or 10 seconds in v1. It is fixed for a recording; stop and start a new recording to change it.

### Recording lifecycle

- Start requires a connected scanner or explicitly labeled demo/replay connection. Only one recording is active at a time. Repeated Start requests cannot create duplicate recordings.
- Create a session ID, connection generation, start time, interval, source mode, and a vehicle description when available. Unknown vehicles get independent IDs and user-editable labels; never merge them automatically.
- Record the metrics already being polled, including tracked custom PIDs. Starting a recording does not increase serial polling frequency or query every possible PID.
- Persist the latest available state on each tick. Capture an initial snapshot and a final snapshot on Stop, where available. Slow/unavailable sensors remain identifiable as such.
- Stop finalizes the session. Disconnect or acquisition-mode change also finalizes it with a reason. Reconnect does not restart recording; Start creates a new session. App close flushes pending writes; after a crash, reopen committed data and mark unfinished sessions interrupted.
- No silent automatic deletion. Show duration, date, source, vehicle label, and approximate storage usage. Storage/write failure stops recording with an explicit error while ordinary scanning continues.
- Users can inspect/select saved sessions without AI enabled. AI can analyze a saved session without a connected scanner; provider access still needs the internet.

### Conversational diagnosis

- A chat starts with a problem description and selected recording(s). Ask naturally for missing symptoms, conditions, onset, recent work, and vehicle details instead of requiring a long form.
- A context-only chat is permitted, but the assistant must say when it has no scanner evidence.
- An active recording can be selected; the assistant can retrieve newly committed samples on a later turn. There are no periodic provider requests, background diagnoses, or automatic messages on new samples.
- Within a user-initiated turn, the assistant may perform several bounded data queries without asking the user to approve each one. It can request another recording or a manual test conversationally, but cannot start recording or operate the scanner.
- Older recordings must be explicitly selected. For v1, a chat is scoped to one vehicle identity; compare only recordings explicitly assigned to that vehicle. Unknown identities must be confirmed/assigned by the user before combining recordings.
- Selected sessions are fixed after the first sent message; adding or removing sessions starts a new chat. This keeps provider history and permitted data consistent.
- Save chat messages locally for resuming; allow deletion of an entire chat. This persistence is a proposed default, not an explicit user requirement.
- Explain likely causes, supporting observations and time ranges, uncertainty, missing evidence, and next checks. Do not manufacture measurements, causal certainty, or numeric confidence percentages. Diagnosis remains advice, not a vehicle-control interface.
- Show provider/model, selected sessions, request progress, and a Cancel control. Retrieval activity can be expanded to inspect which sensors and periods were used.

### Deletion semantics

- Deleting a recording removes its samples, metadata, and every local conversation that references it, including derived summaries/tool results. Confirmation identifies the affected conversations. Deleting a chat alone keeps recordings.
- An active recording must first be stopped. Cancel affected requests and revoke their access before deleting; prevent late responses from recreating removed records. Deletion is transactional.
- Describe this as deletion from the app's local history. It cannot retract data already sent to a provider or promise forensic erasure of backups/storage media.

## Data acquisition and local storage

Introduce a typed measurement path in Rust before `update_card` rounds for presentation. Preserve existing dashboard behavior and PID decoding. A measurement needs a stable metric ID, display label, unrounded numeric value (nullable), explicit unit, availability status, acquisition time, and connection generation/source mode.

Capture acquisition timestamps when each polling call completes, rather than when a group is eventually emitted to the UI. Use monotonic elapsed time for ordering/cadence and UTC for persisted/user-visible time. An observation ID identifies a particular read even when subsequent values are equal. Never infer a new observation from an unchanged value.

The recorder consumes an in-memory latest-measurement cache and writes interval snapshots through a dedicated database worker. No disk writes or provider requests under the scanner mutex. Recording start uses only cache entries from the active connection generation; older entries are marked pre-start with their true acquisition time. Disconnect/reconnect invalidates the old cache.

Every snapshot records both its capture time and each value's acquisition time/observation ID. Include age and whether it changed since the previous snapshot. An unavailable read invalidates that sensor's latest available state rather than quietly extending its last good value. For custom PID read failures, publish unavailable status as well. For v1, classify a reading as stale after 30 seconds without a new observation, expose the age and threshold, and let summaries report unique observations separately from repeated snapshots. This threshold is a proposed builder default, not a claim about a sensor's physical validity.

Record the actual unit alongside each value. Normalize supported units for analysis without changing the source value/unit. Unknown custom units cannot be combined with other units in statistics. Changing display units must not mix incompatible values in summaries. Freeze-frame and replay mode transitions end the current recording so their provenance stays unambiguous.

Proposed SQLite entities:

- `recording_sessions`: identity, vehicle assignment/label, source mode, interval, start/end, lifecycle reason.
- `metric_definitions`: stable IDs and names; versioned custom PID definition and unit metadata.
- `snapshots` and `measurements`: session/time keys, observation ID, acquisition time, source and normalized value/unit, availability.
- `diagnostic_events`: timestamped DTC scan results, readiness results, and decoded vehicle metadata when actually obtained. Preserve not-scanned/error versus scanned-with-no-codes. Retain stored versus permanent code distinctions supported by the existing scanner.
- `conversations`, `conversation_recordings`, `messages`, and `tool_exchanges`: local history, references, provider/model, request IDs, source references and completion state. No API keys.
- Schema version/migrations, foreign keys, and indexes on session/metric/time.

DTC/readiness retrieval returns the last recorded scan and its timestamp; AI tools never trigger a new scanner scan. Prompt the user to scan manually if needed. Metadata obtained before Start may be attached only from the same connection, with its original scan time.

Use bounded writer queues and transactions. If the writer cannot keep up, stop the recording with a visible error rather than quietly dropping intervals. Read queries are bounded/paginated and must not prevent writes. Implementation chooses batching and SQLite journal settings; crash recovery must document the maximum uncommitted interval loss.

## Read-only tools and context boundary

Tools execute in Rust and query the local store. The provider receives results, not direct database/file access. A request carries a conversation ID; Rust resolves its permitted sessions. Validate every requested session, metric, time range, page cursor, and output size independently of model text.

Proposed tool surface:

| Tool | Purpose |
| --- | --- |
| `list_selected_recordings` | Metadata and available time ranges for the chat's selected sessions only. |
| `list_recording_metrics` | Available metrics, units, coverage, and availability for one selected session. |
| `get_recording_summary` | Bounded summaries by metric/time range: min/max/mean, first/last, unique-observation count, missing/stale coverage. |
| `get_recording_samples` | Bounded detailed snapshots with source/acquisition timestamps, units and provenance; continuation cursor when truncated. |
| `get_diagnostic_context` | User-provided vehicle description and recorded DTC/readiness/vehicle metadata, including freshness and missing-data status. |

Summaries use unique valid observations for numeric statistics and separately report interval coverage; repeated stale samples must not bias the mean or imply more measurements. Every result includes a session and time reference usable in the assistant's answer.

Initial guardrails proposed for v1: at most 8 model/tool rounds per user turn, 16 tool executions total, 10 metrics and 500 sample rows per detailed query, and 64 KiB serialized output per tool result. Bound each full turn's assembled context to the selected model's supported context with room for output. Truncate/paginate explicitly; never silently omit measurement provenance. At the turn limit, return the available partial result and offer continuation.

Long conversations use deterministic retention of recent complete turns plus a labeled compact summary and source references. The implementation must preserve protocol-required tool-call pairs and provider reasoning blocks when needed; model-generated summaries remain interpretations and cannot replace measurement evidence. The compaction budget must not erase the user's symptoms/constraints.

Tool results, user text, and custom metric names are untrusted data, never instructions or executable SQL. Use parameterized queries. No tool accepts raw SQL, paths, arbitrary URLs, serial commands, deletion, key access, or fault-code clearing.

## Provider adapters and request lifecycle

Use a shared Rust interface for model discovery, capability validation, and a conversational turn. Implement OpenAI Responses, Anthropic Messages, and DeepSeek Chat Completions adapters against official APIs. Keep provider-specific authentication, tool schemas, tool/result IDs, streaming events, errors and required continuation content in each adapter. Do not assume wire compatibility merely because two providers support function calls.

The current official guides document application-executed tool round trips for [OpenAI](https://developers.openai.com/api/docs/guides/function-calling), [Claude](https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview), and [DeepSeek](https://api-docs.deepseek.com/guides/tool_calls/). Model discovery is documented for [OpenAI](https://developers.openai.com/api/reference/resources/models/methods/list), [Claude](https://platform.claude.com/docs/en/api/models/list), and [DeepSeek](https://api-docs.deepseek.com/api/list-models/). Model IDs, capabilities and version headers must be rechecked during implementation rather than frozen from memory in this design.

The application owns conversation history locally and sends the bounded required context per turn; use non-storing request options where supported and verify provider semantics before implementation. This does not imply a provider never retains requests under its own policy.

- Network calls happen in Rust with fixed official provider endpoints and TLS. No custom endpoint support in v1.
- First use explains that user messages, vehicle description, and retrieved data are sent to the chosen provider and may incur API charges. Full VIN, local paths and serial device identifiers are excluded from automatic provider context. Full VIN sharing is out of scope for v1; warn that free text is sent as typed.
- Use asynchronous requests with timeouts, streaming text where supported, cancellation, and bounded retries. Authentication/quota/model-compatibility failures are explicit. Do not silently switch models/providers or retry an ambiguously completed generation that may bill twice.
- One active turn per conversation; settings/key deletion or disabling AI cancels affected work. Cancellation keeps prior committed history and marks partial turns incomplete; an interrupted tool exchange is never replayed as if complete.
- Capture usage counts when supplied, without estimating currency costs from stale pricing tables. Do not log keys, authorization headers, or full request/vehicle payloads by default.
- Render assistant content as escaped text or sanitized Markdown. Bundle executable frontend assets and restrict script sources before credential entry ships. The frontend uses narrowly scoped Tauri commands, not broad events carrying secret values.

## Scope and non-goals

This is a multi-part feature: recording infrastructure, persistent history, provider configuration, a controlled tool loop, and chat UI. Deliver it incrementally within one agreed design.

Out of scope for v1: autonomous continuous AI monitoring, write-capable vehicle tools, new manufacturer-specific decoding, cloud recording sync, a hosted account/backend, local LLMs/custom endpoints, voice/images, web search, semantic/vector search, automatic repair execution, automatic retention deletion, and replacement of the existing replay format.

## Delivery order and verification intent

1. **Measurements and recording store:** preserve precision/provenance, start/stop lifecycle, interval storage, migrations/recovery, and bounded read APIs.
2. **Recording controls and history:** inspect/select/delete recordings, distinguish live/demo/replay/freeze-frame sources, and validate storage failure/disconnect behavior.
3. **Provider configuration:** native credential storage, per-provider preferences, model discovery and synthetic capability tests.
4. **Assistant backend:** scoped tools, adapters, bounded turns, cancellation, conversation persistence and deletion behavior.
5. **Conversation interface and integration:** symptoms/follow-up flow, streaming/status, source references, selected sessions and understandable error states.

Verify recorder timing with controlled clocks and distinct observation timestamps; test zero/missing/stale values, precision, unit changes, custom PID failures, reconnect isolation, disk failures, crash recovery, and deletion races. Test summaries against known numeric fixtures and enforce tool bounds/session scope in Rust. Use mocked provider responses to exercise tool round trips, multiple calls, incompatible models, malformed arguments, cancellation, rate limits, and context budgets for all three providers. Verify no provider requests occur when AI is disabled or merely recording, and no keys persist in localStorage/database/logs.

Exercise the complete workflow with the repository's demo/replay data, clearly labeled synthetic. Real-provider smoke tests require user-supplied credentials and an explicit small-charge test action; no paid API calls are needed to build or run mocked tests. Verify packaging and credential behavior on Windows, macOS, and Linux as available; report untested platforms honestly. Any scanner/protocol changes must also follow `CONTRIBUTING.md` requirements for emulator or real-vehicle validation.

## Decisions left to implementation

Exact Rust module boundaries, HTTP/credential crates and versions, schema migrations, stable metric-ID mapping, stale-threshold refinement if fixture evidence justifies it, UI layout within the existing design, SQLite batching/journal tuning, retry delays, context-budget implementation, and test harness details are builder decisions. They must preserve the above behavior and be recorded in the implementation plan. No new frontend framework or remote service is needed.

## Review status

Created from repository inspection and the conversation. An independent builder review found no important unanswered questions preventing implementation planning. The user approved the design and defaults on 2026-10-27, including one-second recording cadence, local chat persistence, stopping on disconnect, session-scoped data access, and deletion of dependent chats. This review checks design completeness; it does not establish that any implementation or provider integration has been tested.
