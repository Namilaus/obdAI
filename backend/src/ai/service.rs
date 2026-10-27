use super::*;
use crate::recording::*;
use providers::{transport::*, *};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEvent {
    pub conversation_id: String,
    pub turn_id: String,
    pub kind: String,
    pub text: String,
    pub details: Option<Value>,
}
pub type ChatEventSink = Arc<dyn Fn(ChatEvent) + Send + Sync>;
struct ActiveTurn {
    conversation_id: String,
    provider: ProviderId,
    cancel: CancellationToken,
}
pub struct AssistantService {
    pub recorder: RecorderHandle,
    pub vault: Arc<CredentialVault>,
    providers: BTreeMap<ProviderId, Arc<dyn ProviderAdapter>>,
    active: Mutex<BTreeMap<String, ActiveTurn>>,
}
impl AssistantService {
    pub fn new(
        recorder: RecorderHandle,
        vault: Arc<CredentialVault>,
        providers: BTreeMap<ProviderId, Arc<dyn ProviderAdapter>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            recorder,
            vault,
            providers,
            active: Mutex::new(BTreeMap::new()),
        })
    }
    pub fn network(recorder: RecorderHandle) -> Result<Arc<Self>, AiError> {
        let transport = Arc::new(NetworkTransport::new()?);
        let mut providers: BTreeMap<ProviderId, Arc<dyn ProviderAdapter>> = BTreeMap::new();
        providers.insert(
            ProviderId::Openai,
            Arc::new(OpenAiProvider::new(transport.clone())),
        );
        providers.insert(
            ProviderId::Anthropic,
            Arc::new(AnthropicProvider::new(transport.clone())),
        );
        providers.insert(
            ProviderId::Deepseek,
            Arc::new(DeepSeekProvider::new(transport)),
        );
        Ok(Self::new(
            recorder,
            Arc::new(CredentialVault::default()),
            providers,
        ))
    }
    pub fn adapter(&self, provider: ProviderId) -> Result<Arc<dyn ProviderAdapter>, AiError> {
        self.providers
            .get(&provider)
            .cloned()
            .ok_or_else(|| AiError::new("provider", "This provider is unavailable."))
    }
    pub async fn database<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Recorder) -> Result<T, AiError> + Send + 'static,
    ) -> Result<T, AiError> {
        let handle = self.recorder.clone();
        tokio::task::spawn_blocking(move || handle.call(work))
            .await
            .map_err(|_| AiError::new("storage", "Local history request was interrupted."))?
    }
    pub async fn read<T: Send + 'static>(
        &self,
        work: impl FnOnce(&RecordingStore) -> Result<T, AiError> + Send + 'static,
    ) -> Result<T, AiError> {
        let path = self.recorder.path.clone();
        tokio::task::spawn_blocking(move || {
            let store = RecordingStore::open_reader(&path)?;
            work(&store)
        })
        .await
        .map_err(|_| AiError::new("storage", "Local history query was interrupted."))?
    }
    pub fn cancel_turn(&self, id: &str) {
        if let Some(active) = self.active.lock().unwrap().get(id) {
            active.cancel.cancel();
        }
    }
    pub fn cancel_conversation(&self, id: &str) {
        for active in self.active.lock().unwrap().values() {
            if active.conversation_id == id {
                active.cancel.cancel();
            }
        }
    }
    pub fn cancel_provider(&self, provider: ProviderId) {
        for active in self.active.lock().unwrap().values() {
            if active.provider == provider {
                active.cancel.cancel();
            }
        }
    }
    pub fn cancel_all(&self) {
        for active in self.active.lock().unwrap().values() {
            active.cancel.cancel();
        }
    }
    pub fn register_auxiliary(&self, provider: ProviderId) -> (String, CancellationToken) {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        self.active.lock().unwrap().insert(
            id.clone(),
            ActiveTurn {
                conversation_id: String::new(),
                provider,
                cancel: cancel.clone(),
            },
        );
        (id, cancel)
    }
    pub fn finish_auxiliary(&self, id: &str) {
        self.active.lock().unwrap().remove(id);
    }
    pub async fn send(
        self: &Arc<Self>,
        id: String,
        text: String,
        sink: ChatEventSink,
    ) -> Result<String, AiError> {
        let read_id = id.clone();
        let (chat, preferences) = self
            .read(move |store| {
                Ok((
                    HistoryStore::new(store).get(&read_id)?,
                    SettingsStore::new(store).load()?,
                ))
            })
            .await?;
        Self::eligible(&chat, &preferences)?;
        let provider = chat.selection.provider;
        let revision = self.vault.revision(provider);
        let vault = self.vault.clone();
        let secret = tokio::task::spawn_blocking(move || vault.get(provider))
            .await
            .map_err(|_| AiError::new("auth", "Credential request was interrupted."))??;
        if self.vault.revision(provider) != revision {
            return Err(AiError::new("cancelled", "Provider credentials changed."));
        }
        let lease = self
            .database(move |r| {
                let preferences = SettingsStore::new(&r.store).load()?;
                let history = HistoryStore::new(&r.store);
                let chat = history.get(&id)?;
                Self::eligible(&chat, &preferences)?;
                history.begin_turn(&id, text)
            })
            .await?;
        let cancel = CancellationToken::new();
        let turn_id = lease.turn_id.clone();
        self.active.lock().unwrap().insert(
            turn_id.clone(),
            ActiveTurn {
                conversation_id: lease.conversation_id.clone(),
                provider,
                cancel: cancel.clone(),
            },
        );
        let service = self.clone();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(180),
                service.run_lease(
                    &lease,
                    &secret,
                    provider,
                    revision,
                    cancel.clone(),
                    sink.clone(),
                ),
            )
            .await;
            let (mut status, mut error) = match result {
                Ok(Ok(status)) => (status, None),
                Ok(Err(error)) => (
                    if error.code == "cancelled" {
                        "cancelled"
                    } else {
                        "error"
                    }
                    .into(),
                    Some(error),
                ),
                Err(_) => {
                    cancel.cancel();
                    ("error".into(),Some(AiError::new("timeout","This investigation timed out. Available results are saved; retry only when appropriate.")))
                }
            };
            let finish_lease = lease.clone();
            let finish_status = status.clone();
            let saved = service
                .database(move |r| {
                    let history = HistoryStore::new(&r.store);
                    history.ensure_lease(&finish_lease)?;
                    history.finish(&finish_lease, &finish_status)
                })
                .await;
            if let Err(e) = saved {
                status = if ["cancelled", "deleted"].contains(&e.code) {
                    "cancelled"
                } else {
                    "error"
                }
                .into();
                error = Some(e);
            }
            service.active.lock().unwrap().remove(&lease.turn_id);
            sink(ChatEvent {
                conversation_id: lease.conversation_id,
                turn_id: lease.turn_id,
                kind: status,
                text: error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_default(),
                details: error.map(|e| json!({"code":e.code})),
            });
        });
        Ok(turn_id)
    }
    fn eligible(chat: &Conversation, prefs: &AiPreferences) -> Result<(), AiError> {
        if !prefs.enabled {
            return Err(AiError::new(
                "ai_disabled",
                "Enable AI in Settings to send a message.",
            ));
        }
        if !prefs.disclosed_providers.contains(&chat.selection.provider) {
            return Err(AiError::new(
                "disclosure_required",
                "Review and accept data sharing for this provider before sending.",
            ));
        }
        if !prefs
            .validated_models
            .get(&chat.selection.provider)
            .map(|ids| ids.contains(&chat.selection.model_id))
            .unwrap_or(false)
        {
            return Err(AiError::new(
                "model_not_tested",
                "Test this model's tool compatibility in Settings before using it.",
            ));
        }
        Ok(())
    }
    async fn turn_context(
        &self,
        lease: &TurnLease,
    ) -> Result<(ProviderRequest, ConversationScope, u32), AiError> {
        let lease = lease.clone();
        self.read(move |store| {
            let history = HistoryStore::new(store);
            history.ensure_lease(&lease)?;
            let chat = history.get(&lease.conversation_id)?;
            let preferences = SettingsStore::new(store).load()?;
            Self::eligible(&chat, &preferences)?;
            let limit = preferences
                .models
                .get(&chat.selection.provider)
                .and_then(|models| models.iter().find(|m| m.id == chat.selection.model_id))
                .and_then(|m| m.context_limit)
                .unwrap_or(8192)
                .clamp(2048, 1_000_000);
            Ok((
                ContextBuilder::build(&chat, history.messages(&chat.id)?, limit)?,
                history.scope(&chat.id)?,
                limit,
            ))
        })
        .await
    }
    async fn append(
        &self,
        lease: &TurnLease,
        role: &str,
        text: String,
        extra: Option<Value>,
    ) -> Result<(), AiError> {
        let lease = lease.clone();
        let role = role.to_string();
        self.database(move |r| HistoryStore::new(&r.store).append(&lease, &role, &text, extra))
            .await
    }
    async fn limit(&self, lease: &TurnLease, sink: &ChatEventSink) -> Result<String, AiError> {
        let text="The investigation reached its request or context limit. The observations gathered so far are saved. Ask a follow-up question, or start a new conversation with a smaller selection.".to_string();
        self.append(lease, "assistant", text.clone(), None).await?;
        sink(ChatEvent {
            conversation_id: lease.conversation_id.clone(),
            turn_id: lease.turn_id.clone(),
            kind: "text_delta".into(),
            text,
            details: None,
        });
        Ok("limit".into())
    }
    async fn run_lease(
        &self,
        lease: &TurnLease,
        secret: &SecretString,
        provider: ProviderId,
        revision: u64,
        cancel: CancellationToken,
        sink: ChatEventSink,
    ) -> Result<String, AiError> {
        let adapter = self.adapter(provider)?;
        let mut tool_count = 0;
        let mut usage = TokenUsage::default();
        for round in 0..8 {
            if cancel.is_cancelled() || self.vault.revision(provider) != revision {
                return Err(AiError::new("cancelled", "Provider request cancelled."));
            }
            let (mut request, _, _) = match self.turn_context(lease).await {
                Ok(c) => c,
                Err(e) if e.code == "context_limit" => return self.limit(lease, &sink).await,
                Err(e) => return Err(e),
            };
            if round == 7 || tool_count >= 16 {
                request.tools.clear();
                request.system.push_str("\nNo retrieval calls remain in this turn. Give a partial conclusion using the evidence already obtained and identify what is still unknown.");
            }
            let event_lease = lease.clone();
            let live = sink.clone();
            let token = cancel.clone();
            let text_sink: TextSink = Arc::new(move |text| {
                if !token.is_cancelled() {
                    live(ChatEvent {
                        conversation_id: event_lease.conversation_id.clone(),
                        turn_id: event_lease.turn_id.clone(),
                        kind: "text_delta".into(),
                        text,
                        details: None,
                    });
                }
            });
            let reply = tokio::select! {biased;_=cancel.cancelled()=>return Err(AiError::new("cancelled","Provider request cancelled.")),reply=adapter.respond(request,secret,cancel.clone(),text_sink)=>reply?};
            if cancel.is_cancelled() || self.vault.revision(provider) != revision {
                return Err(AiError::new(
                    "cancelled",
                    "Provider credentials or request changed.",
                ));
            }
            usage.input_tokens += reply.usage.input_tokens;
            usage.output_tokens += reply.usage.output_tokens;
            self.append(
                lease,
                "assistant",
                reply.text.clone(),
                Some(json!({"kind":"provider_reply","wire":reply.wire,"usage":reply.usage})),
            )
            .await?;
            sink(ChatEvent {
                conversation_id: lease.conversation_id.clone(),
                turn_id: lease.turn_id.clone(),
                kind: "usage".into(),
                text: String::new(),
                details: Some(serde_json::to_value(&usage)?),
            });
            if reply.calls.is_empty() {
                if reply.text.trim().is_empty() {
                    return Err(AiError::new(
                        "provider",
                        "The selected model returned no diagnostic response.",
                    ));
                }
                return Ok(if reply.truncated {
                    "limit"
                } else {
                    "completed"
                }
                .into());
            }
            if round == 7 || tool_count >= 16 {
                return self.limit(lease, &sink).await;
            }
            let count = reply.calls.len();
            for (index, call) in reply.calls.into_iter().enumerate() {
                if cancel.is_cancelled() {
                    return Err(AiError::new("cancelled", "Request cancelled."));
                }
                let (context, scope, limit) = match self.turn_context(lease).await {
                    Ok(c) => c,
                    Err(e) if e.code == "context_limit" => return self.limit(lease, &sink).await,
                    Err(e) => return Err(e),
                };
                let available = (limit as usize)
                    .saturating_sub(context.max_output_tokens as usize)
                    .saturating_sub(ContextBuilder::estimate(&context))
                    .saturating_sub(256);
                let budget = (available / (count - index).max(1)).min(64 * 1024);
                let result = if tool_count >= 16 || budget < 512 {
                    Err(AiError::new("context_limit","Retrieval budget exhausted. Use the available evidence and explain what is missing."))
                } else {
                    tool_count += 1;
                    let tool = call.clone();
                    let token = cancel.clone();
                    self.read(move |store| {
                        ToolRouter::execute_cancellable(store, &scope, tool, budget, token)
                    })
                    .await
                };
                if let Err(e) = &result {
                    if ["cancelled", "deleted"].contains(&e.code) {
                        return Err(e.clone());
                    }
                }
                let payload = match result {
                    Ok(value) => value,
                    Err(e) => json!({"error":{"code":e.code,"message":e.message}}),
                };
                self.append(
                    lease,
                    "tool",
                    payload.to_string(),
                    Some(json!({"kind":"tool_result","callId":call.id,"name":call.name})),
                )
                .await?;
                sink(ChatEvent {
                    conversation_id: lease.conversation_id.clone(),
                    turn_id: lease.turn_id.clone(),
                    kind: "tool_activity".into(),
                    text: call.name,
                    details: Some(json!({"arguments":call.arguments,"result":payload})),
                });
            }
        }
        self.limit(lease, &sink).await
    }
}
