mod support;
use obdium::ai::{*,providers::{*,transport::*}};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn openai_preserves_reasoning_and_tool_call_ids_without_remote_storage(){
 let wire=json!({"status":"completed","output":[{"type":"reasoning","id":"r1","encrypted_content":"fixture-encrypted"},{"type":"function_call","id":"fc1","call_id":"call1","name":"get_recording_summary","arguments":"{\"sessionId\":\"s\"}"}],"usage":{"input_tokens":10,"output_tokens":5}});
 let transport=Arc::new(support::MockTransport::new(vec![HttpReply::Json(wire)]));let provider=OpenAiProvider::new(transport.clone());
 let reply=provider.respond(support::request(),&SecretString::new("fake-key".into()),CancellationToken::new(),Arc::new(|_|{})).await.unwrap();
 assert_eq!(reply.calls[0].id,"call1");assert_eq!(reply.wire[0]["encrypted_content"],"fixture-encrypted");
 assert_eq!(transport.requests.lock().unwrap()[0].body.as_ref().unwrap()["store"],false);
}
#[tokio::test]
async fn openai_models_follow_pagination(){
 let transport=Arc::new(support::MockTransport::new(vec![HttpReply::Json(json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a"})),HttpReply::Json(json!({"data":[{"id":"b"}],"has_more":false}))]));
 let models=OpenAiProvider::new(transport.clone()).list_models(&SecretString::new("fake".into()),CancellationToken::new()).await.unwrap();
 assert_eq!(models.len(),2);assert_eq!(transport.requests.lock().unwrap().len(),2);
}
