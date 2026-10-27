use obdium::{
    ai::{providers::ProviderAdapter, *},
    recording::*,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio_util::sync::CancellationToken;
struct UnavailableSecrets;
impl SecretBackend for UnavailableSecrets {
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
struct LoopingProvider {
    calls: AtomicUsize,
    tools_per_response: AtomicUsize,
}
#[async_trait::async_trait]
impl ProviderAdapter for LoopingProvider {
    async fn list_models(
        &self,
        _: &SecretString,
        _: CancellationToken,
    ) -> Result<Vec<ModelInfo>, AiError> {
        Ok(vec![])
    }
    async fn respond(
        &self,
        _: ProviderRequest,
        _: &SecretString,
        _: CancellationToken,
        _: TextSink,
    ) -> Result<ProviderReply, AiError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ProviderReply {
            text: String::new(),
            calls: (0..self.tools_per_response.load(Ordering::SeqCst))
                .map(|i| ToolCall {
                    id: format!("c{n}_{i}"),
                    name: "list_selected_recordings".into(),
                    arguments: json!({}),
                })
                .collect(),
            wire: json!([]),
            usage: TokenUsage::default(),
            truncated: false,
        })
    }
}
fn fixture(enabled: bool) -> (Arc<AssistantService>, String, Arc<LoopingProvider>) {
    let path = std::env::temp_dir().join(format!("obdium-ai-{}.sqlite", uuid::Uuid::new_v4()));
    let handle = RecorderHandle::open(&path).unwrap();
    let id = handle
        .call(move |r| {
            let mut preferences = AiPreferences::default();
            preferences.enabled = enabled;
            preferences
                .validated_models
                .insert(ProviderId::Openai, vec!["fixture".into()]);
            preferences.disclosed_providers.insert(ProviderId::Openai);
            SettingsStore::new(&r.store).save(&preferences)?;
            Ok(HistoryStore::new(&r.store)
                .create(
                    ModelSelection {
                        provider: ProviderId::Openai,
                        model_id: "fixture".into(),
                    },
                    vec![],
                    "Idle problem".into(),
                )?
                .id)
        })
        .unwrap();
    let vault = Arc::new(CredentialVault::new(Arc::new(UnavailableSecrets)));
    vault
        .set(
            ProviderId::Openai,
            SecretString::new("fixture-secret".into()),
            CredentialPersistence::Session,
        )
        .unwrap();
    handle
        .call(|r| {
            let mut preferences = SettingsStore::new(&r.store).load()?;
            preferences.models.insert(
                ProviderId::Openai,
                vec![ModelInfo {
                    id: "fixture".into(),
                    label: "Fixture".into(),
                    context_limit: Some(65536),
                    tool_support: Some(true),
                }],
            );
            SettingsStore::new(&r.store).save(&preferences)
        })
        .unwrap();
    let provider = Arc::new(LoopingProvider {
        calls: AtomicUsize::new(0),
        tools_per_response: AtomicUsize::new(1),
    });
    let mut providers: BTreeMap<ProviderId, Arc<dyn ProviderAdapter>> = BTreeMap::new();
    providers.insert(ProviderId::Openai, provider.clone());
    (
        AssistantService::new(handle, vault, providers),
        id,
        provider,
    )
}
#[tokio::test]
async fn multiple_tool_calls_stop_at_sixteen_executions() {
    let (service, id, provider) = fixture(true);
    provider.tools_per_response.store(3, Ordering::SeqCst);
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    service
        .send(
            id,
            "Investigate".into(),
            Arc::new(move |e| {
                let _ = send.send(e);
            }),
        )
        .await
        .unwrap();
    let executions = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut count = 0;
        loop {
            let event = receive.recv().await.unwrap();
            if event.kind == "tool_activity"
                && event.details.as_ref().unwrap()["result"]["error"].is_null()
            {
                count += 1;
            }
            if ["limit", "completed", "error", "cancelled"].contains(&event.kind.as_str()) {
                break count;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(executions, 16);
    assert!(provider.calls.load(Ordering::SeqCst) <= 8);
}
#[tokio::test]
async fn disabled_ai_makes_no_provider_requests() {
    let (service, id, provider) = fixture(false);
    assert!(service
        .send(id, "Help".into(), Arc::new(|_| {}))
        .await
        .is_err());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn repeated_tools_stop_at_the_turn_limit() {
    let (service, id, provider) = fixture(true);
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    service
        .send(
            id,
            "Investigate".into(),
            Arc::new(move |event| {
                let _ = send.send(event);
            }),
        )
        .await
        .unwrap();
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = receive.recv().await.unwrap();
            if ["completed", "limit", "error", "cancelled"].contains(&event.kind.as_str()) {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(terminal.kind, "limit");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 8);
}
