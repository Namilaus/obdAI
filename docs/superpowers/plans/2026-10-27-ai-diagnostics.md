# Conversational AI Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add manually controlled persistent scanner recordings and an optional conversational assistant that retrieves selected data through read-only tools using the user's OpenAI, Claude, or DeepSeek credentials.

**Architecture:** Keep the existing Tauri application and plain JavaScript interface. A Rust measurement cache feeds a separate local SQLite store; a Rust conversation service scopes retrieval to explicitly selected recordings and delegates network serialization to three provider adapters. Disk and network work never hold the scanner mutex.

**Tech Stack:** Rust, Tauri 2, existing `sqlite`/Serde/Tokio dependencies, reqwest with Rustls, an OS credential-store adapter, plain JavaScript modules, Node's built-in test runner for frontend logic. Resolve compatible dependency versions during execution and commit `backend/Cargo.lock`; no frontend framework or hosted service.

**Spec:** [Approved design](../specs/2026-10-27-ai-diagnostics-design.md), approved 2026-10-27.

## Global Constraints

- AI is disabled by default. Ordinary diagnostics and recording work without AI or network access.
- Manual Start/Stop; one recording at a time; interval choices 1, 2, 5, or 10 seconds, default 1 second, fixed during a recording.
- Recording stops on disconnect, source-mode change, or storage failure; reconnect never restarts it.
- Preserve precision, observation identity, acquisition/capture timestamps, units, unavailable values, and source mode. A value is stale after 30 seconds without a new observation.
- Selected recordings and provider/model are fixed after the first message. One vehicle identity per chat; unknown vehicles are never merged automatically.
- Store recordings and chats locally. Deleting a recording transactionally deletes all dependent conversations; deleting a conversation preserves recordings.
- Provider requests occur only for an explicit model-list/test action or user-initiated chat turn; never from a recording timer.
- Limits: 8 model/tool rounds, 16 tool executions per user turn; 10 metrics and 500 measurement rows per detailed query; 64 KiB per serialized tool result.
- Keys never enter localStorage, application SQLite, logs, exported events, or model context. Use native credential storage or an explicitly chosen memory-only key.
- Model tools cannot change scanner state, start/stop recordings, clear codes, delete data, run SQL, or access files/keys/arbitrary URLs.
- Exclude full VIN, serial device IDs and paths from automatically assembled provider context. Free text is sent as typed, as explained to the user.
- Preserve the existing raw OBD replay format. Do not publish or deploy as part of implementation. Use mocks for paid-provider tests unless the user explicitly runs the small-charge test.
- Existing CONTRIBUTING.md requires emulator/vehicle validation for scanner/protocol changes. Demo testing alone is not evidence of physical adapter correctness.

## Review Focus

1. Wall-clock adjustments during recording: monotonic cadence/order must survive time going backward (Task 3).
2. Delete/cancel races with a late provider response: deleted history must never reappear (Tasks 7 and 12).
3. Equal-valued new observations versus duplicated snapshots: statistics count reads, not interval copies (Tasks 2 and 5).
4. Split UTF-8/SSE messages and malformed tool arguments: display complete text, never execute partial calls (Tasks 9–11).
5. Locked OS key store and unavailable/changed model IDs: preserve ordinary diagnostics, expose recovery, never downgrade to plaintext or another model (Tasks 8–11).

## Execution conventions and milestones

This is one integrated plan with two independently demonstrable milestones: **A, recording/history (Tasks 1–6)**, and **B, conversational diagnosis (Tasks 7–14)**. Shared contracts below prevent splitting the two into incompatible data formats. Tasks are sequential by default; the three provider adapters can be built independently after Task 8 if the chosen execution method supports it.

All shell commands below run from the repository root unless specified. Use `cargo ... --manifest-path backend/Cargo.toml`; the Rust manifest is not at the repository root. Before execution, inspect Git status, reuse/create isolation according to using-git-worktrees, and preserve these approved documents. Run `cargo test --manifest-path backend/Cargo.toml --lib` and `cargo check --manifest-path backend/Cargo.toml --bins` once to establish baseline; distinguish environment/build failures from assertion failures.

Test blocks below name required tests and key assertions; fixture helpers are defined in the test files, not production APIs. For each task, first run the stated focused command and confirm a meaningful failure, implement the task, then rerun it and require all focused tests to pass. Expected failures from missing new APIs count for initial interface scaffolding; infrastructure/download errors do not. Commit only the task's explicit file set with the suggested message; never stage unrelated workspace changes.

Tests for new services live in the library so they do not require a running webview, vehicle, credential store, or real provider. Tauri commands are thin binary-side adapters. New frontend state/formatting modules accept an injected command client and are testable with Node; do not add a browser framework solely for testing.

## File and contract map

