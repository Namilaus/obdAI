use super::{*,common::*,transport::*};
use serde_json::{Value,json};
use std::{collections::BTreeMap,sync::{Arc,atomic::{AtomicBool,Ordering}}};

pub struct DeepSeekProvider{transport:Arc<dyn HttpTransport>}
impl DeepSeekProvider{pub fn new(transport:Arc<dyn HttpTransport>)->Self{Self{transport}}}
fn input(system:&str,messages:&[ChatMessage])->Result<Vec<Value>,AiError>{
    let mut out=vec![json!({"role":"system","content":system})];
    for message in messages{
        if let Some(wire)=message.extra.as_ref().filter(|e|e["kind"]=="provider_reply").map(|e|e["wire"].clone()){out.push(wire);}
        else if message.role=="tool"{let id=message.extra.as_ref().and_then(|e|e["callId"].as_str()).ok_or_else(||protocol("Stored DeepSeek tool-result identity is missing."))?;out.push(json!({"role":"tool","tool_call_id":id,"content":message.content}));}
        else{out.push(json!({"role":message.role,"content":message.content}));}
    }Ok(out)
}
fn assemble(events:Vec<Value>)->Result<Value,AiError>{
    let mut text=String::new();let mut reasoning=String::new();let mut tools:BTreeMap<usize,Value>=BTreeMap::new();let mut usage=Value::Null;let mut finish=Value::Null;
    for event in events{
        if !event["error"].is_null(){return Err(AiError::new("provider","DeepSeek stopped this response with an error."));}
        if !event["usage"].is_null(){usage=event["usage"].clone();}
        let Some(choice)=event["choices"].as_array().and_then(|c|c.first())else{continue;};let delta=&choice["delta"];
        if let Some(s)=delta["content"].as_str(){text.push_str(s);}if let Some(s)=delta["reasoning_content"].as_str(){reasoning.push_str(s);}
        if let Some(calls)=delta["tool_calls"].as_array(){for call in calls{
            let index=call["index"].as_u64().ok_or_else(||protocol("DeepSeek tool index missing."))? as usize;if index>=16{return Err(protocol("DeepSeek returned too many tool calls."));}
            let entry=tools.entry(index).or_insert_with(||json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
            if let Some(id)=call["id"].as_str(){entry["id"]=json!(id);}
            for field in ["name","arguments"]{if let Some(fragment)=call["function"][field].as_str(){let mut value=entry["function"][field].as_str().unwrap_or_default().to_string();value.push_str(fragment);entry["function"][field]=json!(value);}}
        }}
        if !choice["finish_reason"].is_null(){finish=choice["finish_reason"].clone();}
    }
    if finish.is_null(){return Err(protocol("DeepSeek stream ended before a completed response."));}
    let mut message=json!({"role":"assistant","content":text});if !reasoning.is_empty(){message["reasoning_content"]=json!(reasoning);}if !tools.is_empty(){message["tool_calls"]=json!(tools.into_values().collect::<Vec<_>>());}
    Ok(json!({"choices":[{"message":message,"finish_reason":finish}],"usage":usage}))
}
fn parse(value:Value)->Result<ProviderReply,AiError>{
    let choice=value["choices"].as_array().and_then(|c|c.first()).ok_or_else(||protocol("DeepSeek returned no complete choice."))?;let wire=choice["message"].clone();
    if !wire.is_object(){return Err(protocol("DeepSeek returned no complete assistant message."));}
    let text=wire["content"].as_str().unwrap_or_default().to_string();let mut calls=Vec::new();
    if let Some(items)=wire["tool_calls"].as_array(){for call in items{calls.push(parse_call(call["id"].as_str().unwrap_or_default(),call["function"]["name"].as_str().unwrap_or_default(),call["function"]["arguments"].as_str().unwrap_or_default())?);}}
    let truncated=choice["finish_reason"]=="length";if truncated&&!calls.is_empty(){return Err(protocol("DeepSeek stopped before its tool calls completed."));}check_calls(&calls)?;
    Ok(ProviderReply{text,calls,wire,usage:TokenUsage{input_tokens:value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),output_tokens:value["usage"]["completion_tokens"].as_u64().unwrap_or(0)},truncated})
}
#[async_trait::async_trait]
impl ProviderAdapter for DeepSeekProvider{
    async fn list_models(&self,secret:&SecretString,cancel:CancellationToken)->Result<Vec<ModelInfo>,AiError>{models(self.transport.as_ref(),ProviderId::Deepseek,secret,cancel).await}
    async fn respond(&self,request:ProviderRequest,secret:&SecretString,cancel:CancellationToken,sink:TextSink)->Result<ProviderReply,AiError>{
        let tools:Vec<_>=request.tools.iter().map(|t|json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}})).collect();
        let mut body=json!({"model":request.model,"messages":input(&request.system,&request.messages)?,"tools":tools,"max_tokens":request.max_output_tokens,"stream":true,"stream_options":{"include_usage":true},"thinking":{"type":"disabled"}});
        if let Some(name)=request.forced_tool{body["tool_choice"]=json!({"type":"function","function":{"name":name}});}
        let emitted=Arc::new(AtomicBool::new(false));let seen=emitted.clone();let live=sink.clone();
        let reply=self.transport.execute(HttpRequest{provider:ProviderId::Deepseek,kind:RequestKind::Generate,body:Some(body)},secret,cancel,Arc::new(move|e|{if let Some(text)=e.pointer("/choices/0/delta/content").and_then(Value::as_str){seen.store(true,Ordering::Relaxed);live(text.into());}})).await?;
        let value=match reply{HttpReply::Json(v)=>v,HttpReply::Stream(events)=>assemble(events)?};let result=parse(value)?;
        if !emitted.load(Ordering::Relaxed)&&!result.text.is_empty(){sink(result.text.clone());}Ok(result)
    }
}
