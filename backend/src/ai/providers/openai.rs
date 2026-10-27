use super::{common::*, transport::*, *};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct OpenAiProvider {
    transport: Arc<dyn HttpTransport>,
}
impl OpenAiProvider {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self { transport }
    }
}

fn input(messages: &[ChatMessage]) -> Result<Vec<Value>, AiError> {
    let mut items = Vec::new();
    for message in messages {
        if let Some(wire) = message
            .extra
            .as_ref()
            .filter(|e| e["kind"] == "provider_reply")
            .and_then(|e| e["wire"].as_array())
        {
            items.extend(wire.clone());
        } else if message.role == "tool" {
            let id = message
                .extra
                .as_ref()
                .and_then(|e| e["callId"].as_str())
                .ok_or_else(|| protocol("Stored tool-result identity is missing."))?;
            items
                .push(json!({"type":"function_call_output","call_id":id,"output":message.content}));
        } else {
            items.push(json!({"role":message.role,"content":message.content}));
        }
    }
    Ok(items)
}
fn parse(value: Value) -> Result<ProviderReply, AiError> {
    let output = value["output"]
        .as_array()
        .ok_or_else(|| protocol("OpenAI response has no complete output."))?;
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut refusal = false;
    for item in output {
        if item["type"] == "function_call" {
            calls.push(parse_call(
                item["call_id"].as_str().unwrap_or_default(),
                item["name"].as_str().unwrap_or_default(),
                item["arguments"].as_str().unwrap_or_default(),
            )?);
        }
        if let Some(content) = item["content"].as_array() {
            for block in content {
                if block["type"] == "output_text" {
                    text.push_str(block["text"].as_str().unwrap_or_default());
                }
                if block["type"] == "refusal" {
                    refusal = true;
                }
            }
        }
    }
    if refusal && text.is_empty() {
        return Err(AiError::new(
            "refusal",
            "The selected model declined this request. Try clarifying the symptoms.",
        ));
    }
    let truncated = value["status"] == "incomplete";
    if truncated && !calls.is_empty() {
        return Err(protocol("OpenAI stopped before its tool calls completed."));
    }
    check_calls(&calls)?;
    Ok(ProviderReply {
        text,
        calls,
        wire: Value::Array(output.clone()),
        usage: TokenUsage {
            input_tokens: value["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: value["usage"]["output_tokens"].as_u64().unwrap_or(0),
        },
        truncated,
    })
}
#[async_trait::async_trait]
impl ProviderAdapter for OpenAiProvider {
    async fn list_models(
        &self,
        secret: &SecretString,
        cancel: CancellationToken,
    ) -> Result<Vec<ModelInfo>, AiError> {
        models(self.transport.as_ref(), ProviderId::Openai, secret, cancel).await
    }
    async fn respond(
        &self,
        request: ProviderRequest,
        secret: &SecretString,
        cancel: CancellationToken,
        sink: TextSink,
    ) -> Result<ProviderReply, AiError> {
        let tools:Vec<_>=request.tools.iter().map(|t|json!({"type":"function","name":t.name,"description":t.description,"parameters":t.parameters,"strict":false})).collect();
        let mut body = json!({"model":request.model,"instructions":request.system,"input":input(&request.messages)?,"tools":tools,"max_output_tokens":request.max_output_tokens,"store":false,"stream":true,"include":["reasoning.encrypted_content"]});
        if let Some(name) = request.forced_tool {
            body["tool_choice"] = json!({"type":"function","name":name});
        }
        let emitted = Arc::new(AtomicBool::new(false));
        let seen = emitted.clone();
        let live = sink.clone();
        let reply = self
            .transport
            .execute(
                HttpRequest {
                    provider: ProviderId::Openai,
                    kind: RequestKind::Generate,
                    body: Some(body),
                },
                secret,
                cancel,
                Arc::new(move |event| {
                    if event["type"] == "response.output_text.delta" {
                        if let Some(text) = event["delta"].as_str() {
                            seen.store(true, Ordering::Relaxed);
                            live(text.into());
                        }
                    }
                }),
            )
            .await?;
        let value = match reply {
            HttpReply::Json(value) => value,
            HttpReply::Stream(events) => {
                if events
                    .iter()
                    .any(|e| e["type"] == "error" || e["type"] == "response.failed")
                {
                    return Err(AiError::new(
                        "provider",
                        "OpenAI stopped this response with an error.",
                    ));
                }
                events
                    .into_iter()
                    .rev()
                    .find(|e| {
                        e["type"] == "response.completed" || e["type"] == "response.incomplete"
                    })
                    .map(|e| e["response"].clone())
                    .ok_or_else(|| protocol("OpenAI stream ended before a completed response."))?
            }
        };
        let result = parse(value)?;
        if !emitted.load(Ordering::Relaxed) && !result.text.is_empty() {
            sink(result.text.clone());
        }
        Ok(result)
    }
}