| Files | Responsibility |
| --- | --- |
| `backend/src/recording/{mod,types,metrics,cache,store,recorder,queries}.rs` | Typed observations, cache, persistent recording lifecycle and bounded reads. |
| `backend/src/recording/migrations/001_recordings.sql` | Versioned recording/session/diagnostic-event schema. |
| `backend/src/ai/{mod,types,credentials,settings,history,tools,context,service,prompt}.rs` | Credentials, configuration, local conversations, retrieval authorization, bounded turns. |
| `backend/src/ai/providers/{mod,transport,sse,openai,anthropic,deepseek}.rs` | Shared transport plus provider-specific protocol adapters. |
| `backend/src/ai/migrations/002_conversations.sql` | Chat history and non-secret AI preferences in the same application database. |
| `backend/src/bridge/{recordings,ai}.rs` | Typed Tauri command wrappers and tagged UI events. |
| `backend/src/{lib,main,stats,scalar,obd,response}.rs`, `backend/src/pid/diagnostics.rs`, `backend/src/bridge/{mod,events}.rs` | Existing integration points; preserve PID interpretation except explicitly tested correctness fixes. |
| `frontend/scripts/{recordings,recording-state,ai-settings,ai-chat,ai-state,api,graphs}.js` | Recording UI, settings, chat, testable state, Tauri invocation, extracted chart initialization. |
| `frontend/index.html`, `frontend/components/{recordings,ai}.css`, `frontend/vendor/` | Existing app layout plus new UI and bundled executable assets. |
| `frontend/package.json`, `frontend/tests/{recordings,ai}.test.mjs` | ESM declaration and Node tests; no runtime framework. |
| `backend/src/recording/tests/`, `backend/src/ai/tests/` | Rust unit fixtures, fake clock/store/transport, provider wire fixtures; declared with `#[cfg(test)] mod tests`. |
| `docs/ai-diagnostics.md`, `docs/testing/ai-diagnostics.md` | User guide and reproducible validation matrix. |

### Shared domain types

Define these at their owning task before downstream callers. IDs are typed UUID newtypes serialized as strings, except `ConnectionGeneration` and monotonic `ObservationId` are u64 wrappers. Use camelCase JSON at the frontend boundary.

- Recording types (Task 2): `SessionId`, `VehicleId`, `MetricId` (stable string), `ConnectionGeneration`, `ObservationId`; `Timestamp { utc_ms: i64, elapsed_ms: u64 }`; `SourceMode { live, demo, replay, freeze_frame }` plus an independent demo/replay provenance flag so freeze-frame does not hide simulation; `Measurement { metric_id, observation_id, generation, acquired_at, value: Option<f64>, unit: String, availability }`. `availability` is valid/no_data/error; non-finite input becomes unavailable. `CapturedSnapshot` adds captured timestamp, sample age, pre-start/repeated flags and source metadata.
- Session types (Tasks 2–3): `SessionMetadata` includes vehicle assignment, label, interval, provenance, start/end and reason; `StartRecording { interval_seconds, label }` gets connection identity from backend state, never from an untrusted UI claim. `RecordingStatus` is idle/recording/stopped/error with active session and committed snapshot count. `StopReason` includes user/disconnect/source_changed/storage_error/shutdown/interrupted.
- Query types (Task 5): `SamplesQuery { session_id, metric_ids, start_elapsed_ms, end_elapsed_ms, limit, cursor }`; range is half-open and monotonic within a session. `SamplePage` holds rows, provenance, truncation and next cursor. `SummaryQuery` uses the same selection without pagination; `MetricSummary` includes unit, first/last/min/max/mean, unique observation count and repeated/stale/unavailable coverage. `PageRequest`/`Page<T>` paginate metadata separately.
- AI types (Tasks 7–8): `ProviderId { openai, anthropic, deepseek }`, `ConversationId`, `TurnId`, `ModelInfo { id, label, context_limit: Option<u32>, tool_support: known/unknown/unsupported }`, `ModelSelection { provider, model_id }`. `ChatMessage` has role/content/status; `ToolCall { id, name, arguments: serde_json::Value }`; `ToolResult { call_id, payload, is_error }`. `WireContinuation` holds required provider content privately and is persisted under the same deletion scope as its conversation.
- Errors: `RecordingError` and `AiError` have stable UI codes plus redacted messages. Typed variants include invalid_input/not_connected/busy/not_found/deleted/storage/auth/quota/unsupported_model/cancelled/timeout/protocol. Never return raw credential-store/network errors containing secrets to the frontend.

## Task 1: Correct unit conversions needed by recording

**Files:** Modify `backend/src/scalar.rs`; add its inline tests. No unrelated parser edits.

**Interfaces:** Retain `Scalar::convert(&self, target_unit: Unit) -> Option<Scalar>` and `Scalar::new(...)`. Downstream normalization uses the tested conversion path; unsupported dimensional pairs return `None`.

- [ ] Add `scalar::tests::recording_units_are_correct` and source-preservation/round-trip assertions:

