pub mod sse;
pub mod transport;
mod common;
mod openai;
pub use openai::*;
pub use common::validate_model;
use crate::ai::*;
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait ProviderAdapter:Send+Sync {
    async fn list_models(&self,secret:&SecretString,cancel:CancellationToken)->Result<Vec<ModelInfo>,AiError>;
    async fn respond(&self,request:ProviderRequest,secret:&SecretString,cancel:CancellationToken,sink:TextSink)->Result<ProviderReply,AiError>;
}
