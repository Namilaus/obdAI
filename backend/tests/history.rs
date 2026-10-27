use obdium::{
    ai::{HistoryStore, ModelSelection, ProviderId},
    recording::*,
};
fn fixture() -> (RecordingStore, SessionMetadata) {
    let mut store = RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    HistoryStore::initialize(&store).unwrap();
    let session = SessionMetadata::new(
        "vehicle-a".into(),
        "Test".into(),
        1,
        SourceMode::Live,
        1,
        Timestamp::now(),
    );
    store.insert_session(&session).unwrap();
    store
        .finalize(&session.id, "user", Timestamp::now())
        .unwrap();
    (store, session)
}
fn model() -> ModelSelection {
    ModelSelection {
        provider: ProviderId::Openai,
        model_id: "fixture-model".into(),
    }
}
#[test]
fn selected_sessions_are_scoped_and_simultaneous_turns_rejected() {
    let (store, session) = fixture();
    let history = HistoryStore::new(&store);
    let chat = history
        .create(model(), vec![session.id.clone()], "Engine rattles".into())
        .unwrap();
    assert_eq!(
        history.scope(&chat.id).unwrap().recording_ids,
        vec![session.id]
    );
    let lease = history.begin_turn(&chat.id, "Why?".into()).unwrap();
    assert!(history.begin_turn(&chat.id, "Another turn".into()).is_err());
    history
        .append(&lease, "assistant", "Need more information", None)
        .unwrap();
    history.finish(&lease, "completed").unwrap();
    assert_eq!(history.messages(&chat.id).unwrap().len(), 2);
}
#[test]
fn deleting_recording_deletes_chats_and_rejects_late_results() {
    let (mut store, session) = fixture();
    let chat = HistoryStore::new(&store)
        .create(model(), vec![session.id.clone()], "Rough idle".into())
        .unwrap();
    let lease = HistoryStore::new(&store)
        .begin_turn(&chat.id, "Check it".into())
        .unwrap();
    let preview = store.delete_preview(&session.id).unwrap();
    assert_eq!(preview.conversation_ids, vec![chat.id.clone()]);
    store
        .delete_recording(&session.id, &preview.revision_token)
        .unwrap();
    assert!(HistoryStore::new(&store)
        .append(&lease, "assistant", "Late response", None)
        .is_err());
    assert!(HistoryStore::new(&store).scope(&chat.id).is_err());
}