```rust
assert!((convert(212.0, Fahrenheit, Celsius) - 100.0).abs() < 0.001);
assert!((convert(1.0, Kilometers, Feet) - 3280.8399).abs() < 0.01);
assert!((convert(3280.8399, Feet, Kilometers) - 1.0).abs() < 0.001);
assert_eq!(Scalar::no_data().unit, NoData);
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib scalar::tests`; confirm numeric failures. Inspection found Fahrenheit conversion multiplies instead of divides by 1.8, and kilometer/foot conversion uses 83281. Test all currently used preferred-unit pairs before relying on them.
- [ ] Fix only reproduced conversions; add identity and supported normalization conversions as needed, preserving source value/unit. Temperature, distance, speed, pressure, torque, time and flow must not cross dimensions; unknown custom units stay separate.
- [ ] Rerun the command; require all assertions to pass and existing callers to compile.
- [ ] Commit: `fix: preserve correct units for recorded measurements`.

## Task 2: Recording domain, cache and SQLite schema

**Files:** Create `recording/{mod,types,metrics,cache,store}.rs`, `recording/migrations/001_recordings.sql`, `recording/tests/{mod,store,cache}.rs`; modify `backend/src/lib.rs`, `backend/Cargo.toml`, `backend/Cargo.lock`.

**Interfaces:** `MeasurementCache::publish(&self, measurement: Measurement) -> Result<(), RecordingError>`; `snapshot(&self, generation: ConnectionGeneration, now: Timestamp) -> Vec<Measurement>`; `reset(&self, generation)`. `RecordingStore::open(path: &Path) -> Result<Self, RecordingError>` runs migrations; `insert_session(&mut self, metadata: &SessionMetadata)`, `append_snapshot(&mut self, id: SessionId, snapshot: &CapturedSnapshot)`, and `finalize(&mut self, id, reason: StopReason, ended_at: Timestamp)` return `Result<(), RecordingError>`.

- [ ] Add isolated SQLite and cache fixtures. Required assertions in tests:

```rust
assert_eq!(reopened.measurement_value(session, metric), Some(12.375));
assert_eq!(cache.latest(metric).value, Some(0.0));
assert_eq!(cache.after_unavailable(metric).value, None);
assert_ne!(equal_value_read_1.observation_id, equal_value_read_2.observation_id);
assert!(cache.snapshot(new_generation, now).is_empty());
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib recording::tests`; confirm missing-domain/store failures.
- [ ] Implement schema with foreign keys, indexes and migration version. Use `<app_data>/diagnostics.sqlite3`; a separate writer owns its SQLite connection, readers use separate connections. Enable WAL with full synchronous commits. Custom metric IDs incorporate a versioned definition ID, not just display name; map built-in metric IDs explicitly, including bank/sensor distinctions. Tests use temporary directories (`tempfile` dev dependency); add UUID generation and necessary Tokio features when first needed.
- [ ] Rerun focused tests. Include migration rollback, idempotent reopen, malformed/non-finite values and parameterized labels containing SQL punctuation.
- [ ] Commit: `feat: add persistent recording store and measurement cache`.

## Task 3: Recorder timing and lifecycle

**Files:** Create `recording/recorder.rs`, `recording/tests/recorder.rs`; extend Task 2 types/store.

**Interfaces:** Cloneable `RecorderHandle` exposes async `start(StartRecording) -> Result<SessionMetadata, RecordingError>`, `stop(StopReason) -> Result<SessionMetadata, RecordingError>`, `status() -> RecordingStatus`, and `shutdown() -> Result<(), RecordingError>`. `set_connection(ConnectionContext)`/`disconnect(ConnectionGeneration)` update authoritative connection state. A `Clock` trait supplies `now() -> Timestamp` and injectable deadlines; test clocks never sleep in real time.

- [ ] Add deterministic lifecycle assertions:

```rust
assert_eq!(after_3_seconds.periodic_snapshot_count(), 3);
assert_eq!(start_twice.error_code(), "busy");
assert_eq!(after_disconnect.status, "stopped");
assert!(!after_reconnect.is_recording());
assert!(elapsed_times.windows(2).all(|w| w[0] <= w[1])); // UTC moves backward
assert_eq!(reopened_unfinished.reason, "interrupted");
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib recording::tests::recorder`; require expected failures.
- [ ] Implement serialized lifecycle transitions, initial/final snapshots, interval whitelist and 30-second stale boundary (`age > 30_000 ms`). Use skip-on-missed-tick scheduling; mark gaps rather than fabricating historical snapshots. Use at most one queued snapshot plus one in-flight write, one transaction per snapshot. Queue saturation stops recording with a visible storage error; diagnostic-event loss similarly stops the recording. Document crash exposure: at most two accepted but uncommitted snapshots, plus any not-yet-captured interval; slow/delayed scheduling may add a marked gap. No automatic retention purge.
- [ ] Rerun tests, including injected disk-full/commit failure, empty sensor cache, stale readings at 30,000/30,001 ms and duplicate Stop. Status counts only committed snapshots; shutdown drains or reports failure rather than claiming a successful flush.
- [ ] Commit: `feat: add manual interval recording lifecycle`.

