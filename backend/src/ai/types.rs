use serde::{Deserialize,Serialize};
pub type AiError=crate::recording::RecordingError;

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum ProviderId {Openai,Anthropic,Deepseek}
impl ProviderId { pub fn key(self)->&'static str {match self{Self::Openai=>"openai",Self::Anthropic=>"anthropic",Self::Deepseek=>"deepseek"}} }

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ModelSelection {pub provider:ProviderId,pub model_id:String}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Conversation {pub id:String,pub selection:ModelSelection,pub recording_ids:Vec<String>,pub profile:String,pub title:String,pub created_utc_ms:i64,pub status:String,pub active_turn:Option<String>}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ConversationScope {pub conversation_id:String,pub recording_ids:Vec<String>,pub vehicle_id:Option<String>,pub revision:i64}

#[derive(Debug,Clone)]
pub struct TurnLease {pub conversation_id:String,pub turn_id:String,pub revision:i64}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChatMessage {pub role:String,pub content:String,pub extra:Option<serde_json::Value>,pub turn_id:String,pub status:String}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ModelInfo {pub id:String,pub label:String,pub context_limit:Option<u32>,pub tool_support:Option<bool>}

#[derive(Debug,Clone,Default,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase")]
pub struct AiPreferences {
    pub enabled:bool,
    pub default_selection:Option<ModelSelection>,
    pub selections:std::collections::BTreeMap<ProviderId,String>,
    pub models:std::collections::BTreeMap<ProviderId,Vec<ModelInfo>>,
    pub validated_models:std::collections::BTreeMap<ProviderId,Vec<String>>,
    pub disclosed_providers:std::collections::BTreeSet<ProviderId>,
}

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ToolDefinition {pub name:String,pub description:String,pub parameters:serde_json::Value}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ToolCall {pub id:String,pub name:String,pub arguments:serde_json::Value}
#[derive(Debug,Clone,Serialize,Deserialize,Default)]
pub struct TokenUsage {pub input_tokens:u64,pub output_tokens:u64}
#[derive(Debug,Clone)]
pub struct ProviderRequest {
    pub model:String,pub system:String,pub messages:Vec<ChatMessage>,pub tools:Vec<ToolDefinition>,
    pub forced_tool:Option<String>,pub max_output_tokens:u32,
}
#[derive(Debug,Clone)]
pub struct ProviderReply {pub text:String,pub calls:Vec<ToolCall>,pub wire:serde_json::Value,pub usage:TokenUsage,pub truncated:bool}
pub type TextSink=std::sync::Arc<dyn Fn(String)+Send+Sync>;
