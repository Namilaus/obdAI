# Recording and AI-assisted diagnosis

OBDium can save scanner readings locally and use selected recordings in an optional diagnostic conversation. Recording and ordinary scanner features work with AI disabled.

## Record scanner data

1. Connect your scanner, or connect in Demo Mode to use replay data.
2. Choose **Save every**: 1, 2, 5, or 10 seconds. The default is 1 second.
3. Optionally enter a label in **Recordings**, then choose **Start recording**.
4. Choose **Stop recording** when finished. The recording remains available after restarting the app.

The interval controls saving the latest available readings. It does not make every sensor refresh that quickly: OBDium polls different sensors at different rates. Each stored value retains its unit, precision, acquisition time, and availability. Repeated observations are distinguished from new readings. A reading more than 30 seconds old is marked stale.

Disconnecting, changing between live/replay/freeze-frame acquisition, or a storage failure stops the recording. Reconnecting does not start another recording automatically. Demo/replay and freeze-frame sources remain labeled; simulated data is not treated as a live vehicle measurement.

Open **Recordings** to inspect history, select recordings for a new conversation, change a stopped recording's label/vehicle assignment, or delete it. **Load older recordings** reveals additional history. Assign recordings to the same vehicle only when you know they belong together. Recordings from separate unidentified connections are not automatically merged.

## Configure an AI provider

1. Open **Settings → AI diagnosis** and enable the optional assistant.
2. Choose **OpenAI**, **Claude (Anthropic)**, or **DeepSeek**.
3. Enter your own API key and choose **Save key**. Keys use the operating system's credential store. If it is unavailable or locked, unlock it or explicitly select **Keep this key for this app session only** and enter the key again. Session-only keys are lost when the app exits.
4. Choose **Refresh models**, then select a model. You can also enter its exact model ID.
5. Choose **Test tool compatibility**. The app explains that this makes a small, potentially billable synthetic API request. No vehicle data is used in this test. Successful testing saves the model preference.

A model must support the application's tool-call round trip. Appearing in a model list alone does not establish that support. Replacing/removing a key invalidates its saved compatibility results; test the model again. No model or provider is silently substituted when a request fails.

API keys are not saved in browser preferences, the recordings database, or application logs. The app does not return saved key values to the interface. A session-only key temporarily takes precedence over any previously stored native key; removing the provider key attempts to remove both.

## Investigate a problem

Select relevant recordings in **Recordings**, open **Diagnostic assistant**, and describe the symptoms, when they happen, and recent repairs. The assistant can ask follow-up questions, discover available sensors, request summaries, and retrieve particular samples/time ranges. It can also inspect fault-code/readiness results that you actually scanned; it cannot trigger a new scan itself.

The first use of a provider asks you to acknowledge that your messages and retrieved data are sent to that provider and may incur API charges. Full VIN, device identifiers, and local paths are excluded from automatically assembled context. Anything you type into a message is sent as typed.

Recording alone makes no AI requests. A conversation starts only when you send a message. The assistant may make several retrieval/model calls during that turn; recording does not trigger periodic messages or background diagnoses. It can inspect newly committed data in an active selected recording on a later turn. Saved recordings can be analyzed without a connected scanner, but the provider still needs internet access.

Each chat keeps its original provider/model and selected recordings. To change those, start a **New conversation**. Each conversation is limited to one vehicle identity. The assistant has read-only access to selected sessions: it cannot clear codes, send scanner commands, start/stop recordings, delete data, or access other files/recordings.

Answers should distinguish observations from possible causes and identify missing evidence. Expand **Data read** entries to inspect retrieval results. A missing or failed trouble-code scan is unknown, not evidence that a vehicle has no faults. The assistant can suggest further checks or another recording; the user performs those actions.

Use **Cancel** to stop a response. Disabling AI or changing/removing its credentials also cancels affected work. A turn is bounded to eight model rounds and sixteen tool executions. Queries are paginated and context is bounded to the selected model's available metadata, with a conservative fallback for unknown limits. When a limit is reached, the app preserves available results and asks for a follow-up or a smaller new conversation. Token usage is shown when reported by the provider; the app does not estimate currency costs.

## Local history and deletion

The application stores recordings and conversations in `diagnostics.sqlite3` under its normal Tauri application-data directory (`com.obdium.app`). API keys remain in the native credential store or process memory.

Deleting a recording removes its samples, metadata, and every local conversation that references it, including retrieved data and derived answers. The confirmation lists the number of linked chats. Stop an active recording before deleting it. Deleting a chat alone preserves recordings. Deletion from local history cannot recall data already sent to a provider or erase separate backups.

No automatic retention purge is enabled. Older sessions/chats can be loaded from their history lists. Close normally to commit the final snapshot. After an unexpected exit, committed data remains and unfinished recordings/turns are marked interrupted on restart. If recording cannot keep up or storage fails, it stops with an error; the last successful transactions remain available. The recorder uses one synchronous transaction per snapshot, so an in-flight transaction and a not-yet-captured interval can be lost in a crash. A severely delayed scheduler may leave a marked time gap or stop recording rather than inventing samples.

## Development and verification

See [the verification notes](testing/ai-diagnostics.md) for commands and the distinction between mocked providers, replay/serial-emulator tests, and real hardware/provider validation.