## Task 4: Integrate scanner observations and truthful diagnostic status

**Files:** Modify `backend/src/{main,stats,obd,response}.rs`, `backend/src/pid/diagnostics.rs`, `backend/src/bridge/{mod,events}.rs`; create `recording/tests/scanner.rs` and focused inline diagnostic parser/status tests.

**Interfaces:** Add `ObservedScalar { scalar: Scalar, measurement: Measurement }` and `observe_scalar(metric: &MetricDefinition, context: &ConnectionContext, read: impl FnOnce() -> Scalar, clock: &dyn Clock) -> ObservedScalar`. Each polling call is wrapped individually; publish the resulting measurement in memory and keep the scalar for existing dashboard formatting. Add fallible `OBD::scan_trouble_codes() -> Result<Vec<TroubleCode>, Error>` and `scan_permanent_trouble_codes()` alongside compatibility wrappers; readiness acquisition returns a typed status preserving failed/unsupported reads. Define `DiagnosticEvent { generation, acquired_at, kind, status, payload }` and `RecorderHandle::record_diagnostic(event)`.

- [ ] Add fixtures for acquisition time preceding grouped display, precision retained while display rounds, unavailable custom PID results, source switches, and success-empty versus failed DTC scan:

```rust
assert_eq!(recorded.value, Some(12.375)); assert_eq!(display.value, 12.0);
assert!(first.acquired_at.elapsed_ms < second.acquired_at.elapsed_ms);
assert!(failed_scan.is_err()); assert_eq!(successful_empty_scan.unwrap().len(), 0);
assert!(prior_generation_event.is_rejected());
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib recording::tests::scanner` plus diagnostic test filters; confirm meaningful failures.
- [ ] Wrap all polling paths (including tuple evaluations, one-time and custom reads), capture acquisition time immediately after each call, and register generation/mode under the same connection lifecycle. Correct the freeze-frame event-name mismatch. Stop before changing replay/freeze-frame mode; avoid installing duplicate reconnect listeners. Keep disk/network operations outside locks. Propagate transport/read failure through a fallible ISO-TP read path instead of `unwrap_or_default`; validate successful response status before classifying empty codes. Existing Vec-returning APIs may delegate for compatibility, but new recording events use fallible results. Readiness failures must remain unknown rather than all tests passing. Snapshot cached diagnostic context only from the current generation, with original timestamps. A manual VIN decode of an unrelated VIN must not overwrite connected-vehicle context.
- [ ] Rerun focused tests and `cargo check --manifest-path backend/Cargo.toml --bins`. Use fixtures/emulator for success, no-data, malformed response, read failure, permanent/stored distinctions and reconnect events. Compare serial requests before/after: recording adds no polling requests. Mark real-adapter verification pending until performed.
- [ ] Commit: `feat: capture scanner observations with reliable provenance`.

## Task 5: Bounded recording queries and summaries

**Files:** Create `recording/queries.rs`, `recording/tests/queries.rs`; extend store/types.

**Interfaces:** `RecordingReader::list_sessions(PageRequest) -> Result<Page<SessionMetadata>, RecordingError>`, `metrics(SessionId) -> Result<Vec<MetricDefinition>, RecordingError>`, `samples(SamplesQuery) -> Result<SamplePage, RecordingError>`, `summary(SummaryQuery) -> Result<Vec<MetricSummary>, RecordingError>`, `diagnostic_context(SessionId) -> Result<DiagnosticContext, RecordingError>`. `RecordingStore::assign_vehicle(session, vehicle_id, label)` validates identity assignment. Unknown identity grouping requires explicit user assignment; same label alone is insufficient.

- [ ] Add tests for unique-read statistics, mixed preferred units, zero/no-data, boundaries and pagination:

```rust
assert_eq!(summary.unique_observations, 2); // 10, repeated 10, new 20
assert_eq!(summary.mean, Some(15.0));
assert!(page.rows.len() <= 500); assert!(serialized_result.len() <= 65_536);
assert_eq!(normalized_temperature_mean, Some(100.0)); // 100 C and 212 F
assert!(unknown_units_remain_separate);
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib recording::tests::queries`; confirm failures.
- [ ] Implement stable cursor paging ordered by elapsed time/snapshot/metric ID; cap 10 metrics, 500 measurement rows, and 64 KiB including the envelope. Include provenance and explicit truncation. Metadata defaults to pages of 50/max 100 items; cap individual labels and metric descriptions to 256 UTF-8 characters. Summary queries operate in SQLite/incremental aggregation, not by loading whole sessions; cancellation must interrupt long reads. Coverage considers unavailable snapshots while statistics deduplicate valid observations and exclude stale entries. Partition unknown units and incompatible dimensions. No auto DTC scan on query.
- [ ] Rerun tests, including cursor reuse on another session, concurrent append without duplicate pages, reversed ranges, Unicode byte limits, and a large seeded recording with bounded result/memory size.
- [ ] Commit: `feat: query and summarize recorded scanner data`.

