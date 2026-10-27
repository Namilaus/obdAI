use obdium::{ai::*, recording::*};
use serde_json::json;

fn fixture() -> (RecordingStore, Conversation) {
    let mut store = RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    HistoryStore::initialize(&store).unwrap();
    let session = SessionMetadata::new(
        "v".into(),
        "recording".into(),
        1,
        SourceMode::Live,
        1,
        Timestamp::now(),
    );
    store.insert_session(&session).unwrap();
    let chat = HistoryStore::new(&store)
        .create(
            ModelSelection {
                provider: ProviderId::Openai,
                model_id: "fixture-model".into(),
            },
            vec![session.id],
            "Fuel smell after warming up".into(),
        )
        .unwrap();
    (store, chat)
}
#[test]
fn tools_reject_unselected_sessions_extra_arguments_and_write_actions() {
    let (store, chat) = fixture();
    let scope = HistoryStore::new(&store).scope(&chat.id).unwrap();
    for (name, args) in [
        ("get_recording_samples", json!({"sessionId":"unselected"})),
        (
            "get_recording_summary",
            json!({"sessionId":chat.recording_ids[0],"sql":"DELETE FROM recording_sessions"}),
        ),
        ("clear_fault_codes", json!({})),
    ] {
        assert!(ToolRouter::execute(
            &store,
            &scope,
            ToolCall {
                id: "c".into(),
                name: name.into(),
                arguments: args
            },
            4096
        )
        .is_err());
    }
    let result = ToolRouter::execute(
        &store,
        &scope,
        ToolCall {
            id: "c".into(),
            name: "list_selected_recordings".into(),
            arguments: json!({}),
        },
        4096,
    )
    .unwrap();
    assert!(result.to_string().len() <= 4096);
    assert!(!result.to_string().contains("serialPort"));
}
#[test]
fn context_compacts_complete_turns_without_losing_user_symptoms() {
    let (_store, chat) = fixture();
    let mut messages = Vec::new();
    for index in 0..8 {
        messages.push(ChatMessage {
            role: "user".into(),
            content: format!("Symptom {index}: warm engine"),
            extra: None,
            turn_id: index.to_string(),
            status: "completed".into(),
        });
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: "lengthy explanation ".repeat(300),
            extra: None,
            turn_id: index.to_string(),
            status: "completed".into(),
        });
    }
    let request = ContextBuilder::build(&chat, messages, 8192).unwrap();
    assert!(request.system.contains("Fuel smell after warming up"));
    assert!(request.system.contains("Symptom 0"));
    assert!(
        serde_json::to_vec(&request.messages).unwrap().len()
            + request.system.len()
            + serde_json::to_vec(&request.tools).unwrap().len()
            < 8192
    );
}
#[test]
fn every_direct_data_tool_preserves_simulated_freeze_frame_provenance() {
    let mut store = RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    HistoryStore::initialize(&store).unwrap();
    let mut session = SessionMetadata::new(
        "v".into(),
        "Simulated freeze frame".into(),
        1,
        SourceMode::FreezeFrame,
        1,
        Timestamp::now(),
    );
    session.simulated = true;
    store.insert_session(&session).unwrap();
    let chat = HistoryStore::new(&store)
        .create(
            ModelSelection {
                provider: ProviderId::Openai,
                model_id: "fixture".into(),
            },
            vec![session.id.clone()],
            "Test".into(),
        )
        .unwrap();
    let scope = HistoryStore::new(&store).scope(&chat.id).unwrap();
    for name in [
        "get_recording_summary",
        "get_recording_samples",
        "list_recording_metrics",
        "get_diagnostic_context",
    ] {
        let result = ToolRouter::execute(
            &store,
            &scope,
            ToolCall {
                id: "c".into(),
                name: name.into(),
                arguments: json!({"sessionId":session.id}),
            },
            65536,
        )
        .unwrap();
        let envelope = if name == "get_diagnostic_context" {
            &result["recordings"][0]
        } else {
            &result
        };
        assert_eq!(envelope["source"], "freeze_frame", "{name}");
        assert_eq!(envelope["simulated"], true, "{name}");
    }
}
