# AI diagnostics verification

Run commands from the repository root:

```powershell
cargo test --manifest-path backend/Cargo.toml --lib --tests
cargo check --manifest-path backend/Cargo.toml --bins
cargo build --manifest-path backend/Cargo.toml --bins
node --test frontend/tests/recordings.test.mjs frontend/tests/ai.test.mjs
git diff --check
```

The Rust tests use isolated SQLite stores, synthetic credentials, mocked provider transports, the repository's replay trace copied to a temporary directory, and an in-process ELM327 serial emulator. They make no paid provider requests and do not connect to a physical vehicle.

| Area | Evidence |
| --- | --- |
| Measurement accuracy | Unrounded values, zero/missing states, corrected Fahrenheit/foot conversions, acquisition timestamps. |
| Recording lifecycle | Explicit start/stop, monotonic timing under changing UTC, interruption recovery, reconnect isolation. |
| Retrieval | Observation-based statistics, compatible-unit normalization, paging and scope checks. |
| Local history | One active turn per chat, selected vehicle/session scope, recording deletion cascading to chats, late-write rejection. |
| Credential handling | Redacted secrets, explicit memory fallback, locked native backend errors, preferences containing no credentials. |
| Provider protocols | OpenAI continuation/tool identities; Claude thinking signatures and complete blocks; DeepSeek argument fragments/reasoning continuation; interrupted/malformed streams. |
| Resource bounds | Disabled AI makes no provider calls; eight model rounds and sixteen tool executions. |
| Complete flow | Replay → persistent recording → reopen → provider tool call → actual stored-data summary → delete dependent history. |
| Frontend state | Duplicate start prevention, saved history, explicit deletion preview, stale turn filtering, deleted-chat clearing, older recording pages. |

Additional regressions cover switching from a pending chat request to another running conversation, simulation provenance in every direct data tool, recording duration/size metadata, spaced 29-bit CAN and legacy headers, and legacy checksum rejection. Failed SQLite writes are injected with `PRAGMA query_only` to verify that snapshot or diagnostic persistence failures stop recording visibly and never count unsaved snapshots.

The ELM327 emulator exercises actual serial-trait writes/reads for RPM, stored codes and an unavailable permanent-code response. DTC fixtures cover full zero-byte code pairs, a code containing the service byte, separate ECU/count fields and complete/incomplete ISO-TP frames. The interpretation follows the manufacturer's [ELM327 datasheet](https://www.elmelectronics.com/wp-content/uploads/2017/01/ELM327DS.pdf), particularly the trouble-code and CAN-message sections. Physical adapter timing, driver behavior and manufacturer-specific differences still require vehicle validation before release.

The frontend was exercised in installed Edge with mocked Tauri IPC/provider responses and the actual application CSP. Checks cover recording controls, AI configuration, chat rendering, script-like assistant content remaining text, no remote executable requests, and 1200×800 / 1025×400 layouts. These checks do not replace testing a packaged Tauri webview or native credential dialogs on every platform.

## Manual acceptance before release

- Connect an actual scanner or an external ELM327 emulator. Record, stop, restart, inspect and delete a session. Verify no additional serial polling requests are caused by recording.
- Confirm that disconnect/reconnect and replay/freeze-frame changes end the current session and do not restart it automatically.
- Exercise storage failure/recovery and verify that the error is visible and ordinary scanning remains available.
- With a user-supplied key, explicitly run the small-charge compatibility test for each chosen provider/model, then diagnose a selected demo recording. Verify follow-up tool calls and cancellation; inspect the provider account's reported usage.
- Verify native key save/read/remove on Windows Credential Manager, macOS Keychain, and Linux Secret Service. Test a locked/unavailable vault and explicit session-only mode.
- Verify platform packaging, charts and IPC under CSP. Existing upstream warnings should be distinguished from new failures.

## Current execution record

The independent final review reported three important findings (scanner header variants, pending-request navigation, simulation provenance) and one minor finding (duration/size display). All were fixed, regression-tested, and confirmed by the reviewer. Paid provider calls and physical-vehicle tests have not been performed by the coding agent.

| Platform/check | Result |
| --- | --- |
| Windows Rust tests and native debug binary | 38 Rust tests passed; `cargo check --bins` and `cargo build --bins` passed. Eight frontend tests passed. |
| Edge with actual CSP and mocked IPC | Passed, zero page errors/external requests; both supported test window sizes. |
| Windows native credential save/read/remove | Not exercised with a real key; mock secret-store error/fallback tests passed. |
| macOS Keychain and Linux Secret Service | Implementation selected by keyring's platform backend; native builds and round trips unrun. |
| Tauri packaging / installer | Unrun: `cargo tauri` CLI is not installed; native binary built with Cargo. |
| Real provider APIs and physical scanner | Unrun; synthetic transports, replay, and serial emulator provide the automated evidence. |