## Task 6: Recording controls and usable local history (Milestone A)

**Files:** Create `backend/src/bridge/recordings.rs`, `frontend/scripts/{api,recordings,recording-state}.js`, `frontend/components/recordings.css`, `frontend/package.json`, `frontend/tests/recordings.test.mjs`; modify `backend/src/{main.rs,bridge/mod.rs}`, `frontend/index.html`, `frontend/scripts/settings.js`.

**Interfaces:** Tauri commands: `recording_start`, `recording_stop`, `recording_status`, `recording_list`, `recording_metrics`, `recording_samples`, `recording_summary`, `recording_assign_vehicle`, `recording_delete_preview`, `recording_delete`. Mutations return the authoritative state. JavaScript `createRecordingController({invoke, subscribe, render})` exposes `initialize()`, `start(options)`, `stop()`, `select(id)`, `delete(id, previewToken)`, `dispose()`; events include monotonically increasing revisions to ignore stale UI updates.

- [ ] Add Node tests with fake invocation/subscription and render callbacks:

```js
assert.equal(startCallsAfterDoubleClick, 1);
assert.equal(state.recording, false); // disconnect notification
assert.equal(networkCallsWithAiDisabled, 0);
assert.deepEqual(restoredSessionIds, savedSessionIds);
assert.equal(deletionWithoutMatchingPreview.allowed, false);
```

- [ ] Run `node --test frontend/tests/recordings.test.mjs`; confirm expected missing-controller failures.
- [ ] Implement an always-visible recording indicator/control in the dashboard and a Recordings history view using existing navigation. Include interval setting, source, duration, committed count, storage estimate, inspection, labels/vehicle assignment and selection for later diagnosis. Reject active-session deletion; preview the exact affected items and require a matching revision token before transactional deletion. Milestone A returns zero affected chats; Task 7 extends this. Persist preferences separately from credentials. Use `window.__TAURI__.core.invoke` through one adapter; do not spread legacy invocation shapes into new code. On app close await recorder shutdown through Tauri lifecycle handling with a visible failure state.
- [ ] Rerun Node tests and binary check. Manually exercise demo Start → Stop → restart → inspect → delete, double clicks, disconnect and disk error; ordinary dashboard still works with AI absent. Check controls at the app's 1025×400 minimum window size.
- [ ] Commit: `feat: add recording controls and persistent history`.

## Task 7: Local conversation history and deletion authorization

**Files:** Create `ai/{mod,types,history}.rs`, `ai/migrations/002_conversations.sql`, `ai/tests/{mod,history}.rs`; extend recording store/deletion commands and library exports.

**Interfaces:** `HistoryStore::create(selection: ModelSelection, sessions: Vec<SessionId>, vehicle: Option<VehicleId>) -> Result<Conversation, AiError>`; `begin_turn(id, user_text) -> Result<TurnLease, AiError>`; `append(lease: &TurnLease, exchange: Exchange) -> Result<(), AiError>`; `finish(lease, status)`; `scope(id) -> Result<ConversationScope, AiError>`; `delete_conversation(id)`. `TurnLease { conversation_id, turn_id, revision }` prevents stale writes. `DeletionPreview { recording_id, conversation_ids, revision_token }` is shared with recording commands. `ConversationScope` includes permitted sessions, vehicle and deletion revision.

- [ ] Add tests:

```rust
assert!(chat_with_mixed_vehicle_ids.is_err());
assert!(change_selection_after_first_message.is_err());
assert!(second_simultaneous_turn.is_err());
assert!(append_after_recording_deletion.is_err());
assert_eq!(remaining_referenced_conversations, 0);
assert!(recording_survives_chat_deletion);
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib ai::tests::history`; confirm failures.
- [ ] Implement history in the same SQLite database/worker for atomic cascade deletion. Keep one active lease per conversation; revoke it before cancellation/deletion and check it again before every result commit. Recheck preview revision so newly dependent chats cannot be deleted without appearing in confirmation. A conversation with no recordings uses an optional manual vehicle profile; a later recording selection starts a new chat. Persist full exchanges/provider continuation privately; restart marks unfinished turns interrupted and excludes incomplete tool sequences from future requests. Revocation correctness must not depend on the network task honoring cancellation.
- [ ] Rerun tests, including app restart, provider-default changes leaving existing chats unchanged, deletion rollback, and late response after delete/recreate with a different UUID.
- [ ] Commit: `feat: persist scoped diagnostic conversations`.

