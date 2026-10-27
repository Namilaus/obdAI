mod support;
use obdium::ai::{
    providers::{transport::*, *},
    *,
};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn deepseek_assembles_argument_fragments_and_retains_reasoning() {
    let events = vec![
        json!({"choices":[{"delta":{"reasoning_content":"private","tool_calls":[{"index":0,"id":"call1","type":"function","function":{"name":"get_recording_summary","arguments":"{\"sessionId\":"}}]},"finish_reason":null}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"s\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":20}}),
    ];
    let transport = Arc::new(support::MockTransport::new(vec![HttpReply::Stream(events)]));
    let reply = DeepSeekProvider::new(transport)
        .respond(
            support::request(),
            &SecretString::new("fake".into()),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
    assert_eq!(reply.calls[0].id, "call1");
    assert_eq!(reply.calls[0].arguments["sessionId"], "s");
    assert_eq!(reply.wire["reasoning_content"], "private");
}
#[tokio::test]
async fn deepseek_rejects_invalid_completed_arguments() {
    let reply = json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"c","function":{"name":"get_recording_summary","arguments":"{"}}]},"finish_reason":"tool_calls"}]});
    let transport = Arc::new(support::MockTransport::new(vec![HttpReply::Json(reply)]));
    assert!(DeepSeekProvider::new(transport)
        .respond(
            support::request(),
            &SecretString::new("fake".into()),
            CancellationToken::new(),
            Arc::new(|_| {})
        )
        .await
        .is_err());
}
