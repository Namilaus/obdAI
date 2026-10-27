mod anthropic;
mod common;
mod deepseek;
mod openai;
pub mod sse;
pub mod transport;
use crate::ai::*;
pub use anthropic::*;
pub use common::validate_model;
pub use deepseek::*;
pub use openai::*;
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn list_models(
        &self,
        secret: &SecretString,
        cancel: CancellationToken,
    ) -> Result<Vec<ModelInfo>, AiError>;
    async fn respond(
        &self,
        request: ProviderRequest,
        secret: &SecretString,
        cancel: CancellationToken,
        sink: TextSink,
    ) -> Result<ProviderReply, AiError>;
}