## Task 8: Secure settings, frontend isolation and provider contracts

**Files:** Create `ai/{credentials,settings}.rs`, `ai/providers/{mod,transport,sse}.rs`, `ai/tests/{credentials,transport,sse}.rs`, `frontend/scripts/graphs.js`, `frontend/vendor/chart.umd.js` and license/provenance file; extend `ai/types.rs`, Cargo manifest/lock, `frontend/index.html`, `backend/tauri.conf.json`.

**Interfaces:** `CredentialStore` trait: `set(provider, secret: SecretString, persistence: CredentialPersistence)`, `get(provider) -> Result<SecretString, AiError>`, `status(provider) -> KeyStatus`, `remove(provider)`; all operations return `Result` and secret types redact Debug. `AiSettingsStore::load()/save(AiPreferences)` stores enabled/default model/per-provider choices, model cache and disclosure acknowledgment, never keys. Async `ProviderAdapter` trait: `list_models(credentials, cursor, cancel) -> Result<ModelPage, AiError>` and `respond(request: ProviderRequest, credentials, events: EventSink, cancel) -> Result<ProviderReply, AiError>`. `ProviderRequest` owns model, prompt, bounded complete history, tool schemas/results and continuation; `ProviderReply` contains text, complete tool calls, usage and continuation. `EventSink` tags text/progress with conversation/turn IDs. Inject `HttpTransport` and credential mocks for tests.

- [ ] Add credential/settings/transport/SSE fixtures:

