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
pub struct Conversation {pub id:String,pub selection:ModelSelection,pub recording_ids:Vec<String>,pub profile:String,pub title:String,pub created_utc_ms:i64,pub status:String}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ConversationScope {pub conversation_id:String,pub recording_ids:Vec<String>,pub vehicle_id:Option<String>,pub revision:i64}

#[derive(Debug,Clone)]
pub struct TurnLease {pub conversation_id:String,pub turn_id:String,pub revision:i64}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChatMessage {pub role:String,pub content:String,pub extra:Option<serde_json::Value>,pub turn_id:String,pub status:String}
