mod support;
use obdium::{
    ai::{
        providers::{transport::*, *},
        *,
    },
    recording::*,
    OBD,
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
struct NoNativeKeys;
impl SecretBackend for NoNativeKeys {
    fn get(&self, _: ProviderId) -> Result<Option<SecretString>, AiError> {
        Ok(None)
    }
    fn set(&self, _: ProviderId, _: &SecretString) -> Result<(), AiError> {
        unreachable!()
    }
    fn remove(&self, _: ProviderId) -> Result<(), AiError> {
        Ok(())
    }
}
#[tokio::test]
async fn replay_recording_reopens_and_ai_retrieves_it_then_deletion_revokes_history() {
    let dir = std::env::temp_dir().join(format!("obdium-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let trace = dir.join("requests.json");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/requests.json"),
        &trace,
    )
    .unwrap();
    let db = dir.join("diagnostics.sqlite3");
    let mut obd = OBD::new();
    obd.set_replay_file(trace.to_string_lossy().into_owned())
        .unwrap();
    obd.connect("DEMO MODE", 38400, 0).unwrap();
    let generation = obd.generation();
    let cache = Arc::new(MeasurementCache::default());
    let mut recorder = Recorder::new(RecordingStore::open(&db).unwrap(), cache.clone());
    recorder.set_connection(Some(ConnectionContext {
        generation,
        vehicle_id: uuid::Uuid::new_v4().to_string(),
        source: SourceMode::Demo,
        simulated: true,
    }));
    let session = recorder
        .start(
            1,
            "Replay idle".into(),
            Timestamp {
                utc_ms: 1000,
                elapsed_ms: 0,
            },
        )
        .unwrap();
    let mut values = Vec::new();
    for i in 1..=2 {
        let rpm = obd.rpm();
        assert_eq!(rpm.unit, obdium::scalar::Unit::RPM);
        values.push(rpm.value as f64);
        cache
            .publish(Measurement {
                metric_id: "engine.rpm".into(),
                name: "Engine RPM".into(),
                observation_id: i,
                generation,
                acquired_at: Timestamp {
                    utc_ms: 1000 + i as i64 * 1000,
                    elapsed_ms: i * 1000,
                },
                value: Some(rpm.value as f64),
                unit: "RPM".into(),
                availability: Availability::Valid,
            })
            .unwrap();
        recorder
            .tick(Timestamp {
                utc_ms: 1000 + i as i64 * 1000,
                elapsed_ms: i * 1000,
            })
            .unwrap();
    }
    recorder
        .stop(
            "user",
            Timestamp {
                utc_ms: 4000,
                elapsed_ms: 3000,
            },
        )
        .unwrap();
    drop(recorder);
    let handle = RecorderHandle::open(&db).unwrap();
    let recording_id = session.id.clone();
    let chat = handle
        .call(move |r| {
            let s = r.store.session(&recording_id)?;
            assert_eq!(s.source, SourceMode::Demo);
            assert!(s.simulated);
            let summary = r.store.summary(&recording_id, &[], 0, u64::MAX)?;
            assert_eq!(summary[0].unique_observations, 2);
            assert!((summary[0].mean.unwrap() - (values[0] + values[1]) / 2.0).abs() < 0.000001);
            let mut preferences = AiPreferences::default();
            preferences.enabled = true;
            preferences.disclosed_providers.insert(ProviderId::Openai);
            preferences
                .validated_models
                .insert(ProviderId::Openai, vec!["fixture".into()]);
            preferences.models.insert(
                ProviderId::Openai,
                vec![ModelInfo {
                    id: "fixture".into(),
                    label: "Fixture".into(),
                    context_limit: Some(65536),
                    tool_support: Some(true),
                }],
            );
            SettingsStore::new(&r.store).save(&preferences)?;
            HistoryStore::new(&r.store).create(
                ModelSelection {
                    provider: ProviderId::Openai,
                    model_id: "fixture".into(),
                },
                vec![recording_id],
                "Rough idle in a replay recording".into(),
            )
        })
        .unwrap();
    let transport = Arc::new(support::MockTransport::new(vec![
        HttpReply::Json(
            json!({"status":"completed","output":[{"type":"function_call","call_id":"summary1","name":"get_recording_summary","arguments":json!({"sessionId":session.id}).to_string()}]}),
        ),
        HttpReply::Json(
            json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I found recorded RPM observations. What symptoms occur while the engine is warm?"}]}]}),
        ),
    ]));
    let vault = Arc::new(CredentialVault::new(Arc::new(NoNativeKeys)));
    vault
        .set(
            ProviderId::Openai,
            SecretString::new("synthetic-secret".into()),
            CredentialPersistence::Session,
        )
        .unwrap();
    let mut providers: BTreeMap<ProviderId, Arc<dyn ProviderAdapter>> = BTreeMap::new();
    providers.insert(
        ProviderId::Openai,
        Arc::new(OpenAiProvider::new(transport.clone())),
    );
    let service = AssistantService::new(handle.clone(), vault, providers);
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    service
        .send(
            chat.id.clone(),
            "Inspect the recording".into(),
            Arc::new(move |event| {
                let _ = send.send(event);
            }),
        )
        .await
        .unwrap();
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = receive.recv().await.unwrap();
            if ["completed", "error", "cancelled", "limit"].contains(&event.kind.as_str()) {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(terminal.kind, "completed", "{}", terminal.text);
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let output = requests[1].body.as_ref().unwrap()["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["type"] == "function_call_output")
        .unwrap()["output"]
        .as_str()
        .unwrap();
    let result: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_eq!(result["items"][0]["unit"], "RPM");
    assert_eq!(result["source"], "demo");
    for request in requests.iter() {
        let payload = request.body.as_ref().unwrap().to_string();
        assert!(!payload.contains("synthetic-secret"));
        assert!(!payload.contains("serialPort"));
        assert!(!payload.contains(trace.to_string_lossy().as_ref()));
    }
    drop(requests);
    let id = session.id.clone();
    let chat_id = chat.id.clone();
    handle
        .call(move |r| {
            let preview = r.store.delete_preview(&id)?;
            assert_eq!(preview.conversation_ids, vec![chat_id.clone()]);
            r.store.delete_recording(&id, &preview.revision_token)?;
            assert!(HistoryStore::new(&r.store).get(&chat_id).is_err());
            Ok(())
        })
        .unwrap();
}
