mod support;
use obdium::ai::{
    providers::{transport::*, *},
    *,
};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn claude_assembles_complete_tools_and_preserves_thinking_signatures() {
    let events = vec![
        json!({"type":"message_start","message":{"usage":{"input_tokens":12}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call1","name":"get_recording_summary","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"sessionId\":"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"s\"}"}}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}}),
        json!({"type":"message_stop"}),
    ];
    let transport = Arc::new(support::MockTransport::new(vec![HttpReply::Stream(events)]));
    let reply = AnthropicProvider::new(transport)
        .respond(
            support::request(),
            &SecretString::new("fake".into()),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
    assert_eq!(reply.calls[0].arguments["sessionId"], "s");
    assert_eq!(reply.wire["content"][0]["signature"], "signed");
}
#[tokio::test]
async fn claude_rejects_an_interrupted_tool_stream() {
    let transport = Arc::new(support::MockTransport::new(vec![HttpReply::Stream(vec![
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"c","name":"get_recording_summary","input":{}}}),
    ])]));
    assert!(AnthropicProvider::new(transport)
        .respond(
            support::request(),
            &SecretString::new("fake".into()),
            CancellationToken::new(),
            Arc::new(|_| {})
        )
        .await
        .is_err());
}