```rust
assert!(!serialized_settings.contains("test-secret"));
assert!(!format!("{secret:?}").contains("test-secret"));
assert_eq!(locked_vault_save.error_code(), "credential_unavailable");
assert!(!locked_vault_creates_plaintext_file);
assert_eq!(split_utf8_and_sse_chunks.text(), "Temperatur: 90 °C");
assert_eq!(redirected_request.forwarded_auth_headers(), 0);
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib ai::tests`; confirm the relevant failures.
- [ ] Implement per-platform native credential adapters with explicit memory-only fallback. Use a distinct key-store service label, e.g. `obdium.ai`, and provider-specific accounts; use native durable Linux Secret Service rather than assuming a volatile kernel keyring is persistent. Verify current ecosystem APIs before choosing crate features: [keyring documentation](https://docs.rs/keyring/latest/keyring/) now distinguishes simple compatibility use from `keyring-core` plus explicit backends. Add reqwest/Rustls, cancellation and streaming dependencies with compatible pinned lockfile versions. Use fixed official origins, HTTPS-only, no redirects, no verbose body/auth logging, 10-second connect and 60-second idle timeout, 180-second total turn deadline. Retry idempotent model discovery at most twice; generation retries only after an explicit rejection known not to have run, never after ambiguous timeout/partial output. See [reqwest configuration](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html).
- [ ] Bundle Chart.js with verified source/version/license, extract existing inline graph initialization into `graphs.js`, and set CSP to local scripts without unsafe-eval/unsafe-inline script execution. Preserve only required Tauri IPC/local asset connections; fonts may use system fallback. This is required before key entry is wired in Task 13. Test graph startup in the built app under CSP. Rerun credential/transport/SSE tests with locked vault, corrupt model cache, cancellation and split JSON/tool blocks. Bound SSE event buffers (1 MiB) and reject overlong incomplete messages.
- [ ] Commit: `feat: add secure AI configuration and provider contracts`.

## Tasks 9–11: Provider adapters (one independently tested task per provider)

**Task 9 files:** Create `ai/providers/openai.rs`, `ai/tests/openai.rs`, `ai/tests/fixtures/openai/`; modify provider registry.

**Task 10 files:** Create `ai/providers/anthropic.rs`, `ai/tests/anthropic.rs`, `ai/tests/fixtures/anthropic/`; modify registry.

**Task 11 files:** Create `ai/providers/deepseek.rs`, `ai/tests/deepseek.rs`, `ai/tests/fixtures/deepseek/`; modify registry.

**Interfaces:** Each implements Task 8's `ProviderAdapter`. Add shared `validate_model(adapter, model, credentials, cancel) -> Result<ModelValidation, AiError>` using a fixed synthetic tool that returns `{"ok":true}`; mark compatible only when the model produces a valid tool call and completes the result round trip. Listing alone never certifies compatibility. Model pagination and capabilities remain provider-specific.

For **each** of Tasks 9, 10 and 11 independently:

- [ ] Add sanitized documented wire fixtures: model list with another page, streamed text, multiple tool calls, arguments across SSE boundaries, result continuation, refusal, invalid key, quota/rate limit, unsupported model, timeout and cancellation. Required assertions:

```rust
assert_eq!(executed_call_ids, returned_call_ids);
assert!(continuation_keeps_required_provider_blocks);
assert_eq!(compatibility_without_tool_call, ModelValidation::Unsupported);
assert_eq!(generation_retries_after_partial_output, 0);
assert!(model_list_pagination_collects_all_pages);
```

- [ ] Run the provider's exact filter: `cargo test --manifest-path backend/Cargo.toml --lib ai::tests::openai`, then separately `... ai::tests::anthropic` and `... ai::tests::deepseek` for their tasks. Confirm expected failures before implementing that adapter.
- [ ] Implement the provider protocol verified against the linked official docs in the spec: OpenAI Responses, Anthropic Messages and DeepSeek Chat Completions. Recheck authentication/version headers and model capabilities during execution; do not invent a fixed latest-model ID. Preserve protocol-required reasoning/continuation content and full tool/result IDs; execute only completed calls. Disable provider response storage where supported, without promising zero retention. A context limit absent from discovery uses versioned verified model metadata; unknown exact IDs require a successful capability test and a conservative documented context budget. Pin the tested capability result to provider/model and invalidate it after incompatible responses.
- [ ] Rerun that provider's focused tests. Run validation with the fake synthetic tool and prove no vehicle data appears in its serialized request. Report real-provider tests as unrun without authorized credentials; passing mocks is not live API validation.
- [ ] Commit separately: `feat: add OpenAI diagnostic provider`, `feat: add Claude diagnostic provider`, `feat: add DeepSeek diagnostic provider`.

## Task 12: Scoped tools, bounded conversations and cancellation

**Files:** Create `ai/{tools,context,service,prompt}.rs`, `ai/tests/{tools,context,service}.rs`; extend history/types/settings revocation hooks.

**Interfaces:** `ToolRouter::execute(scope: &ConversationScope, call: ToolCall, cancel) -> Result<ToolResult, AiError>` implements exactly `list_selected_recordings`, `list_recording_metrics`, `get_recording_summary`, `get_recording_samples`, `get_diagnostic_context`. `ContextBuilder::build(conversation, limit: ContextBudget) -> Result<ProviderRequest, AiError>`; `AssistantService::send(conversation_id, user_text, events: EventSink) -> Result<TurnId, AiError>` launches one turn and emits terminal status; `cancel(turn_id)`; `disable()`/credential invalidation revoke affected work. `TurnBudget` defaults to 8 rounds/16 calls/180 seconds; a final no-tools completion counts as a round.

- [ ] Add tool scope, long history, loop and race fixtures:

```rust
assert!(request_for_unselected_session.is_denied());
assert!(malformed_or_extra_tool_arguments.is_rejected());
assert!(turn.model_calls <= 8); assert!(turn.tool_calls <= 16);
assert!(result_json.len() <= 65_536);
assert!(context.has_user_symptoms_and_constraints());
assert!(context.has_only_complete_tool_pairs());
assert_eq!(provider_calls_when_disabled_or_recording_only, 0);
assert!(!late_result_after_delete_or_disable.was_persisted());
```

- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib ai::tests`; confirm failures in new tools/context/service tests, not regressions silently accepted in earlier tasks.
- [ ] Implement parameter validation, parameterized read APIs, independent per-call session authorization and revocation recheck immediately before dispatch and commit. Limit summaries/metadata as well as samples to the envelope byte cap. Keep latest complete turns and a deterministic labeled extractive summary of earlier user context plus source references; no extra summarization API call in v1. Retain complete required provider blocks. Reserve 4,096 output tokens when supported, otherwise the model's lower verified limit; include schemas/continuations/results in conservative context accounting. If even pinned user context cannot fit, return a clear context-limit state rather than silently losing it. Unknown context models use a conservative 8,192-token total ceiling until verified; unsupported limits remain recoverable errors, not a model switch. The loop should use summaries before detailed samples, explain absent data, cite session/time evidence, and ask follow-ups. On limits/cancellation return a truthful partial state; never claim an unfinished diagnosis is complete.
- [ ] Rerun all AI unit tests with provider mocks. Include tool names requesting deletion/SQL/serial commands, stale cursor after deletion, huge custom labels, free-text prompt injection, cancellation during read/stream, API key removal/replacement, and new committed samples during an active recording. Disable/revoke before network cancellation so a late callback cannot commit.
- [ ] Commit: `feat: add bounded conversational diagnosis with read-only tools`.

## Task 13: AI settings and conversational interface

**Files:** Create `backend/src/bridge/ai.rs`, `frontend/scripts/{ai-settings,ai-chat,ai-state}.js`, `frontend/components/ai.css`, `frontend/tests/ai.test.mjs`; modify `backend/src/{main.rs,bridge/mod.rs}`, `frontend/index.html`, `frontend/scripts/api.js` and recording selection integration.

**Interfaces:** Tauri commands `ai_preferences_get/set`, `ai_key_set/remove/status`, `ai_models_refresh`, `ai_model_test`, `ai_conversation_create/list/get/delete`, `ai_message_send`, `ai_turn_cancel`; frontend never receives key reads. `createAiController({invoke, subscribe, render})` exposes `initialize()`, `createConversation(selection)`, `send(text)`, `cancel()`, `dispose()`; events are tagged by conversation/turn and include text_delta/tool_activity/error/completed/cancelled. Credential calls use dedicated commands and clear password fields after submission.

- [ ] Add controller/render tests using injected command mocks and an escaped-text rendering helper:

```js
assert.equal(sendEnabledWithoutKey, false);
assert.equal(providerAfterGlobalDefaultChange, originalConversationProvider);
assert.equal(staleTurnDeltaWasRendered, false);
assert.equal(renderedScriptExecuted, false);
assert.equal(modelTestContainsVehicleContext, false);
assert.equal(keyWasWrittenToLocalStorage, false);
```

- [ ] Run `node --test frontend/tests/ai.test.mjs`; confirm expected failures.
- [ ] Implement Settings → AI (enabled/provider/key/model/test/refresh, exact model entry, memory-only option); show the small-charge disclosure before synthetic testing. Implement assistant navigation with recording selection, typed problem description, resumable chats, streamed escaped text, cancel/retry states, expandable retrieval activity and source/time references. Use plain text for v1 assistant output, avoiding an unnecessary Markdown renderer. Explain once per provider before first send that messages and retrieved data leave the device; no per-tool confirmation. Chat can begin without recordings and must show that state. Keep recording controls available with AI disabled. Provide dependent-chat deletion preview and separate chat deletion. Resuming uses the chat's fixed provider/model; deleted/unavailable credentials surface an actionable configuration error.
- [ ] Rerun both frontend test files and `cargo check --manifest-path backend/Cargo.toml --bins`. Manually verify keyboard use, scrolling during streaming, minimum window size, unavailable models, key store failure, disconnect, and restoring history. Confirm CSP still permits charts and Tauri IPC while script injection is blocked.
- [ ] Commit: `feat: add optional AI settings and diagnostic chat`.

## Task 14: Integration evidence, packaging and user documentation

**Files:** Create `backend/src/ai/tests/integration.rs`, `docs/ai-diagnostics.md`, `docs/testing/ai-diagnostics.md`; update README feature/build documentation and only necessary Cargo/packaging configuration.

**Interfaces:** No new public API. Use the real recorder/store/tool router with fake providers and replay fixtures. Existing raw replay remains independently usable.

- [ ] Add an end-to-end library fixture: connect simulated generation → manual recording → multiple ticks → stop/reopen → create scoped chat → tool query → answer → delete recording → stale completion rejected. Assert timestamps/units/provenance survive, chats resume, provider payload omits automatic VIN/path/device fields, and disabled AI makes zero requests.
- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib ai::tests::integration` and identify any genuine failure before corrective implementation.
- [ ] Complete only integration fixes supported by failures. Document local file location, recording cadence versus polling frequency, active-session queries, stale/missing data, deletion scope, API charges, credential recovery, supported providers, manual model testing, interruption loss bounds and platform limitations. Add a test matrix for Windows/macOS/Linux native credentials and packaging; use current Tauri dependency instructions when exercising each platform. Do not invoke or modify the publishing workflow to release the fork.
- [ ] Run `cargo test --manifest-path backend/Cargo.toml --lib`, `cargo check --manifest-path backend/Cargo.toml --bins`, and `node --test frontend/tests/recordings.test.mjs frontend/tests/ai.test.mjs`; require zero test failures. Run formatting checks on touched Rust files and inspect `git diff --check`. Build from `backend/` using `cargo tauri build --no-bundle` if the CLI/platform dependencies are installed. Record actual results; do not label unavailable platform or real-provider checks passed. Verify scanner changes with emulator or a real adapter as required, and identify any remaining physical validation before release.
- [ ] Commit: `test: verify recorded-data AI diagnosis end to end`. Perform the execution workflow's final code review and address substantive findings before declaring implementation complete.

## Self-review and handoff

Coverage checked against the approved design: persistence/lifecycle Tasks 2–6; accurate scanner evidence Tasks 1/4; settings/keys/model discovery Tasks 8–11/13; selected-session and vehicle authorization Tasks 5/7/12; conversation persistence/deletion Tasks 7/12; provider protocols Tasks 8–11; context/cost/cancellation limits Task 12; UI and content isolation Tasks 6/8/13; verification/release limits Task 14. The five Review Focus cases have explicit owning tests. Signatures and shared types above are the contract for implementers; helper details remain local to each module.

Plan-only status: no product changes, dependency installation, paid API requests, or application tests have been performed as part of writing this plan. The unit-conversion arithmetic was checked directly against the source formulas; executable Rust regressions belong to Task 1. Design approval is recorded in the linked spec. Execution method is not yet selected; recommend native execution in this chat because acquisition, lifecycle, persistence and deletion share interfaces, followed by independent whole-change review.
