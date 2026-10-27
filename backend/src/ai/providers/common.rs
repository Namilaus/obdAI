use super::{transport::*, *};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};

pub fn protocol(message: &str) -> AiError {
    AiError::new("protocol", message)
}
pub fn parse_call(id: &str, name: &str, arguments: &str) -> Result<ToolCall, AiError> {
    let arguments: Value = serde_json::from_str(arguments)
        .map_err(|_| protocol("Provider returned incomplete or invalid tool arguments."))?;
    if id.is_empty()
        || id.len() > 256
        || name.is_empty()
        || name.len() > 128
        || !arguments.is_object()
    {
        return Err(protocol("Provider returned an invalid tool call."));
    }
    Ok(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    })
}
pub fn check_calls(calls: &[ToolCall]) -> Result<(), AiError> {
    let ids: BTreeSet<_> = calls.iter().map(|c| &c.id).collect();
    if ids.len() != calls.len() || calls.len() > 16 {
        return Err(protocol(
            "Provider returned duplicate or excessive tool calls.",
        ));
    }
    Ok(())
}
pub async fn models(
    transport: &dyn HttpTransport,
    provider: ProviderId,
    secret: &SecretString,
    cancel: CancellationToken,
) -> Result<Vec<ModelInfo>, AiError> {
    let mut out = Vec::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    for _ in 0..32 {
        let reply = transport
            .execute(
                HttpRequest {
                    provider,
                    kind: RequestKind::Models {
                        cursor: cursor.clone(),
                    },
                    body: None,
                },
                secret,
                cancel.clone(),
                Arc::new(|_| {}),
            )
            .await?;
        let HttpReply::Json(value) = reply else {
            return Err(protocol("Model discovery returned an unexpected stream."));
        };
        let entries = value["data"]
            .as_array()
            .ok_or_else(|| protocol("Provider model list was invalid."))?;
        for entry in entries {
            let Some(id) = entry["id"].as_str() else {
                continue;
            };
            if id.len() > 256 {
                continue;
            }
            if seen.insert(id.to_string()) {
                out.push(ModelInfo {
                    id: id.into(),
                    label: entry["display_name"]
                        .as_str()
                        .or_else(|| entry["name"].as_str())
                        .unwrap_or(id)
                        .into(),
                    context_limit: entry["context_window"]
                        .as_u64()
                        .or_else(|| entry["max_input_tokens"].as_u64())
                        .and_then(|v| v.try_into().ok()),
                    tool_support: entry
                        .pointer("/capabilities/tool_calling")
                        .and_then(Value::as_bool),
                });
            }
        }
        if out.len() > 2000 {
            return Err(protocol("Provider model list exceeded its limit."));
        }
        if value["has_more"] != true {
            return Ok(out);
        }
        let next = value["last_id"]
            .as_str()
            .or_else(|| entries.last().and_then(|v| v["id"].as_str()))
            .map(str::to_string);
        if next.is_none() || next == cursor {
            return Err(protocol("Provider model pagination did not advance."));
        }
        cursor = next;
    }
    Err(protocol("Provider model pagination exceeded its limit."))
}

pub async fn validate_model(
    adapter: &dyn ProviderAdapter,
    model: String,
    secret: &SecretString,
    cancel: CancellationToken,
) -> Result<TokenUsage, AiError> {
    let tool = ToolDefinition {
        name: "obdium_test".into(),
        description: "Return a synthetic connection-test result. No vehicle data is available."
            .into(),
        parameters: json!({"type":"object","properties":{},"additionalProperties":false,"required":[]}),
    };
    let mut request=ProviderRequest{model,system:"This is a synthetic API compatibility test. Call obdium_test once, then confirm the result in a short sentence.".into(),messages:vec![ChatMessage{role:"user".into(),content:"Test the tool connection.".into(),extra:None,turn_id:"synthetic".into(),status:"pending".into()}],tools:vec![tool],forced_tool:Some("obdium_test".into()),max_output_tokens:512};
    let first = adapter
        .respond(request.clone(), secret, cancel.clone(), Arc::new(|_| {}))
        .await?;
    if first.calls.len() != 1
        || first.calls[0].name != "obdium_test"
        || first.calls[0].arguments != json!({})
    {
        return Err(AiError::new(
            "unsupported_model",
            "This model did not complete the required synthetic tool call.",
        ));
    }
    request.messages.push(ChatMessage {
        role: "assistant".into(),
        content: first.text.clone(),
        extra: Some(json!({"kind":"provider_reply","wire":first.wire})),
        turn_id: "synthetic".into(),
        status: "pending".into(),
    });
    request.messages.push(ChatMessage {
        role: "tool".into(),
        content: "{\"ok\":true}".into(),
        extra: Some(json!({"kind":"tool_result","callId":first.calls[0].id})),
        turn_id: "synthetic".into(),
        status: "pending".into(),
    });
    request.forced_tool = None;
    request.tools.clear();
    let second = adapter
        .respond(request, secret, cancel, Arc::new(|_| {}))
        .await?;
    if !second.calls.is_empty() || second.text.trim().is_empty() || second.truncated {
        return Err(AiError::new(
            "unsupported_model",
            "This model did not finish the synthetic tool-result round trip.",
        ));
    }
    Ok(TokenUsage {
        input_tokens: first.usage.input_tokens + second.usage.input_tokens,
        output_tokens: first.usage.output_tokens + second.usage.output_tokens,
    })
}
